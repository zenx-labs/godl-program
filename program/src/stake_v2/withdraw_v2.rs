use godl_api::prelude::*;
use solana_program::log::sol_log;
use spl_token::amount_to_ui_amount;
use steel::*;

/// Withdraws GODL from the staking contract.
pub fn process_withdraw_v2(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    // Parse data.
    let args = WithdrawV2::try_from_bytes(data)?;
    let id = u64::from_le_bytes(args.id);
    let amount = u64::from_le_bytes(args.amount);

    // Load accounts.
    let clock = Clock::get()?;
    let [signer_info, mint_info, recipient_info, stake_info, stake_tokens_info, treasury_info, system_program, token_program, associated_token_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    mint_info.has_address(&MINT_ADDRESS)?.as_mint()?;
    recipient_info.is_writable()?;
    stake_info.has_seeds(
        &[STAKE_V2, &signer_info.key.to_bytes(), &id.to_le_bytes()],
        &godl_api::ID,
    )?;
    let stake = stake_info
        .as_account_mut::<StakeV2>(&godl_api::ID)?
        .assert_mut(|s| s.authority == *signer_info.key)?
        .assert_mut(|s| s.id == id)?;
    stake_tokens_info.as_associated_token_account(stake_info.key, mint_info.key)?;
    let treasury = treasury_info.as_account_mut::<Treasury>(&godl_api::ID)?;
    system_program.is_program(&system_program::ID)?;
    token_program.is_program(&spl_token::ID)?;
    associated_token_program.is_program(&spl_associated_token_account::ID)?;

    // Auto-migrate to the sqrt weight curve on touch (no-op once version 1).
    stake.migrate_weight(treasury)?;

    let unlock_at = stake.created_at.saturating_add(stake.lock_duration.max(0));

    // Check if the stake is locked.
    if clock.unix_timestamp < unlock_at {
        return Err(GodlError::StakeLocked.into());
    }

    // Validate the amount
    if amount == 0 {
        return Err(GodlError::AmountTooSmall.into());
    }

    // Log withdraw.
    sol_log(
        &format!(
            "Withdrawing {} GODL",
            amount_to_ui_amount(amount, TOKEN_DECIMALS)
        )
        .as_str(),
    );

    // Open recipient token account.
    if recipient_info.data_is_empty() {
        create_associated_token_account(
            signer_info,
            signer_info,
            recipient_info,
            mint_info,
            system_program,
            token_program,
            associated_token_program,
        )?;
    } else {
        recipient_info.as_associated_token_account(&signer_info.key, &mint_info.key)?;
    }

    // Deposit into stake account.
    let amount = stake.withdraw(amount, &clock, treasury)?;

    // Transfer GODL to recipient.
    transfer_signed(
        stake_info,
        stake_tokens_info,
        recipient_info,
        token_program,
        amount,
        &[STAKE_V2, &signer_info.key.to_bytes(), &id.to_le_bytes()],
    )?;

    // Safety check.
    let stake_tokens =
        stake_tokens_info.as_associated_token_account(stake_info.key, mint_info.key)?;
    assert!(stake_tokens.amount() >= stake.balance);

    Ok(())
}
