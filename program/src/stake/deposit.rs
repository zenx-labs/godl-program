use godl_api::prelude::*;
use steel::*;

/// V1 staking deposits are permanently disabled — stake via `DepositV2`.
///
/// `Withdraw` and `ClaimYield` remain live so existing v1 holders can always
/// exit and claim. The remaining v1 accounts stay accounting-consistent with
/// the sqrt reward curve: v1 weight == balance == 1x, which is the curve's
/// fixpoint (`effective_multiplier(1x) == 1x`).
pub fn process_deposit(_accounts: &[AccountInfo<'_>], _data: &[u8]) -> ProgramResult {
    Err(GodlError::StakeV1Deprecated.into())
}
