use godl_api::prelude::*;
use solana_program::log::sol_log;
use steel::*;

/// Admin-only compare-and-swap on `treasury.total_unclaimed`. `total_unclaimed` is the
/// denominator of the gold rewards factor, so it must equal the sum of `rewards_godl` across all
/// miner accounts before the first `BuyGold`. Like `RebaseTotalStaked`, the instruction fails
/// without mutating anything if the on-chain value drifted from the off-chain snapshot.
pub fn process_rebase_total_unclaimed(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    let args = RebaseTotalUnclaimed::try_from_bytes(data)?;
    let expected = u64::from_le_bytes(args.expected);
    let new_value = u64::from_le_bytes(args.new_value);

    let [signer_info, config_info, treasury_info] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;

    // Admin gate.
    let config = config_info.as_account::<Config>(&godl_api::ID)?;
    if config.admin != *signer_info.key {
        return Err(GodlError::NotAuthorized.into());
    }

    let treasury = treasury_info
        .is_writable()?
        .has_seeds(&[TREASURY], &godl_api::ID)?
        .as_account_mut::<Treasury>(&godl_api::ID)?;

    if treasury.total_unclaimed != expected {
        sol_log(
            &format!(
                "Rebase mismatch: expected {}, on-chain {}",
                expected, treasury.total_unclaimed,
            )
            .as_str(),
        );
        return Err(GodlError::RebaseMismatch.into());
    }

    treasury.total_unclaimed = new_value;
    sol_log(&format!("Rebased total_unclaimed: {} -> {}", expected, new_value).as_str());

    Ok(())
}
