use godl_api::prelude::*;
use solana_program::log::sol_log;
use spl_token::amount_to_ui_amount;
use steel::*;

/// Permissionless: deposit GODL into the OTC desk's GODL balance.
pub fn process_fund_godl_otc(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    // Parse data.
    let args = FundGodlOtc::try_from_bytes(data)?;
    let amount = u64::from_le_bytes(args.amount);

    // Load accounts.
    let [signer_info, mint_info, signer_tokens_info, otc_treasury_info, otc_treasury_tokens_info, token_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    mint_info.has_address(&MINT_ADDRESS)?.as_mint()?;
    signer_tokens_info
        .is_writable()?
        .as_associated_token_account(signer_info.key, &MINT_ADDRESS)?;
    otc_treasury_info
        .is_writable()?
        .has_seeds(&[OTC_TREASURY], &godl_api::ID)?;
    let otc_treasury = otc_treasury_info.as_account_mut::<OtcTreasury>(&godl_api::ID)?;
    otc_treasury_tokens_info
        .is_writable()?
        .as_associated_token_account(otc_treasury_info.key, &MINT_ADDRESS)?;
    token_program.is_program(&spl_token::ID)?;

    // Validate amount.
    if amount == 0 {
        return Err(GodlError::AmountTooSmall.into());
    }

    // Transfer GODL from signer to the OTC treasury ATA.
    transfer(
        signer_info,
        signer_tokens_info,
        otc_treasury_tokens_info,
        token_program,
        amount,
    )?;

    // Increase the OTC treasury's GODL balance.
    otc_treasury.godl_balance = otc_treasury
        .godl_balance
        .checked_add(amount)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    sol_log(
        format!(
            "Funded OTC treasury with {} GODL",
            amount_to_ui_amount(amount, TOKEN_DECIMALS),
        )
        .as_str(),
    );

    Ok(())
}
