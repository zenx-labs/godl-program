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

    /// Reward-weight curve version: 0 = linear multiplier (legacy),
    /// 1 = sqrt(multiplier). Existing accounts are zero-initialized (linear)
    /// and flipped by `MigrateStakeWeight`; new accounts are created at 1.
    pub weight_version: u8,

    /// Buffer for future use
    pub buffer: [u8; 30],
}

/// Layout guard: `weight_version` was carved out of the reserved buffer, so the
/// struct (and the 200-byte on-chain account) must not change size.
const _: () = assert!(core::mem::size_of::<StakeV2>() == 192);

/// Integer square root (floor) via Newton's method. The initial guess
/// `1 << ceil(bit_length / 2)` always upper-bounds the root, so the iteration
/// descends monotonically and terminates at floor(sqrt(n)).
pub fn isqrt_u128(n: u128) -> u128 {
    if n == 0 {
        return 0;
    }
    let mut x = 1u128 << ((128 - n.leading_zeros()).div_ceil(2));
    loop {
        let y = (x + n / x) / 2;
        if y >= x {
            return x;
        }
        x = y;
    }
}

/// `multiplier` is scaled by STAKE_MULTIPLIER_SCALE (1e9). Returns
/// floor(sqrt(multiplier)) in the same fixed-point scale:
/// isqrt(m × 1e9) == 1e9 × sqrt(m / 1e9). Fixpoint at 1x (1e9 -> 1e9);
/// 20x (20e9) -> 4_472_135_954 (~4.47x).
pub fn effective_multiplier(multiplier: u64) -> u64 {
    isqrt_u128(multiplier as u128 * STAKE_MULTIPLIER_SCALE as u128) as u64
}

impl StakeV2 {
    /// The multiplier actually applied to reward weight, per this account's
    /// weight version.
    pub fn effective_multiplier(&self) -> u64 {
        match self.weight_version {
            0 => self.multiplier,
            _ => effective_multiplier(self.multiplier),
        }
    }

    fn apply_multiplier(&self, amount: u64) -> Result<u64, ProgramError> {
        let multiplier = self.effective_multiplier();
        if amount == 0 || multiplier == 0 {
            return Ok(0);
        }
        let weighted = (amount as u128 * multiplier as u128) / STAKE_MULTIPLIER_SCALE as u128;
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

    /// Flips this account from the linear weight curve (version 0) to the sqrt
    /// curve (version 1). Idempotent. Order is load-bearing: pending rewards
    /// are settled at the OLD weight before the flip, then `total_staked` is
    /// adjusted by exactly the weighted-units delta.
    pub fn migrate_weight(&mut self, treasury: &mut Treasury) -> ProgramResult {
        if self.weight_version != 0 {
            return Ok(());
        }
        self.update_rewards(treasury)?;
        let prev_units = self.weighted_units()?;
        self.weight_version = 1;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consts::{MAX_SUPPLY, STAKE_MULTIPLIER_SCALE};

    const SCALE: u64 = STAKE_MULTIPLIER_SCALE;

    fn xorshift64(state: &mut u64) -> u64 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *state = x;
        x
    }

    fn stake(balance: u64, multiplier: u64, nft: u8, version: u8) -> StakeV2 {
        let mut s = StakeV2::zeroed();
        s.balance = balance;
        s.multiplier = multiplier;
        s.is_nft_staked = nft;
        s.weight_version = version;
        s
    }

    fn treasury(total_staked: u64, stake_rewards_factor: Numeric) -> Treasury {
        let mut t = Treasury::zeroed();
        t.total_staked = total_staked;
        t.stake_rewards_factor = stake_rewards_factor;
        t
    }

    #[test]
    fn layout_offsets_are_frozen() {
        // Raw account offsets = struct offsets + 8 (steel discriminant).
        // These are consumed by the indexer and Appendix A of the plan.
        assert_eq!(core::mem::size_of::<StakeV2>(), 192);
        assert_eq!(core::mem::offset_of!(StakeV2, balance), 40);
        assert_eq!(core::mem::offset_of!(StakeV2, multiplier), 104);
        assert_eq!(core::mem::offset_of!(StakeV2, is_nft_staked), 160);
        assert_eq!(core::mem::offset_of!(StakeV2, weight_version), 161);
        assert_eq!(core::mem::offset_of!(StakeV2, buffer), 162);
    }

    #[test]
    fn effective_multiplier_anchor_points() {
        assert_eq!(effective_multiplier(0), 0);
        assert_eq!(effective_multiplier(SCALE), SCALE); // 1x is the fixpoint
        assert_eq!(effective_multiplier(20 * SCALE), 4_472_135_954); // 20x -> ~4.47x
        assert_eq!(effective_multiplier(4 * SCALE), 2 * SCALE); // perfect square
    }

    #[test]
    fn isqrt_edge_cases() {
        assert_eq!(isqrt_u128(0), 0);
        assert_eq!(isqrt_u128(1), 1);
        assert_eq!(isqrt_u128(2), 1);
        assert_eq!(isqrt_u128(3), 1);
        assert_eq!(isqrt_u128(4), 2);
        assert_eq!(isqrt_u128(8), 2);
        assert_eq!(isqrt_u128(9), 3);
        assert_eq!(isqrt_u128(u128::MAX), (1u128 << 64) - 1);
        // Largest input effective_multiplier can produce: u64::MAX * 1e9 < 2^94.
        let max_input = u64::MAX as u128 * SCALE as u128;
        let r = isqrt_u128(max_input);
        assert!(r * r <= max_input && (r + 1) * (r + 1) > max_input);
        assert!(r <= u64::MAX as u128); // the u64 cast in effective_multiplier is safe
        // Powers of two and neighbors across the whole domain.
        for shift in 0..128 {
            let n = 1u128 << shift;
            for n in [n.saturating_sub(1), n, n.saturating_add(1)] {
                let r = isqrt_u128(n);
                assert!(r * r <= n, "isqrt({n}) = {r} not a lower bound");
                assert!(
                    (r + 1).checked_mul(r + 1).map_or(true, |sq| sq > n),
                    "isqrt({n}) = {r} not tight"
                );
            }
        }
    }

    #[test]
    fn isqrt_floor_correctness_random() {
        let mut rng = 0x9E3779B97F4A7C15u64;
        for _ in 0..100_000 {
            // Vary magnitude: shift a full-width random product down 0..=64 bits.
            let hi = xorshift64(&mut rng) as u128;
            let lo = xorshift64(&mut rng) as u128;
            let n = ((hi << 64) | lo) >> (xorshift64(&mut rng) % 65);
            let r = isqrt_u128(n);
            assert!(r * r <= n, "isqrt({n}) = {r} not a lower bound");
            assert!(
                (r + 1).checked_mul(r + 1).map_or(true, |sq| sq > n),
                "isqrt({n}) = {r} not tight"
            );
        }
    }

    #[test]
    fn effective_multiplier_properties_random() {
        let mut rng = 0xD1B54A32D192ED03u64;
        let mut prev_pair: Option<(u64, u64)> = None;
        for _ in 0..100_000 {
            let m = xorshift64(&mut rng);
            let e = effective_multiplier(m);
            // Floor-correctness in the 1e9 fixed-point scale.
            let n = m as u128 * SCALE as u128;
            let e128 = e as u128;
            assert!(e128 * e128 <= n && (e128 + 1) * (e128 + 1) > n);
            // sqrt contracts toward the 1x fixpoint.
            match m.cmp(&SCALE) {
                core::cmp::Ordering::Less if m > 0 => assert!(e > m && e <= SCALE),
                core::cmp::Ordering::Greater => assert!(e < m && e > SCALE),
                _ => assert_eq!(e, m.min(SCALE)),
            }
            // Monotone non-decreasing.
            if let Some((pm, pe)) = prev_pair {
                if m >= pm {
                    assert!(e >= pe);
                } else {
                    assert!(e <= pe);
                }
            }
            prev_pair = Some((m, e));
        }
    }

    #[test]
    fn weighted_units_version_branch() {
        let bal = 1_000 * crate::consts::ONE_GODL; // 1e14 grams
        let linear = stake(bal, 20 * SCALE, 0, 0).weighted_units().unwrap();
        assert_eq!(linear, bal * 20);
        let sqrt = stake(bal, 20 * SCALE, 0, 1).weighted_units().unwrap();
        assert_eq!(
            sqrt as u128,
            bal as u128 * 4_472_135_954 / SCALE as u128
        );
        assert!(sqrt < linear);
        // 1x accounts are unchanged by the flip.
        assert_eq!(
            stake(bal, SCALE, 0, 0).weighted_units().unwrap(),
            stake(bal, SCALE, 0, 1).weighted_units().unwrap()
        );
    }

    #[test]
    fn nft_boost_applies_after_curve() {
        // bal=7, eff(20x)=4_472_135_954:
        //   after-curve  : floor(floor(7 * 4.472...) * 11/10) = floor(31 * 1.1) = 34
        //   before-curve : floor(floor(7 * 1.1) * 4.472...)   = floor(7 * 4.472...) = 31
        let s = stake(7, 20 * SCALE, 1, 1);
        assert_eq!(s.weighted_units().unwrap(), 34);
        // General exact formula on a bigger account.
        let bal = 123_456_789_012_345u64;
        let s = stake(bal, 20 * SCALE, 1, 1);
        let base = bal as u128 * 4_472_135_954 / SCALE as u128;
        assert_eq!(s.weighted_units().unwrap() as u128, base * 11 / 10);
    }

    #[test]
    fn overflow_headroom_at_max_supply() {
        // Max supply, max multiplier, NFT boost: must fit u64 in both versions.
        for version in [0, 1] {
            let w = stake(MAX_SUPPLY, 20 * SCALE, 1, version)
                .weighted_units()
                .unwrap();
            assert!(w > 0);
        }
        // Linear ceiling: 2.1e17 * 20 * 1.1 = 4.62e18 < u64::MAX (1.8e19).
        assert_eq!(
            stake(MAX_SUPPLY, 20 * SCALE, 1, 0).weighted_units().unwrap(),
            (MAX_SUPPLY as u128 * 20 * 11 / 10) as u64
        );
    }

    #[test]
    fn migrate_settles_at_old_weight_then_flips() {
        let bal = 100 * crate::consts::ONE_GODL; // 1e13 grams
        let mut s = stake(bal, 20 * SCALE, 0, 0);
        let linear = s.weighted_units().unwrap(); // 2e14
        let sqrt = {
            let mut c = s;
            c.weight_version = 1;
            c.weighted_units().unwrap()
        };
        // 0.25 GODL-per-weighted-unit is exact in I80F48.
        let mut t = treasury(linear, Numeric::from_fraction(1, 4));

        s.migrate_weight(&mut t).unwrap();

        // Rewards settled at the OLD (linear) weight, not the sqrt weight.
        assert_eq!(s.rewards, linear / 4);
        assert_ne!(linear / 4, sqrt / 4);
        assert_eq!(s.weight_version, 1);
        assert_eq!(s.rewards_factor, t.stake_rewards_factor);
        // total_staked adjusted by exactly new - prev.
        assert_eq!(t.total_staked, sqrt);
        // Stored multiplier untouched.
        assert_eq!(s.multiplier, 20 * SCALE);
    }

    #[test]
    fn migrate_is_idempotent() {
        let bal = 100 * crate::consts::ONE_GODL;
        let mut s = stake(bal, 20 * SCALE, 0, 0);
        let linear = s.weighted_units().unwrap();
        let mut t = treasury(linear, Numeric::from_fraction(1, 4));
        s.migrate_weight(&mut t).unwrap();
        let (rewards, total, version, factor) =
            (s.rewards, t.total_staked, s.weight_version, s.rewards_factor);

        // Even with more rewards accrued since, a second migrate is a pure
        // no-op (it must not settle or touch anything).
        t.stake_rewards_factor = t.stake_rewards_factor + Numeric::from_fraction(1, 4);
        s.migrate_weight(&mut t).unwrap();

        assert_eq!(s.rewards, rewards);
        assert_eq!(t.total_staked, total);
        assert_eq!(s.weight_version, version);
        assert_eq!(s.rewards_factor, factor);
    }

    #[test]
    fn weighted_units_overflow_returns_error_not_panic() {
        // Beyond-u64 weight must surface as StakeOverflow through both the
        // base guard and the NFT-boost guard, on both curve versions.
        let overflow: Result<u64, ProgramError> = Err(GodlError::StakeOverflow.into());

        // Base guard: u64::MAX balance at 20x overflows linear (×20) and
        // sqrt (×~4.47) alike.
        for version in [0, 1] {
            assert_eq!(
                stake(u64::MAX, 20 * SCALE, 0, version).weighted_units(),
                overflow
            );
        }

        // Boost guard specifically: at exactly 1x the base weight is u64::MAX
        // (passes the base guard), then ×11/10 overflows.
        for version in [0, 1] {
            assert_eq!(
                stake(u64::MAX, SCALE, 1, version).weighted_units(),
                overflow
            );
        }
    }

    #[test]
    fn migrate_sub_1x_multiplier_increases_weight() {
        // Not producible by stake_multiplier(), but exploit-era accounts have
        // taught us not to assume that: a sub-1x multiplier must migrate
        // through the checked_add branch (sqrt RAISES weight below the 1x
        // fixpoint). 0.25x -> exactly 0.5x (perfect square).
        let bal = 100 * crate::consts::ONE_GODL; // 1e13 grams
        let mut s = stake(bal, SCALE / 4, 0, 0);
        let linear = s.weighted_units().unwrap();
        assert_eq!(linear, bal / 4);
        let mut t = treasury(linear, Numeric::from_fraction(1, 4));

        s.migrate_weight(&mut t).unwrap();

        assert_eq!(s.weight_version, 1);
        assert_eq!(s.weighted_units().unwrap(), bal / 2);
        assert_eq!(t.total_staked, bal / 2, "increase path must adjust total");
        // Still settled at the OLD (0.25x) weight.
        assert_eq!(s.rewards, linear / 4);

        // Degenerate zero multiplier: weight 0 on both curves, migrate is a
        // clean no-op on the total.
        let mut z = stake(bal, 0, 0, 0);
        assert_eq!(z.weighted_units().unwrap(), 0);
        let mut t = treasury(0, Numeric::ZERO);
        z.migrate_weight(&mut t).unwrap();
        assert_eq!(z.weight_version, 1);
        assert_eq!(z.weighted_units().unwrap(), 0);
        assert_eq!(t.total_staked, 0);
    }

    #[test]
    fn degenerate_flag_bytes_behave_like_canonical_values() {
        let bal = 100 * crate::consts::ONE_GODL;

        // Any nonzero weight_version hits the sqrt arm, same as 1.
        let canonical = stake(bal, 20 * SCALE, 0, 1).weighted_units().unwrap();
        for version in [2, 7, 255] {
            assert_eq!(
                stake(bal, 20 * SCALE, 0, version).weighted_units().unwrap(),
                canonical
            );
            // ...and migrate treats it as already migrated: pure no-op.
            let mut s = stake(bal, 20 * SCALE, 0, version);
            let mut t = treasury(canonical, Numeric::from_fraction(1, 4));
            s.migrate_weight(&mut t).unwrap();
            assert_eq!(s.weight_version, version, "must not rewrite the byte");
            assert_eq!(s.rewards, 0, "must not settle");
            assert_eq!(t.total_staked, canonical);
        }

        // Any nonzero is_nft_staked applies the boost, same as 1.
        let boosted = stake(bal, 20 * SCALE, 1, 1).weighted_units().unwrap();
        for nft in [2, 255] {
            assert_eq!(
                stake(bal, 20 * SCALE, nft, 1).weighted_units().unwrap(),
                boosted
            );
        }
    }

    #[test]
    fn migrate_never_increases_weight_for_real_multipliers() {
        // Production multipliers are in [1x, 20x]; sqrt must only shrink them.
        let mut rng = 0xA0761D6478BD642Fu64;
        for _ in 0..10_000 {
            let m = SCALE + xorshift64(&mut rng) % (19 * SCALE + 1);
            let bal = xorshift64(&mut rng) % MAX_SUPPLY;
            let nft = (xorshift64(&mut rng) % 2) as u8;
            let mut s = stake(bal, m, nft, 0);
            let prev = s.weighted_units().unwrap();
            let mut t = treasury(prev, Numeric::ZERO);
            s.migrate_weight(&mut t).unwrap();
            let new = s.weighted_units().unwrap();
            assert!(new <= prev);
            assert_eq!(t.total_staked, new);
        }
    }
}
