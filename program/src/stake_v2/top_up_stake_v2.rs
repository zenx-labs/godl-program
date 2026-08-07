use godl_api::prelude::*;
use solana_program::log::sol_log;
use spl_token::amount_to_ui_amount;
use steel::*;

/// Deposits additional GODL into an existing stake account, RESTARTING the
/// lock: `created_at` is reset to now, so the entire balance is locked for the
/// full `lock_duration` again.
///
/// The reset is what makes topping up locked (or expired-lock) accounts sound:
/// `multiplier` is frozen at creation and never re-derived, and an account
/// with `created_at = now` carries exactly the fresh-deposit-equivalent terms
/// that multiplier was priced at — new capital gains nothing it could not get
/// by depositing anew, and the existing balance pays by re-committing.
pub fn process_top_up_stake_v2(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    // Parse data.
    let args = TopUpStakeV2::try_from_bytes(data)?;
    let id = u64::from_le_bytes(args.id);
    let amount = u64::from_le_bytes(args.amount);

    // Load accounts.
    let clock = Clock::get()?;
    let [signer_info, mint_info, sender_info, stake_info, stake_tokens_info, treasury_info, system_program, token_program, associated_token_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    mint_info.has_address(&MINT_ADDRESS)?.as_mint()?;
    let sender = sender_info
        .is_writable()?
        .as_associated_token_account(&signer_info.key, &MINT_ADDRESS)?;
    stake_info.is_writable()?.has_seeds(
        &[STAKE_V2, &signer_info.key.to_bytes(), &id.to_le_bytes()],
        &godl_api::ID,
    )?;
    if stake_info.data_is_empty() {
        return Err(ProgramError::UninitializedAccount);
    }
    let stake = stake_info
        .as_account_mut::<StakeV2>(&godl_api::ID)?
        .assert_mut(|s| s.authority == *signer_info.key)?
        .assert_mut(|s| s.id == id)?;
    let treasury = treasury_info.as_account_mut::<Treasury>(&godl_api::ID)?;
    system_program.is_program(&system_program::ID)?;
    token_program.is_program(&spl_token::ID)?;
    associated_token_program.is_program(&spl_associated_token_account::ID)?;

    // Validate the amount
    if amount == 0 {
        return Err(GodlError::AmountTooSmall.into());
    }

    // Auto-migrate to the sqrt weight curve on touch (no-op once version 1).
    stake.migrate_weight(treasury)?;

    // Create stake tokens account.
    if stake_tokens_info.data_is_empty() {
        create_associated_token_account(
            signer_info,
            stake_info,
            stake_tokens_info,
            mint_info,
            system_program,
            token_program,
            associated_token_program,
        )?;
    } else {
        stake_tokens_info.as_associated_token_account(stake_info.key, mint_info.key)?;
    }

    // Deposit into stake account (settles rewards before the balance change and
    // adds the exact weighted delta to treasury.total_staked).
    let amount = stake.deposit(amount, &clock, treasury, &sender)?;

    // deposit() clamps to the sender's token balance; a zero-effective top-up
    // must fail rather than restart the lock below (which would re-lock the
    // entire existing balance in exchange for depositing nothing).
    if amount == 0 {
        return Err(GodlError::AmountTooSmall.into());
    }

    // Restart the lock: the whole balance is committed for the full
    // lock_duration from now (a no-op for never-locked accounts).
    stake.created_at = clock.unix_timestamp;

    // Transfer GODL to the stake vault.
    transfer(
        signer_info,
        sender_info,
        stake_tokens_info,
        token_program,
        amount,
    )?;

    // Log top-up.
    sol_log(
        &format!(
            "Topping up {} GODL",
            amount_to_ui_amount(amount, TOKEN_DECIMALS)
        )
        .as_str(),
    );

    // Safety check.
    let stake_tokens =
        stake_tokens_info.as_associated_token_account(stake_info.key, mint_info.key)?;
    if stake_tokens.amount() < stake.balance {
        return Err(ProgramError::InvalidAccountData);
    }

    Ok(())
}
