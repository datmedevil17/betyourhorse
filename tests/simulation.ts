import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { PublicKey } from "@solana/web3.js";
import { Horsebetting } from "../target/types/horsebetting";
import { randomBytes } from "crypto";
import {
    awaitComputationFinalization,
    getArciumEnv,
    getCompDefAccOffset,
    getArciumAccountBaseSeed,
    getArciumProgramId,
    getArciumProgram,
    uploadCircuit,
    RescueCipher,
    deserializeLE,
    getMXEPublicKey,
    getMXEAccAddress,
    getMempoolAccAddress,
    getCompDefAccAddress,
    getExecutingPoolAccAddress,
    getComputationAccAddress,
    getClusterAccAddress,
    getLookupTableAddress,
    x25519,
} from "@arcium-hq/client";
import * as fs from "fs";
import * as os from "os";
import { expect } from "chai";

describe("Horse Betting Simulation", () => {
    anchor.setProvider(anchor.AnchorProvider.env());
    const program = anchor.workspace.Horsebetting as Program<Horsebetting>;
    const provider = anchor.getProvider() as anchor.AnchorProvider;
    const arciumProgram = getArciumProgram(provider);
    const arciumEnv = getArciumEnv();

    // Accounts
    let racePda: PublicKey;
    let horsePdas: PublicKey[] = [];
    const raceId = new anchor.BN(randomBytes(8));

    // Arcium Setup
    let cipher: RescueCipher;
    let mxePublicKey: Uint8Array;

    it("Initialized Arcium & Circuit", async () => {
        const owner = readKpJson(`${os.homedir()}/.config/solana/id.json`); // Assuming default wallet
        console.log("Initializing simulate_race computation definition...");

        // Check if MXE exists first to avoid re-init errors if re-running
        // But initSimulateRaceCompDef is needed for CompDef account

        try {
            await initSimulateRaceCompDef(program, owner);
            console.log("Simulate Race Comp Def initialized.");
        } catch (e) {
            console.log("Comp Def might already be initialized or error:", e);
        }

        // Setup Cipher
        mxePublicKey = await getMXEPublicKeyWithRetry(provider, program.programId);
        const privateKey = x25519.utils.randomSecretKey();
        const sharedSecret = x25519.getSharedSecret(privateKey, mxePublicKey);
        cipher = new RescueCipher(sharedSecret);
    });

    it("Creates Horses", async () => {
        // Create 4 horses
        for (let i = 0; i < 4; i++) {
            const [horsePda] = PublicKey.findProgramAddressSync(
                [Buffer.from("horse"), provider.wallet.publicKey.toBuffer(), Buffer.from([i])],
                program.programId
            );
            horsePdas.push(horsePda);

            // Attributes: mix of speed/stamina
            const speed = 10 + i * 5; // 10, 15, 20, 25
            const stamina = 20 - i * 2; // 20, 18, 16, 14

            await program.methods
                .createHorse(i, speed, stamina, `Horse ${i}`)
                .accounts({
                    horse: horsePda,
                    payer: provider.wallet.publicKey,
                })
                .rpc();
            console.log(`Created Horse ${i} at ${horsePda.toBase58()}`);
        }
    });

    it("Creates Race", async () => {
        [racePda] = PublicKey.findProgramAddressSync(
            [Buffer.from("race"), raceId.toArrayLike(Buffer, 'le', 8)],
            program.programId
        );

        await program.methods
            .createRace(raceId, [horsePdas[0], horsePdas[1], horsePdas[2], horsePdas[3]])
            .accounts({
                race: racePda,
                payer: provider.wallet.publicKey,
            })
            .rpc();
        console.log(`Created Race at ${racePda.toBase58()}`);
    });

    it("Places Bets", async () => {
        // Encrypt bets
        const amounts = [100, 200, 300, 400];
        const selections = [0, 1, 2, 3]; // Each bets on a different horse

        for (let i = 0; i < 4; i++) {
            // Encrypt selection (horse_id)
            // Arcium client encrypts array of BigInts.
            // We want to encrypt a single u8.
            const plaintext = [BigInt(selections[i])];
            const nonce = randomBytes(16);
            const ciphertext = cipher.encrypt(plaintext, nonce);
            // ciphertext is BigInt[][]. We want the first field's ciphertext.
            // ciphertext[0] is array of bytes? No, usually BigInts or bytes depending on client version.
            // In previous test it used: Array.from(ciphertext[0])
            // Let's assume ciphertext returns array of byte-arrays?
            // Wait, 'add_together' test used: Array.from(ciphertext[0]).
            // And ciphertext was result of cipher.encrypt(plaintext, nonce).
            // Let's check imports. 'RescueCipher'.

            // The contract expects [u8; 32].
            // So we pass Array.from(ciphertext[0]).

            await program.methods
                .placeBet(new anchor.BN(amounts[i]), Array.from(ciphertext[0]) as any)
                .accounts({
                    race: racePda,
                    bettor: provider.wallet.publicKey, // Same bettor for simplicity
                })
                .rpc();
            console.log(`Placed bet on Horse ${selections[i]} with amount ${amounts[i]}`);
        }
    });

    it("Runs Race and Verifies Result", async () => {
        const computationOffset = new anchor.BN(randomBytes(8), "hex");
        const seed = new anchor.BN(randomBytes(8));
        const clusterAccount = getClusterAccAddress(arciumEnv.arciumClusterOffset);

        console.log("Queueing race simulation...");
        const tx = await program.methods
            .runRace(computationOffset, seed)
            .accountsPartial({
                computationAccount: getComputationAccAddress(
                    arciumEnv.arciumClusterOffset,
                    computationOffset
                ),
                clusterAccount,
                mxeAccount: getMXEAccAddress(program.programId),
                mempoolAccount: getMempoolAccAddress(arciumEnv.arciumClusterOffset),
                executingPool: getExecutingPoolAccAddress(arciumEnv.arciumClusterOffset),
                compDefAccount: getCompDefAccAddress(
                    program.programId,
                    Buffer.from(getCompDefAccOffset("simulate_race")).readUInt32LE()
                ),
                race: racePda,
                horse1: horsePdas[0],
                horse2: horsePdas[1],
                horse3: horsePdas[2],
                horse4: horsePdas[3],
            })
            .rpc({ skipPreflight: true, commitment: "confirmed" });

        console.log("Race queued. Sig:", tx);

        console.log("Waiting for finalization...");
        await awaitComputationFinalization(
            provider,
            computationOffset,
            program.programId,
            "confirmed"
        );
        console.log("Computation finalized.");

        // Check Race State
        const raceAccount = await program.account.race.fetch(racePda);
        console.log("Race Result:", {
            winner: raceAccount.winnerId,
            payouts: raceAccount.payouts.map(p => p.toString()),
            status: raceAccount.status
        });

        expect(raceAccount.status).to.deep.equal({ completed: {} });
        // Verify winner is valid (0-3)
        expect(raceAccount.winnerId >= 0 && raceAccount.winnerId < 4).to.be.true;

        // Verify payouts
        // Winner should have > 0 payout unless logic failed.
        const winningPayout = raceAccount.payouts[raceAccount.winnerId];
        console.log(`Winner (IDs 0-3): ${raceAccount.winnerId}. Payout: ${winningPayout}`);
        // With 4 different bets, someone must win.
        expect(winningPayout.gt(new anchor.BN(0))).to.be.true;
    });
});

// Helpers
async function initSimulateRaceCompDef(
    program: Program<Horsebetting>,
    owner: anchor.web3.Keypair,
): Promise<string> {
    const baseSeedCompDefAcc = getArciumAccountBaseSeed(
        "ComputationDefinitionAccount",
    );
    const offset = getCompDefAccOffset("simulate_race");

    const compDefPDA = PublicKey.findProgramAddressSync(
        [baseSeedCompDefAcc, program.programId.toBuffer(), offset],
        getArciumProgramId(),
    )[0];

    const mxeAccount = getMXEAccAddress(program.programId);
    // Might fail if MXE not initialized, but test setup implies environment exists
    const mxeAcc = await getArciumProgram(anchor.getProvider() as anchor.AnchorProvider).account.mxeAccount.fetch(mxeAccount);
    const lutAddress = getLookupTableAddress(program.programId, mxeAcc.lutOffsetSlot);

    const sig = await program.methods
        .initSimulateRaceCompDef()
        .accounts({
            compDefAccount: compDefPDA,
            payer: owner.publicKey,
            mxeAccount,
            addressLookupTable: lutAddress,
        })
        .signers([owner])
        .rpc({
            commitment: "confirmed",
        });

    const rawCircuit = fs.readFileSync("build/simulate_race.arcis");
    await uploadCircuit(
        anchor.getProvider() as anchor.AnchorProvider,
        "simulate_race",
        program.programId,
        rawCircuit,
        true,
    );

    return sig;
}

async function getMXEPublicKeyWithRetry(
    provider: anchor.AnchorProvider,
    programId: PublicKey,
    maxRetries: number = 20,
    retryDelayMs: number = 500,
): Promise<Uint8Array> {
    for (let attempt = 1; attempt <= maxRetries; attempt++) {
        try {
            const mxePublicKey = await getMXEPublicKey(provider, programId);
            if (mxePublicKey) {
                return mxePublicKey;
            }
        } catch (error) {
            // ignore
        }

        if (attempt < maxRetries) {
            await new Promise((resolve) => setTimeout(resolve, retryDelayMs));
        }
    }
    throw new Error("Failed to fetch MXE public key");
}

function readKpJson(path: string): anchor.web3.Keypair {
    const file = fs.readFileSync(path);
    return anchor.web3.Keypair.fromSecretKey(
        new Uint8Array(JSON.parse(file.toString())),
    );
}
