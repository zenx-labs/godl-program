use serde::{Deserialize, Serialize};
use steel::*;

use crate::{
    consts::GOLD_FACTOR_SCALE,
    state::{miner_extended_pda, GodlAccount, GoldVault},
};

/// MinerExtended holds the per-miner gold rewards state that did not fit in `Miner`.
///
/// One exists per miner authority. It snapshots the gold vault's rewards factor and accrues
/// XAUt0 lazily, weighted by the miner's unrefined GODL (`Miner::rewards_godl`). It must be
/// synced (`update_rewards`) before that balance changes: every handler that mutates
/// `rewards_godl` calls it right after `Miner::update_rewards`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct MinerExtended {
    /// The authority of the miner this account extends.
    pub authority: Pubkey,

    /// The gold vault rewards factor last time rewards were updated on this account.
    pub xaut_rewards_factor: Numeric,

    /// The amount of XAUt0 this miner can claim.
    pub xaut_rewards: u64,

    /// The total amount of XAUt0 this miner has ever accrued.
    pub lifetime_xaut: u64,

    /// Reserved for future fields.
    pub buffer: [u8; 32],
}

impl MinerExtended {
    pub fn pda(&self) -> (Pubkey, u8) {
        miner_extended_pda(self.authority)
    }

    /// Initializes a freshly created account: nothing accrued, factor snapshotted to now.
    pub fn init(&mut self, authority: Pubkey, gold_vault: &GoldVault) {
        self.authority = authority;
        self.xaut_rewards_factor = gold_vault.xaut_rewards_factor;
        self.xaut_rewards = 0;
        self.lifetime_xaut = 0;
        self.buffer = [0; 32];
    }

    /// Accrues XAUt0 earned since the last update on `unrefined` grams of GODL, then snapshots
    /// the vault factor. Must run before `unrefined` changes.
    pub fn update_rewards(&mut self, gold_vault: &GoldVault, unrefined: u64) {
        if gold_vault.xaut_rewards_factor > self.xaut_rewards_factor {
            let accumulated = gold_vault.xaut_rewards_factor - self.xaut_rewards_factor;
            if accumulated < Numeric::ZERO {
                panic!("Accumulated gold rewards is negative");
            }
            // factor is (units * SCALE / gram); multiply by (grams / SCALE) to get units.
            let personal = accumulated * Numeric::from_fraction(unrefined, GOLD_FACTOR_SCALE);
            let personal = personal.to_u64();
            self.xaut_rewards += personal;
            self.lifetime_xaut += personal;
        }
        self.xaut_rewards_factor = gold_vault.xaut_rewards_factor;
    }

    /// Takes the claimable balance.
    pub fn claim(&mut self) -> u64 {
        let amount = self.xaut_rewards;
        self.xaut_rewards = 0;
        amount
    }
}

account!(GodlAccount, MinerExtended);
