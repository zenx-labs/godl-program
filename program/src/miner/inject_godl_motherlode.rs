use godl_api::prelude::*;
use solana_program::log::sol_log;
use spl_token::amount_to_ui_amount;
use steel::*;

/// Injects GODL into the motherlode rewards pool. Can be called by anyone.
pub fn process_inject_godl_motherlode(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    // Parse data.
    let args = InjectGodlMotherlode::try_from_bytes(data)?;
    let amount = u64::from_le_bytes(args.amount);

    // Load accounts.
    let [signer_info, treasury_info, signer_tokens_info, treasury_tokens_info, token_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    treasury_info
        .is_writable()?
        .has_seeds(&[TREASURY], &godl_api::ID)?;
    let treasury = treasury_info.as_account_mut::<Treasury>(&godl_api::ID)?;
    signer_tokens_info
        .is_writable()?
        .as_associated_token_account(signer_info.key, &MINT_ADDRESS)?;
    treasury_tokens_info
        .is_writable()?
        .as_associated_token_account(treasury_info.key, &MINT_ADDRESS)?;
    token_program.is_program(&spl_token::ID)?;

    // Validate amount.
    if amount == 0 {
        return Err(GodlError::AmountTooSmall.into());
    }

    // Transfer GODL from signer to treasury.
    transfer(
        signer_info,
        signer_tokens_info,
        treasury_tokens_info,
        token_program,
        amount,
    )?;

    // Increase the motherlode pool.
    treasury.motherlode = treasury
        .motherlode
        .checked_add(amount)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    sol_log(
        format!(
            "Injected {} GODL into motherlode",
            amount_to_ui_amount(amount, TOKEN_DECIMALS),
        )
        .as_str(),
    );

    Ok(())
}
