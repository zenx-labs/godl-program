use godl_api::prelude::*;
use spl_token_2022::instruction::AuthorityType;
use steel::*;

/// Transfers the GODL mint authority from the treasury PDA to the godl-mint
/// program's authority PDA.
pub fn process_transfer_mint_authority(
    accounts: &[AccountInfo<'_>],
    _data: &[u8],
) -> ProgramResult {
    // Load accounts.
    let [signer_info, config_info, mint_info, treasury_info, godl_mint_authority_info, token_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    config_info
        .as_account::<Config>(&godl_api::ID)?
        .assert_err(
            |c| c.admin == *signer_info.key,
            GodlError::NotAuthorized.into(),
        )?;
    mint_info.has_address(&MINT_ADDRESS)?.as_mint()?;
    treasury_info
        .is_writable()?
        .has_seeds(&[TREASURY], &godl_api::ID)?;
    godl_mint_authority_info.has_address(&godl_mint_api::state::authority_pda().0)?;
    token_program.is_program(&spl_token::ID)?;

    // Transfer mint authority.
    set_authority_signed(
        mint_info,
        treasury_info,
        Some(godl_mint_authority_info),
        AuthorityType::MintTokens,
        token_program,
        &[TREASURY],
    )?;

    Ok(())
}
