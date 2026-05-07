use godl_api::prelude::*;
use solana_program::{log::sol_log, native_token::lamports_to_sol};
use steel::*;

/// Permissionless: drain the OTC desk's accumulated SOL balance to CHEST_ADDRESS.
pub fn process_withdraw_sol_otc(accounts: &[AccountInfo<'_>], _data: &[u8]) -> ProgramResult {
    // Load accounts.
    let [signer_info, otc_treasury_info, chest_info, system_program] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    otc_treasury_info
        .is_writable()?
        .has_seeds(&[OTC_TREASURY], &godl_api::ID)?;
    let otc_treasury = otc_treasury_info.as_account_mut::<OtcTreasury>(&godl_api::ID)?;
    chest_info.is_writable()?.has_address(&CHEST_ADDRESS)?;
    system_program.is_program(&system_program::ID)?;

    // Snapshot and zero out the tracked balance.
    let amount = otc_treasury.sol_balance;
    if amount == 0 {
        return Ok(());
    }
    otc_treasury.sol_balance = 0;

    // Forward lamports from the OTC PDA to the chest.
    otc_treasury_info.send(amount, chest_info);

    sol_log(
        format!(
            "Withdrew {} SOL from OTC treasury to chest",
            lamports_to_sol(amount)
        )
        .as_str(),
    );

    Ok(())
}
