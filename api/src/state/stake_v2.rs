use serde::{Deserialize, Serialize};
use steel::*;

use crate::{
    consts::{NFT_BOOST_DENOMINATOR, NFT_BOOST_NUMERATOR, STAKE_MULTIPLIER_SCALE},
    error::GodlError,
    state::{stake_v2_pda, Treasury},
};

use super::GodlAccount;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct StakeV2 {
    /// The id of this stake account.
    pub id: u64,

    /// The authority of this miner account.
    pub authority: Pubkey,

    /// The balance of this stake account.
    pub balance: u64,

    /// The timestamp of last claim.
    pub last_claim_at: i64,

    /// The timestamp the last time this staker deposited.
    pub last_deposit_at: i64,

    /// The timestamp the last time this staker withdrew.
    pub last_withdraw_at: i64,

    /// The rewards factor last time rewards were updated on this stake account.
    pub rewards_factor: Numeric,

    /// The amount of GODL this staker can claim.
    pub rewards: u64,

    /// The total amount of GODL this staker has earned over its lifetime.
    pub lifetime_rewards: u64,

    /// The multiplier for this stake account, scaled by STAKE_MULTIPLIER_SCALE.
    pub multiplier: u64,

    // The amount of seconds this stake is locked for
    pub lock_duration: i64,

    /// The executor of this stake account.
    pub executor: Pubkey,

    /// The timestamp this stake was created at
    pub created_at: i64,

    /// Whether an NFT from the boost collection is staked (0 = no, 1 = yes).
    pub is_nft_staked: u8,

    /// Buffer for future use
    pub buffer: [u8; 31],
}

impl StakeV2 {
    fn apply_multiplier(&self, amount: u64) -> Result<u64, ProgramError> {
        if amount == 0 || self.multiplier == 0 {
            return Ok(0);
        }
        let weighted = (amount as u128 * self.multiplier as u128) / STAKE_MULTIPLIER_SCALE as u128;
        if weighted > u64::MAX as u128 {
            return Err(GodlError::StakeOverflow.into());
        }
        Ok(weighted as u64)
    }

    pub fn weighted_units(&self) -> Result<u64, ProgramError> {
        let base = self.apply_multiplier(self.balance)?;
        if self.is_nft_staked != 0 {
            let boosted = (base as u128)
                .checked_mul(NFT_BOOST_NUMERATOR)
                .and_then(|v| v.checked_div(NFT_BOOST_DENOMINATOR))
                .ok_or(GodlError::StakeOverflow)?;
            if boosted > u64::MAX as u128 {
                return Err(GodlError::StakeOverflow.into());
            }
            return Ok(boosted as u64);
        }
        Ok(base)
    }

    pub fn pda(&self) -> (Pubkey, u8) {
        stake_v2_pda(self.authority, self.id)
    }

    pub fn claim(
        &mut self,
        amount: u64,
        clock: &Clock,
        treasury: &Treasury,
    ) -> Result<u64, ProgramError> {
        self.update_rewards(treasury)?;
        let amount = self.rewards.min(amount);
        self.rewards -= amount;
        self.last_claim_at = clock.unix_timestamp;
        Ok(amount)
    }

    pub fn deposit(
        &mut self,
        amount: u64,
        clock: &Clock,
        treasury: &mut Treasury,
        sender: &TokenAccount,
    ) -> Result<u64, ProgramError> {
        self.update_rewards(treasury)?;
        let amount = sender.amount().min(amount);
        let prev_units = self.weighted_units()?;
        self.balance = self
            .balance
            .checked_add(amount)
            .ok_or(GodlError::StakeOverflow)?;
        self.last_deposit_at = clock.unix_timestamp;
        let new_units = self.weighted_units()?;
        if new_units >= prev_units {
            treasury.total_staked = treasury
                .total_staked
                .checked_add(new_units - prev_units)
                .ok_or(GodlError::StakeOverflow)?;
        } else {
            treasury.total_staked = treasury
                .total_staked
                .checked_sub(prev_units - new_units)
                .ok_or(GodlError::StakeOverflow)?;
        }
        Ok(amount)
    }

    pub fn withdraw(
        &mut self,
        amount: u64,
        clock: &Clock,
        treasury: &mut Treasury,
    ) -> Result<u64, ProgramError> {
        self.update_rewards(treasury)?;
        let amount = self.balance.min(amount);
        let prev_units = self.weighted_units()?;
        self.balance -= amount;
        self.last_withdraw_at = clock.unix_timestamp;
        let new_units = self.weighted_units()?;
        if prev_units >= new_units {
            treasury.total_staked = treasury
                .total_staked
                .checked_sub(prev_units - new_units)
                .ok_or(GodlError::StakeOverflow)?;
        } else {
            treasury.total_staked = treasury
                .total_staked
                .checked_add(new_units - prev_units)
                .ok_or(GodlError::StakeOverflow)?;
        }
        Ok(amount)
    }

    pub fn stake_nft(&mut self, treasury: &mut Treasury) -> ProgramResult {
        if self.is_nft_staked != 0 {
            return Err(GodlError::NftAlreadyStaked.into());
        }
        self.update_rewards(treasury)?;
        let prev_units = self.weighted_units()?;
        self.is_nft_staked = 1;
        let new_units = self.weighted_units()?;
        if new_units >= prev_units {
            treasury.total_staked = treasury
                .total_staked
                .checked_add(new_units - prev_units)
                .ok_or(GodlError::StakeOverflow)?;
        } else {
            treasury.total_staked = treasury
                .total_staked
                .checked_sub(prev_units - new_units)
                .ok_or(GodlError::StakeOverflow)?;
        }
        Ok(())
    }

    pub fn unstake_nft(&mut self, treasury: &mut Treasury) -> ProgramResult {
        if self.is_nft_staked == 0 {
            return Err(GodlError::NftNotStaked.into());
        }
        self.update_rewards(treasury)?;
        let prev_units = self.weighted_units()?;
        self.is_nft_staked = 0;
        let new_units = self.weighted_units()?;
        if prev_units >= new_units {
            treasury.total_staked = treasury
                .total_staked
                .checked_sub(prev_units - new_units)
                .ok_or(GodlError::StakeOverflow)?;
        } else {
            treasury.total_staked = treasury
                .total_staked
                .checked_add(new_units - prev_units)
                .ok_or(GodlError::StakeOverflow)?;
        }
        Ok(())
    }

    pub fn update_rewards(&mut self, treasury: &Treasury) -> ProgramResult {
        // Accumulate rewards, weighted by stake balance.
        if treasury.stake_rewards_factor > self.rewards_factor {
            let accumulated_rewards = treasury.stake_rewards_factor - self.rewards_factor;
            if accumulated_rewards < Numeric::ZERO {
                panic!("Accumulated rewards is negative");
            }
            let balance_with_multiplier = self.weighted_units()?;
            let personal_rewards = accumulated_rewards * Numeric::from_u64(balance_with_multiplier);
            self.rewards = self
                .rewards
                .checked_add(personal_rewards.to_u64())
                .ok_or(GodlError::StakeOverflow)?;
            self.lifetime_rewards = self
                .lifetime_rewards
                .checked_add(personal_rewards.to_u64())
                .ok_or(GodlError::StakeOverflow)?;
        }

        // Update this stake account's last seen rewards factor.
        self.rewards_factor = treasury.stake_rewards_factor;
        Ok(())
    }
}

account!(GodlAccount, StakeV2);
