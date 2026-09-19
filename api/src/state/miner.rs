use serde::{Deserialize, Serialize};
use steel::*;

use crate::{
    consts::REFERRER_LOCKED_SENTINEL,
    state::{miner_pda, Treasury},
};

use super::GodlAccount;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct Miner {
    /// The authority of this miner account.
    pub authority: Pubkey,

    /// The referrer state associated with this miner.
    pub referrer: Pubkey,

    /// The miner's prospects in the current round.
    pub deployed: [u64; 25],

    /// The cumulative amount of SOL deployed on each square prior to this miner's move.
    pub cumulative: [u64; 25],

    /// SOL witheld in reserve to pay for checkpointing.
    pub checkpoint_fee: u64,

    /// The last round that this miner checkpointed.
    pub checkpoint_id: u64,

    /// The last time this miner claimed GODL rewards.
    pub last_claim_godl_at: i64,

    /// The last time this miner claimed SOL rewards.
    pub last_claim_sol_at: i64,

    /// The rewards factor last time rewards were updated on this miner account.
    pub rewards_factor: Numeric,

    /// The amount of SOL this miner can claim.
    pub rewards_sol: u64,

    /// The amount of GODL this miner can claim.
    pub rewards_godl: u64,

    /// The amount of GODL this miner has earned from claim fees.
    pub refined_godl: u64,

    /// The ID of the round this miner last played in.
    pub round_id: u64,

    /// The total amount of SOL this miner has deployed across all rounds.
    pub lifetime_deployed: u64,

    /// The total amount of SOL this miner has mined across all blocks.
    pub lifetime_rewards_sol: u64,

    /// The total amount of GODL this miner has mined across all blocks.
    pub lifetime_rewards_godl: u64,
}

impl Miner {
    pub fn pda(&self) -> (Pubkey, u8) {
        miner_pda(self.authority)
    }

    pub fn referrer_account(&self) -> Option<Pubkey> {
        if self.referrer == Pubkey::default() || self.referrer == REFERRER_LOCKED_SENTINEL {
            None
        } else {
            Some(self.referrer)
        }
    }

    pub fn is_locked_without_referrer(&self) -> bool {
        self.referrer == REFERRER_LOCKED_SENTINEL
    }

    pub fn claim_godl(&mut self, clock: &Clock, treasury: &mut Treasury) -> u64 {
        self.update_rewards(treasury);
        let refined_godl = self.refined_godl;
        let rewards_godl = self.rewards_godl;
        let mut amount = refined_godl + rewards_godl;
        self.refined_godl = 0;
        self.rewards_godl = 0;
        treasury.total_unclaimed -= rewards_godl;
        treasury.total_refined -= refined_godl;
        self.last_claim_godl_at = clock.unix_timestamp;

        // Charge a 10% fee and share with miners who haven't claimed yet.
        if treasury.total_unclaimed > 0 {
            let fee = rewards_godl / 10;
            amount -= fee;
            treasury.miner_rewards_factor += Numeric::from_fraction(fee, treasury.total_unclaimed);
            treasury.total_refined += fee;
            self.lifetime_rewards_godl -= fee;
        }

        amount
    }

    pub fn claim_sol(&mut self, clock: &Clock) -> u64 {
        let amount = self.rewards_sol;
        self.rewards_sol = 0;
        self.last_claim_sol_at = clock.unix_timestamp;
        amount
    }

    /// Settles the refined-GODL share this miner is owed by the claim-fee factor, weighted by
    /// its unrefined GODL (`rewards_godl`). Must be called before `rewards_godl` changes, and
    /// gold-aware call sites must follow it with `MinerExtended::update_rewards` for the same
    /// reason.
    pub fn update_rewards(&mut self, treasury: &Treasury) {
        // Accumulate rewards, weighted by stake balance.
        if treasury.miner_rewards_factor > self.rewards_factor {
            let accumulated_rewards = treasury.miner_rewards_factor - self.rewards_factor;
            if accumulated_rewards < Numeric::ZERO {
                panic!("Accumulated rewards is negative");
            }
            let personal_rewards = accumulated_rewards * Numeric::from_u64(self.rewards_godl);
            self.refined_godl += personal_rewards.to_u64();
            self.lifetime_rewards_godl += personal_rewards.to_u64();
        }

        // Update this miner account's last seen rewards factor.
        self.rewards_factor = treasury.miner_rewards_factor;
    }
}

account!(GodlAccount, Miner);
