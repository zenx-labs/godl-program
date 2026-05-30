use godl_api::prelude::*;
use solana_program::rent::Rent;
use steel::*;

/// Send SOL from the treasury to the WSOL account.
pub fn process_pre_bury(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    // Parse args.
    let args = PreBury::try_from_bytes(data)?;
    let bury_amount = u64::from_le_bytes(args.bury_amount);
    let chest_amount = u64::from_le_bytes(args.chest_amount);
    let admin_amount = u64::from_le_bytes(args.admin_amount);

    let total_amount = bury_amount + chest_amount + admin_amount;

    // Load accounts.
    let [signer_info, config_info, treasury_info, treasury_sol_info, chest_info, admin_info, system_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    config_info
        .as_account::<Config>(&godl_api::ID)?
        .assert(|c| c.bury_authority == *signer_info.key)?;
    let treasury = treasury_info.as_account_mut::<Treasury>(&godl_api::ID)?;
    treasury_sol_info
        .is_writable()?
        .as_associated_token_account(treasury_info.key, &SOL_MINT)?;
    system_program.is_program(&system_program::ID)?;
    chest_info.has_address(&CHEST_ADDRESS)?;
    admin_info.has_address(&ADMIN_GODL_FEE)?;

    // Validate amount.
    assert!(total_amount > 0, "Amount must be greater than zero");
    assert!(
        total_amount <= treasury.balance,
        "Insufficient vault balance: requested {}, available {}",
        total_amount,
        treasury.balance
    );

    let min_balance = Rent::get()?.minimum_balance(std::mem::size_of::<Treasury>());
    let treasury_lamports = treasury_info.lamports();
    assert!(
        treasury_lamports >= min_balance + total_amount,
        "Insufficient SOL balance: treasury has {} lamports, needs {} (min rent) + {} (pre-bury)",
        treasury_lamports,
        min_balance,
        total_amount
    );

    // Send SOL to the WSOL account.
    treasury_info.send(bury_amount, treasury_sol_info);
    treasury_info.send(chest_amount, chest_info);
    treasury_info.send(admin_amount, admin_info);

    // Update treasury.
    treasury.balance -= total_amount;

    Ok(())
}
