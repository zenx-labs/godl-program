use serde::{Deserialize, Serialize};
use steel::*;

use crate::{
    consts::GOLD_FACTOR_SCALE,
    state::{gold_vault_pda, GodlAccount},
};

/// GoldVault is a singleton account which tracks XAUt0 rewards owed to unrefined GODL holders.
///
/// It owns the WSOL and XAUt0 token accounts used by `BuyGold`: SOL accumulated in the sol
/// motherlode is swapped into XAUt0 and the proceeds are distributed pro rata to every miner's
/// unrefined GODL balance (`Miner::rewards_godl`) through a cumulative rewards factor.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct GoldVault {
    /// Cumulative XAUt0 distributed per gram of unrefined GODL, scaled by `GOLD_FACTOR_SCALE`.
    pub xaut_rewards_factor: Numeric,

    /// XAUt0 received while there was no unrefined GODL to distribute to. Rolled into the next
    /// distribution.
    pub pending_xaut: u64,

    /// Lifetime XAUt0 credited to the rewards factor.
    pub total_xaut_distributed: u64,

    /// Lifetime XAUt0 paid out to miners.
    pub total_xaut_claimed: u64,

    /// Lifetime SOL swapped into XAUt0.
    pub total_sol_swapped: u64,

    /// Reserved for future fields.
    pub buffer: [u8; 32],
}

impl GoldVault {
    pub fn pda() -> (Pubkey, u8) {
        gold_vault_pda()
    }

    /// Credits `xaut` (plus anything pending) to the rewards factor, split across
    /// `total_unclaimed` grams of unrefined GODL. If nothing is unrefined the amount is parked in
    /// `pending_xaut` instead.
    pub fn distribute(&mut self, xaut: u64, total_unclaimed: u64) -> Result<u64, ProgramError> {
        let amount = self
            .pending_xaut
            .checked_add(xaut)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        if amount == 0 {
            return Ok(0);
        }
        if total_unclaimed == 0 {
            self.pending_xaut = amount;
            return Ok(0);
        }
        let scaled = amount
            .checked_mul(GOLD_FACTOR_SCALE)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        self.xaut_rewards_factor += Numeric::from_fraction(scaled, total_unclaimed);
        self.pending_xaut = 0;
        self.total_xaut_distributed = self
            .total_xaut_distributed
            .checked_add(amount)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        Ok(amount)
    }
}

account!(GodlAccount, GoldVault);
