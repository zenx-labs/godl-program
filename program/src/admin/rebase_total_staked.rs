use godl_api::prelude::*;
use solana_program::log::sol_log;
use steel::*;

/// Admin-only compare-and-swap on `treasury.total_staked`. Verification
/// backstop for the sqrt-weight migration sweep: if the on-chain value no
/// longer matches `expected` (a competing stake op landed between the
/// off-chain snapshot and this transaction), the instruction fails without
/// mutating anything and the caller recomputes and retries.
pub fn process_rebase_total_staked(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    let args = RebaseTotalStaked::try_from_bytes(data)?;
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

    if treasury.total_staked != expected {
        sol_log(
            &format!(
                "Rebase mismatch: expected {}, on-chain {}",
                expected, treasury.total_staked,
            )
            .as_str(),
        );
        return Err(GodlError::RebaseMismatch.into());
    }

    treasury.total_staked = new_value;
    sol_log(&format!("Rebased total_staked: {} -> {}", expected, new_value).as_str());

    Ok(())
}
