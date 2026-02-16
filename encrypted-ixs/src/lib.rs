use arcis::*;

#[encrypted]
mod circuits {
    use arcis::*;

    #[derive(Copy, Clone)]
    pub struct Horse {
        id: u8,
        speed: u8,
        stamina: u8,
    }

    #[derive(Copy, Clone)]
    pub struct Bet {
        amount: u64,
        horse_id: u8,
    }

    #[derive(Copy, Clone)]
    pub struct RaceInputs {
        pub horses: [Horse; 4], // Support 4 horses per race for MVP
        pub bets: [Bet; 4],     // Support 4 bets per race for MVP
        pub random_seed: u64,
    }

    pub struct RaceOutputs {
        pub winner_id: u8,
        pub payouts: [u64; 4], // Payouts corresponding to the input bets
    }

    // Simple pseudo-random number generator
    fn next_rng(state: u64) -> u64 {
        state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)
    }

    #[instruction]
    pub fn simulate_race(inputs: Enc<Shared, RaceInputs>) -> Enc<Shared, RaceOutputs> {
        let input = inputs.to_arcis();
        
        // ... (logic is same) ...
        let mut best_performance: u64 = 0;
        let mut winner_id: u8 = 0;
        let mut rng = input.random_seed;

        // 1. Simulate Race
        for i in 0..4 {
            let horse = input.horses[i];
            rng = next_rng(rng);
            let random_factor = (rng % 100) as u64; // 0-99
            let performance = (horse.speed as u64) * 10 + (horse.stamina as u64) * 5 + random_factor;

            if performance > best_performance {
                best_performance = performance;
                winner_id = horse.id;
            }
        }

        // 2. Calculate Pot and Payouts
        let mut total_pool: u64 = 0;
        let mut winner_pool: u64 = 0;

        for i in 0..4 {
            let bet = input.bets[i];
            total_pool += bet.amount;
            if bet.horse_id == winner_id {
                winner_pool += bet.amount;
            }
        }

        let mut payouts = [0u64; 4];
        
        if winner_pool > 0 {
             for i in 0..4 {
                let bet = input.bets[i];
                if bet.horse_id == winner_id {
                    payouts[i] = (bet.amount * total_pool) / winner_pool;
                } else {
                    payouts[i] = 0;
                }
            }
        } else {
            for i in 0..4 {
                payouts[i] = input.bets[i].amount;
            }
        }

        inputs.owner.from_arcis(RaceOutputs {
            winner_id,
            payouts,
        }).reveal()
    }
}