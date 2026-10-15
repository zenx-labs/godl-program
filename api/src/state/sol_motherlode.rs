use serde::{Deserialize, Serialize};
use steel::*;

use crate::consts::{DENOMINATOR_BPS, RUSH_SOL_MOTHERLODE_BPS};

use super::GodlAccount;

/// SolMotherlode is a singleton account which contains the amount of SOL in the motherlode rewards pool.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct SolMotherlode {
    /// The amount of SOL in the motherlode rewards pool.
    pub amount: u64,
}

impl SolMotherlode {
    /// Splits an amount leaving the motherlode in `PreBuyGold` into `(rush, gold)`:
    /// `RUSH_SOL_MOTHERLODE_BPS` goes to the rush SOL vault, the remainder is swapped into XAUt0
    /// gold rewards. `None` on overflow.
    pub fn split_payout(amount: u64) -> Option<(u64, u64)> {
        let rush = amount.checked_mul(RUSH_SOL_MOTHERLODE_BPS)? / DENOMINATOR_BPS;
        Some((rush, amount - rush))
    }
}

account!(GodlAccount, SolMotherlode);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_payout_is_ten_percent_rush_and_conserves() {
        assert_eq!(SolMotherlode::split_payout(0), Some((0, 0)));
        assert_eq!(SolMotherlode::split_payout(9), Some((0, 9)));
        assert_eq!(SolMotherlode::split_payout(10), Some((1, 9)));
        assert_eq!(
            SolMotherlode::split_payout(10_000_000),
            Some((1_000_000, 9_000_000))
        );
        for amount in [1u64, 7, 12_345, 1_000_000_007] {
            let (rush, gold) = SolMotherlode::split_payout(amount).unwrap();
            assert_eq!(rush + gold, amount);
            assert_eq!(rush, amount / 10);
        }
        assert_eq!(SolMotherlode::split_payout(u64::MAX), None);
    }
}
