use godl_api::prelude::*;
use steel::*;

/// Initializes the OTC treasury PDA and its GODL associated token account. Deployer-only.
pub fn process_initialize_otc_treasury(
    accounts: &[AccountInfo<'_>],
    _data: &[u8],
) -> ProgramResult {
    // Load accounts.
    let [signer_info, mint_info, otc_treasury_info, otc_treasury_tokens_info, system_program, token_program, associated_token_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    signer_info.has_address(&OTC_ORACLE_SIGNER)?;
    mint_info.has_address(&MINT_ADDRESS)?.as_mint()?;
    otc_treasury_info
        .is_empty()?
        .is_writable()?
        .has_seeds(&[OTC_TREASURY], &godl_api::ID)?;
    otc_treasury_tokens_info.is_writable()?;
    system_program.is_program(&system_program::ID)?;
    token_program.is_program(&spl_token::ID)?;
    associated_token_program.is_program(&spl_associated_token_account::ID)?;

    // Create the OTC treasury PDA.
    create_program_account::<OtcTreasury>(
        otc_treasury_info,
        system_program,
        signer_info,
        &godl_api::ID,
        &[OTC_TREASURY],
    )?;

    // Initialize the OTC treasury.
    let otc_treasury = otc_treasury_info.as_account_mut::<OtcTreasury>(&godl_api::ID)?;
    otc_treasury.sol_balance = 0;
    otc_treasury.godl_balance = 0;
    otc_treasury.buffer = [0; 32];

    // Create the OTC treasury GODL ATA if it doesn't exist.
    if otc_treasury_tokens_info.data_is_empty() {
        create_associated_token_account(
            signer_info,
            otc_treasury_info,
            otc_treasury_tokens_info,
            mint_info,
            system_program,
            token_program,
            associated_token_program,
        )?;
    } else {
        otc_treasury_tokens_info
            .as_associated_token_account(otc_treasury_info.key, mint_info.key)?;
    }

    Ok(())
}
