use godl_api::prelude::*;
use solana_program::log::sol_log;
use spl_token::amount_to_ui_amount;
use steel::*;

/// Claims rewards and re-deposits them into the stake account.
pub fn process_compound_yield_v2(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    // Parse data.
    let args = CompoundYieldV2::try_from_bytes(data)?;
    let id = u64::from_le_bytes(args.id);

    // Load accounts.
    let clock = Clock::get()?;
    let [signer_info, mint_info, stake_info, stake_tokens_info, treasury_info, treasury_tokens_info, system_program, token_program, associated_token_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    stake_info.is_writable()?;
    stake_tokens_info.is_writable()?;
    treasury_tokens_info.is_writable()?;
    mint_info
        .has_address(&MINT_ADDRESS)?
        .is_writable()?
        .as_mint()?;
    treasury_info
        .is_writable()?
        .has_seeds(&[TREASURY], &godl_api::ID)?;
    let treasury = treasury_info.as_account_mut::<Treasury>(&godl_api::ID)?;
    system_program.is_program(&system_program::ID)?;
    token_program.is_program(&spl_token::ID)?;
    associated_token_program.is_program(&spl_associated_token_account::ID)?;

    let stake = stake_info
        .as_account_mut::<StakeV2>(&godl_api::ID)?
        .assert_mut(|s| s.id == id)?;

    // Only the stake authority or delegated executor may compound.
    if stake.authority != *signer_info.key && stake.executor != *signer_info.key {
        return Err(GodlError::NotAuthorized.into());
    }

    let authority = stake.authority;
    stake_info.has_seeds(
        &[STAKE_V2, &authority.to_bytes(), &id.to_le_bytes()],
        &godl_api::ID,
    )?;

    stake_tokens_info.as_associated_token_account(stake_info.key, mint_info.key)?;

    // Validate treasury token account.
    treasury_tokens_info
        .is_writable()?
        .as_associated_token_account(&treasury_info.key, &mint_info.key)?;

    // Auto-migrate to the sqrt weight curve on touch (no-op once version 1).
    stake.migrate_weight(treasury)?;

    // Claim all pending rewards.
    let compounded = stake.claim(u64::MAX, &clock, treasury)?;
    if compounded == 0 {
        return Err(GodlError::AmountTooSmall.into());
    }

    let fee = compounded / 100;

    if fee > 0 {
        burn_signed(
            treasury_tokens_info,
            mint_info,
            treasury_info,
            token_program,
            fee,
            &[TREASURY],
        )?;
    }

    // Move rewards from treasury to the stake's token account.
    let amount = compounded - fee;
    transfer_signed(
        treasury_info,
        treasury_tokens_info,
        stake_tokens_info,
        token_program,
        amount,
        &[TREASURY],
    )?;

    // Refresh stake token account and deposit compounded rewards.
    let stake_tokens =
        stake_tokens_info.as_associated_token_account(stake_info.key, mint_info.key)?;
    let deposited = stake.deposit(amount, &clock, treasury, &stake_tokens)?;

    sol_log(
        &format!(
            "Compounded {} GODL",
            amount_to_ui_amount(deposited, TOKEN_DECIMALS)
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
