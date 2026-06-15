use godl_api::prelude::*;
use solana_program::log::sol_log;
use spl_token::amount_to_ui_amount;
use steel::*;

pub fn stake_multiplier(lock_duration: i64) -> u64 {
    let d = lock_duration.max(0).min(MAX_LOCK_DURATION) as u64;

    // Linear interpolation between 1x and MAX_STAKE_MULTIPLIER, in fixed point.
    let base = STAKE_MULTIPLIER_SCALE as u128;
    let extra = (MAX_STAKE_MULTIPLIER.saturating_sub(1) as u128)
        .saturating_mul(d as u128)
        .saturating_mul(STAKE_MULTIPLIER_SCALE as u128)
        / (MAX_LOCK_DURATION as u128);

    (base.saturating_add(extra)) as u64
}

/// Deposits GODL into the staking contract.
pub fn process_deposit_v2(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    // Parse data.
    let args = DepositV2::try_from_bytes(data)?;
    let id = u64::from_le_bytes(args.id);
    let amount = u64::from_le_bytes(args.amount);
    let lock_duration = i64::from_le_bytes(args.lock_duration);

    // Load accounts.
    let clock = Clock::get()?;
    let [signer_info, mint_info, sender_info, stake_info, stake_tokens_info, treasury_info, executor_info, system_program, token_program, associated_token_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    mint_info.has_address(&MINT_ADDRESS)?.as_mint()?;
    let sender = sender_info
        .is_writable()?
        .as_associated_token_account(&signer_info.key, &MINT_ADDRESS)?;
    stake_info.is_writable()?;
    let treasury = treasury_info.as_account_mut::<Treasury>(&godl_api::ID)?;
    system_program.is_program(&system_program::ID)?;
    token_program.is_program(&spl_token::ID)?;
    associated_token_program.is_program(&spl_associated_token_account::ID)?;

    // Validate the lock duration
    if lock_duration < 0 || lock_duration > MAX_LOCK_DURATION {
        return Err(GodlError::InvalidLockDuration.into());
    }

    // Validate the amount
    if amount == 0 {
        return Err(GodlError::AmountTooSmall.into());
    }

    // Disallow deposits into existing stake accounts.
    if !stake_info.data_is_empty() {
        return Err(GodlError::StakeAlreadyExists.into());
    }

    stake_info.has_seeds(
        &[STAKE_V2, &signer_info.key.to_bytes(), &id.to_le_bytes()],
        &godl_api::ID,
    )?;

    // Create new stake account.
    create_program_account::<StakeV2>(
        stake_info,
        system_program,
        &signer_info,
        &godl_api::ID,
        &[STAKE_V2, &signer_info.key.to_bytes(), &id.to_le_bytes()],
    )?;
    let stake = stake_info.as_account_mut::<StakeV2>(&godl_api::ID)?;
    stake.id = id;
    stake.authority = *signer_info.key;
    stake.balance = 0;
    stake.last_claim_at = 0;
    stake.last_deposit_at = 0;
    stake.last_withdraw_at = 0;
    stake.rewards_factor = treasury.stake_rewards_factor;
    stake.rewards = 0;
    stake.multiplier = stake_multiplier(lock_duration);
    stake.lifetime_rewards = 0;
    stake.lock_duration = lock_duration;
    stake.executor = *executor_info.key;
    stake.created_at = clock.unix_timestamp;
    stake.is_nft_staked = 0;
    stake.buffer = [0; 31];

    // Settle rewards with the current multiplier before any changes.
    stake.update_rewards(treasury)?;

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

    // Deposit into stake account.
    let amount = stake.deposit(amount, &clock, treasury, &sender)?;

    // Transfer GODL to treasury.
    transfer(
        signer_info,
        sender_info,
        stake_tokens_info,
        token_program,
        amount,
    )?;

    // Log deposit.
    sol_log(
        &format!(
            "Depositing {} GODL",
            amount_to_ui_amount(amount, TOKEN_DECIMALS)
        )
        .as_str(),
    );

    // Safety check.
    let stake_tokens =
        stake_tokens_info.as_associated_token_account(stake_info.key, mint_info.key)?;
    assert!(stake_tokens.amount() >= stake.balance);

    Ok(())
}
