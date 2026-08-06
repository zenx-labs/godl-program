//! §5.5(c): op-sequence fuzzer over the StakeV2/Treasury state machine.
//!
//! Random sequences of {deposit, withdraw, claim, migrate, nft ops,
//! distribute} run against the REAL api-crate methods (the same code the
//! program executes), with two oracles checked continuously:
//!
//! 1. Σ invariant: treasury.total_staked == Σ weighted_units at all times.
//! 2. Reward conservation: everything claimed plus everything still pending
//!    never exceeds what distributions funded.
//!
//! Failed ops are applied to a scratch copy and discarded, mirroring on-chain
//! transaction rollback.

#![no_main]

use godl_api::prelude::*;
use libfuzzer_sys::fuzz_target;
use steel::*;

const N_STAKES: usize = 4;
const MAX_OPS: usize = 512;

struct Feed<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Feed<'a> {
    fn u8(&mut self) -> u8 {
        let b = self.data.get(self.pos).copied().unwrap_or(0);
        self.pos += 1;
        b
    }

    fn u64(&mut self) -> u64 {
        let mut buf = [0u8; 8];
        for b in &mut buf {
            *b = self.u8();
        }
        u64::from_le_bytes(buf)
    }

    fn done(&self) -> bool {
        self.pos >= self.data.len()
    }
}

fn total_units(stakes: &[StakeV2]) -> u128 {
    stakes
        .iter()
        .map(|s| s.weighted_units().expect("weighted_units overflow") as u128)
        .sum()
}

fuzz_target!(|data: &[u8]| {
    let mut feed = Feed { data, pos: 0 };
    let clock = Clock::default();
    let sender = TokenAccount::V0(spl_token::state::Account {
        amount: u64::MAX,
        ..Default::default()
    });

    // Seed a small population from the fuzz input: arbitrary balances,
    // multipliers past the real 20x cap, both weight versions, NFT flags.
    let mut stakes: Vec<StakeV2> = (0..N_STAKES)
        .map(|i| {
            let mut s = StakeV2::zeroed();
            s.id = i as u64;
            s.balance = feed.u64() % (MAX_SUPPLY / N_STAKES as u64);
            s.multiplier = feed.u64() % (21 * STAKE_MULTIPLIER_SCALE);
            s.is_nft_staked = feed.u8() & 1;
            s.weight_version = feed.u8() & 1;
            s
        })
        .collect();

    let mut treasury = Treasury::zeroed();
    treasury.total_staked = u64::try_from(total_units(&stakes)).unwrap();

    let mut funded: u128 = 0; // distributed to stakers via the factor
    let mut paid: u128 = 0; // claimed out of the treasury

    let mut ops = 0;
    while !feed.done() && ops < MAX_OPS {
        ops += 1;
        let op = feed.u8() % 7;
        let i = feed.u8() as usize % N_STAKES;

        // Scratch copies: an op that errors must leave no partial mutation,
        // exactly like a failed transaction.
        let mut s = stakes[i];
        let mut t = treasury;
        let ok = match op {
            0 => s
                .deposit(feed.u64() % (1_000 * ONE_GODL), &clock, &mut t, &sender)
                .map(|_| ()),
            1 => s.withdraw(feed.u64(), &clock, &mut t).map(|_| ()),
            2 => s.claim(u64::MAX, &clock, &t).map(|claimed| {
                paid += claimed as u128;
            }),
            3 => s.migrate_weight(&mut t),
            4 => s.stake_nft(&mut t),
            5 => s.unstake_nft(&mut t),
            _ => {
                let amount = feed.u64() % (1_000 * ONE_GODL);
                if t.total_staked > 0 && amount > 0 {
                    t.stake_rewards_factor =
                        t.stake_rewards_factor + Numeric::from_fraction(amount, t.total_staked);
                    funded += amount as u128;
                }
                Ok(())
            }
        };
        if ok.is_ok() {
            stakes[i] = s;
            treasury = t;
        }

        // Oracle 1: the load-bearing invariant, after every committed op.
        assert_eq!(
            treasury.total_staked as u128,
            total_units(&stakes),
            "total_staked invariant violated after op {op} on stake {i}"
        );
    }

    // Oracle 2: settle everyone and check conservation. Floor rounding may
    // strand dust in the treasury but must never over-distribute.
    let mut pending: u128 = 0;
    for s in &mut stakes {
        s.update_rewards(&treasury).expect("settle failed");
        pending += s.rewards as u128;
    }
    assert!(
        paid + pending <= funded,
        "over-distribution: paid {paid} + pending {pending} > funded {funded}"
    );
});
