use godl_api::prelude::*;
use solana_program::log::sol_log;
use steel::*;

/// Permissionlessly migrates a StakeV2 account from the linear weight curve
/// (version 0) to the sqrt curve (version 1). Idempotent: an already-migrated
/// account is a no-op. Pending rewards are settled at the OLD weight before
/// the flip, then `treasury.total_staked` is adjusted by exactly the
/// weighted-units delta (see `StakeV2::migrate_weight`).
///
/// Like `reconcile_stake_v2`, this deliberately does NOT enforce the canonical
/// PDA seeds: the exploit-era on-curve stake accounts must be migratable too.
/// `as_account_mut` still validates owner + discriminant, and the instruction
/// moves no tokens, so permissionless + seedless is safe.
pub fn process_migrate_stake_weight(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    MigrateStakeWeight::try_from_bytes(data)?;

    let [signer_info, stake_info, treasury_info] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;

    let stake = stake_info
        .is_writable()?
        .as_account_mut::<StakeV2>(&godl_api::ID)?;
    let treasury = treasury_info
        .is_writable()?
        .has_seeds(&[TREASURY], &godl_api::ID)?
        .as_account_mut::<Treasury>(&godl_api::ID)?;

    if stake.weight_version != 0 {
        return Ok(());
    }

    let prev_units = stake.weighted_units()?;
    stake.migrate_weight(treasury)?;
    let new_units = stake.weighted_units()?;

    sol_log(
        &format!(
            "Migrated stake {} to sqrt weight: {} -> {}",
            stake_info.key, prev_units, new_units,
        )
        .as_str(),
    );

    Ok(())
}
