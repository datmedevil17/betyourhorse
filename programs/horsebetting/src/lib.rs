use anchor_lang::prelude::*;
use arcium_anchor::prelude::*;

const COMP_DEF_OFFSET_SIMULATE_RACE: u32 = comp_def_offset("simulate_race");

declare_id!("38oYmq8DqpyGV3L6Gp3qdiYcmCt6GuL9jCF3WD2fgNhm");

#[arcium_program]
pub mod horsebetting {
    use super::*;

    pub fn init_simulate_race_comp_def(ctx: Context<InitSimulateRaceCompDef>) -> Result<()> {
        init_comp_def(ctx.accounts, None, None)?;
        Ok(())
    }

    pub fn create_horse(ctx: Context<CreateHorse>, id: u8, speed: u8, stamina: u8, name: String) -> Result<()> {
        let horse = &mut ctx.accounts.horse;
        horse.owner = ctx.accounts.payer.key();
        horse.id = id;
        horse.speed = speed;
        horse.stamina = stamina;
        horse.name = name;
        Ok(())
    }

    pub fn create_race(ctx: Context<CreateRace>, race_id: u64, horses: [Pubkey; 4]) -> Result<()> {
        let race = &mut ctx.accounts.race;
        race.race_id = race_id;
        race.status = RaceStatus::Open;
        race.horses = horses;
        race.bet_count = 0;
        race.winner_id = 0;
        Ok(())
    }

    pub fn place_bet(ctx: Context<PlaceBet>, amount: u64, horse_selection_ciphertext: [u8; 32]) -> Result<()> {
        let race = &mut ctx.accounts.race;
        require!(race.status == RaceStatus::Open, ErrorCode::RaceNotOpen);
        require!(race.bet_count < 4, ErrorCode::RaceFull);

        let bet_index = race.bet_count as usize;
        race.bets[bet_index] = BetInfo {
            bettor: ctx.accounts.bettor.key(),
            amount,
            selection_ciphertext: horse_selection_ciphertext,
        };
        race.bet_count += 1;
        Ok(())
    }

    pub fn run_race(
        ctx: Context<SimulateRace>,
        computation_offset: u64,
        random_seed: u64,
    ) -> Result<()> {
        let race = &mut ctx.accounts.race;
        require!(race.status == RaceStatus::Open, ErrorCode::RaceNotOpen);
        // require!(race.bet_count > 0, ErrorCode::NoBets); // Optional
        race.status = RaceStatus::Running;
        race.random_seed = random_seed;

        ctx.accounts.sign_pda_account.bump = ctx.bumps.sign_pda_account;
        
        // Build Arguments: Horses (4) + Bets (4) + Random Seed
        let mut builder = ArgBuilder::new();

        // 1. Add Horses (Plaintext attributes)
        // Accessing horse accounts from remaining accounts or a direct passed array would be cleaner, 
        // but for Anchor we typically pass specific accounts.
        // For MVP, we pass the 4 horse accounts in the Context.
        
        let horses = [&ctx.accounts.horse1, &ctx.accounts.horse2, &ctx.accounts.horse3, &ctx.accounts.horse4];
        
        for horse in horses.iter() {
           builder = builder
               .plaintext_u8(horse.id)
               .plaintext_u8(horse.speed)
               .plaintext_u8(horse.stamina);
        }

        // 2. Add Bets (4)
        // We iterate through fixed size 4. If fewer bets, we pad with empty/zeros.
        for i in 0..4 {
             if i < race.bet_count as usize {
                 let bet = &race.bets[i];
                 builder = builder
                    .plaintext_u64(bet.amount)
                    .encrypted_u8(bet.selection_ciphertext);
             } else {
                 // Padding
                 builder = builder
                    .plaintext_u64(0)
                    .encrypted_u8([0u8; 32]); // Encrypted zero/null
             }
        }

        // 3. Random Seed
        builder = builder.plaintext_u64(random_seed);

        let args = builder.build();

        queue_computation(
            ctx.accounts,
            computation_offset,
            args,
            vec![SimulateRaceCallback::callback_ix(
                computation_offset,
                &ctx.accounts.mxe_account,
                &[] 
            )?],
            1, // Priority
            0, // Time to live
        )?;
        Ok(())
    }

    #[arcium_callback(encrypted_ix = "simulate_race")]
    pub fn simulate_race_callback(
        ctx: Context<SimulateRaceCallback>,
        output: SignedComputationOutputs<SimulateRaceOutput>,
    ) -> Result<()> {
        let o = match output.verify_output(&ctx.accounts.cluster_account, &ctx.accounts.computation_account) {
            Ok(data) => data.field_0,
            Err(_) => return Err(ErrorCode::AbortedComputation.into()),
        };

        // winner_id is the first field (u8)
        let winner_id = o.ciphertexts[0][0];
        
        // payouts are the next 4 fields (u64)
        let mut payouts = [0u64; 4];
        for i in 0..4 {
            let bytes: [u8; 8] = o.ciphertexts[i + 1][0..8].try_into().unwrap();
            payouts[i] = u64::from_le_bytes(bytes);
        }

        let race = &mut ctx.accounts.race;
        race.status = RaceStatus::Completed;
        race.winner_id = winner_id;
        race.payouts = payouts;

        emit!(RaceResolvedEvent {
            race_id: race.race_id,
            winner_id,
            payouts,
        });

        Ok(())
    }


}

// ---------------- Accounts & Structs ----------------

#[account]
pub struct Horse {
    pub owner: Pubkey,
    pub id: u8,
    pub speed: u8,
    pub stamina: u8,
    pub name: String, 
}

#[account]
pub struct Race {
    pub race_id: u64,
    pub status: RaceStatus,
    pub horses: [Pubkey; 4],
    pub bets: [BetInfo; 4],
    pub bet_count: u8,
    pub winner_id: u8,
    pub payouts: [u64; 4],
    pub random_seed: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq)]
pub enum RaceStatus {
    Open,
    Running,
    Completed
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct BetInfo {
    pub bettor: Pubkey,
    pub amount: u64,
    pub selection_ciphertext: [u8; 32],
}

// ---------------- Contexts ----------------

#[derive(Accounts)]
#[instruction(id: u8)]
pub struct CreateHorse<'info> {
    #[account(
        init, 
        payer = payer, 
        space = 8 + 32 + 1 + 1 + 1 + 32, // Adjust space as needed
        seeds = [b"horse", payer.key().as_ref(), &[id]],
        bump
    )]
    pub horse: Account<'info, Horse>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(race_id: u64)]
pub struct CreateRace<'info> {
    #[account(
        init, 
        payer = payer, 
        space = 8 + 8 + 1 + (32 * 4) + (80 * 4) + 1 + 1 + (8 * 4) + 8, // Approx space
        seeds = [b"race".as_ref(), race_id.to_le_bytes().as_ref()],
        bump
    )]
    pub race: Account<'info, Race>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct PlaceBet<'info> {
    #[account(mut)]
    pub race: Account<'info, Race>,
    #[account(mut)]
    pub bettor: Signer<'info>,
}

#[queue_computation_accounts("simulate_race", payer)]
#[derive(Accounts)]
#[instruction(computation_offset: u64)]
pub struct SimulateRace<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(
        init_if_needed,
        space = 9,
        payer = payer,
        seeds = [&SIGN_PDA_SEED],
        bump,
        address = derive_sign_pda!(),
    )]
    pub sign_pda_account: Account<'info, ArciumSignerAccount>,
    #[account(
        address = derive_mxe_pda!()
    )]
    pub mxe_account: Account<'info, MXEAccount>,
    #[account(
        mut,
        address = derive_mempool_pda!(mxe_account, ErrorCode::ClusterNotSet)
    )]
    /// CHECK: mempool_account, checked by the arcium program.
    pub mempool_account: UncheckedAccount<'info>,
    #[account(
        mut,
        address = derive_execpool_pda!(mxe_account, ErrorCode::ClusterNotSet)
    )]
    /// CHECK: executing_pool, checked by the arcium program.
    pub executing_pool: UncheckedAccount<'info>,
    #[account(
        mut,
        address = derive_comp_pda!(computation_offset, mxe_account, ErrorCode::ClusterNotSet)
    )]
    /// CHECK: computation_account, checked by the arcium program.
    pub computation_account: UncheckedAccount<'info>,
    #[account(
        address = derive_comp_def_pda!(COMP_DEF_OFFSET_SIMULATE_RACE)
    )]
    pub comp_def_account: Account<'info, ComputationDefinitionAccount>,
    #[account(
        mut,
        address = derive_cluster_pda!(mxe_account, ErrorCode::ClusterNotSet)
    )]
    pub cluster_account: Account<'info, Cluster>,
    #[account(
        mut,
        address = ARCIUM_FEE_POOL_ACCOUNT_ADDRESS,
    )]
    pub pool_account: Account<'info, FeePool>,
    #[account(
        mut,
        address = ARCIUM_CLOCK_ACCOUNT_ADDRESS
    )]
    pub clock_account: Account<'info, ClockAccount>,
    
    // Application Accounts
    #[account(mut)]
    pub race: Account<'info, Race>,
    // Horses
    pub horse1: Account<'info, Horse>,
    pub horse2: Account<'info, Horse>,
    pub horse3: Account<'info, Horse>,
    pub horse4: Account<'info, Horse>,

    pub system_program: Program<'info, System>,
    pub arcium_program: Program<'info, Arcium>,
}

#[callback_accounts("simulate_race")]
#[derive(Accounts)]
pub struct SimulateRaceCallback<'info> {
    pub arcium_program: Program<'info, Arcium>,
    #[account(
        address = derive_comp_def_pda!(COMP_DEF_OFFSET_SIMULATE_RACE)
    )]
    pub comp_def_account: Account<'info, ComputationDefinitionAccount>,
    #[account(
        address = derive_mxe_pda!()
    )]
    pub mxe_account: Account<'info, MXEAccount>,
    /// CHECK: computation_account, checked by arcium program via constraints in the callback context.
    pub computation_account: UncheckedAccount<'info>,
    #[account(
        address = derive_cluster_pda!(mxe_account, ErrorCode::ClusterNotSet)
    )]
    pub cluster_account: Account<'info, Cluster>,
    
    // Application Accounts
    #[account(mut)]
    pub race: Account<'info, Race>,

    #[account(address = ::anchor_lang::solana_program::sysvar::instructions::ID)]
    /// CHECK: instructions_sysvar, checked by the account constraint
    pub instructions_sysvar: AccountInfo<'info>,
}

#[init_computation_definition_accounts("simulate_race", payer)]
#[derive(Accounts)]
pub struct InitSimulateRaceCompDef<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(
        mut,
        address = derive_mxe_pda!()
    )]
    pub mxe_account: Box<Account<'info, MXEAccount>>,
    #[account(mut)]
    /// CHECK: comp_def_account, checked by arcium program.
    pub comp_def_account: UncheckedAccount<'info>,
    #[account(
        mut,
        address = derive_mxe_lut_pda!(mxe_account.lut_offset_slot)
    )]
    /// CHECK: address_lookup_table, checked by arcium program.
    pub address_lookup_table: UncheckedAccount<'info>,
    #[account(address = LUT_PROGRAM_ID)]
    /// CHECK: lut_program is the Address Lookup Table program.
    pub lut_program: UncheckedAccount<'info>,
    pub arcium_program: Program<'info, Arcium>,
    pub system_program: Program<'info, System>,
}





#[event]
pub struct RaceResolvedEvent {
    pub race_id: u64,
    pub winner_id: u8,
    pub payouts: [u64; 4],
}

#[event]
pub struct SumEvent {
    pub sum: [u8; 32],
    pub nonce: [u8; 16],
}





#[error_code]
pub enum ErrorCode {
    #[msg("The computation was aborted")]
    AbortedComputation,
    #[msg("Cluster not set")]
    ClusterNotSet,
    #[msg("Race is not open")]
    RaceNotOpen,
    #[msg("Race is full")]
    RaceFull,
    #[msg("No bets placed")]
    NoBets,
}