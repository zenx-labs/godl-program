use godl_api::prelude::*;
use solana_program::{log::sol_log, native_token::lamports_to_sol};
use spl_token::amount_to_ui_amount;
use steel::*;

use crate::stake_v2::stake_multiplier;

/// Executes an OTC quote signed by both the buyer and the OTC oracle.
///
/// - Pulls `sol_in` lamports from the buyer into the OTC treasury PDA.
/// - Mints a fresh 1-month locked StakeV2 owned by the buyer and deposits `godl_out` into it.
/// - Injects `godl_bonus` as unrefined miner rewards (creating the miner if missing).
/// - Tracks per-user totals in the buyer's OtcUser PDA.
pub fn process_execute_otc_trade(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    // Parse data.
    let args = ExecuteOtcTrade::try_from_bytes(data)?;
    let stake_id = u64::from_le_bytes(args.stake_id);
    let sol_in = u64::from_le_bytes(args.sol_in);
    let godl_out = u64::from_le_bytes(args.godl_out);
    let godl_bonus = u64::from_le_bytes(args.godl_bonus);
    let expiry_slot = u64::from_le_bytes(args.expiry_slot);
    let lock_duration = i64::from_le_bytes(args.lock_duration);

    // Load accounts.
    let clock = Clock::get()?;
    let [buyer_info, oracle_info, mint_info, miner_info, otc_user_info, otc_treasury_info, otc_treasury_tokens_info, treasury_info, treasury_tokens_info, stake_info, stake_tokens_info, system_program, token_program, associated_token_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // Both the buyer and the OTC oracle must sign.
    buyer_info.is_signer()?;
    oracle_info
        .is_signer()?
        .has_address(&OTC_ORACLE_SIGNER)?;

    mint_info.has_address(&MINT_ADDRESS)?.as_mint()?;
    miner_info
        .is_writable()?
        .has_seeds(&[MINER, &buyer_info.key.to_bytes()], &godl_api::ID)?;
    otc_user_info
        .is_writable()?
        .has_seeds(&[OTC_USER, &buyer_info.key.to_bytes()], &godl_api::ID)?;
    otc_treasury_info
        .is_writable()?
        .has_seeds(&[OTC_TREASURY], &godl_api::ID)?;
    let otc_treasury_tokens = otc_treasury_tokens_info
        .is_writable()?
        .as_associated_token_account(otc_treasury_info.key, &MINT_ADDRESS)?;
    treasury_info
        .is_writable()?
        .has_seeds(&[TREASURY], &godl_api::ID)?;
    treasury_tokens_info
        .is_writable()?
        .as_associated_token_account(treasury_info.key, &MINT_ADDRESS)?;
    stake_info
        .is_writable()?
        .has_seeds(
            &[STAKE_V2, &buyer_info.key.to_bytes(), &stake_id.to_le_bytes()],
            &godl_api::ID,
        )?;
    stake_tokens_info.is_writable()?;
    system_program.is_program(&system_program::ID)?;
    token_program.is_program(&spl_token::ID)?;
    associated_token_program.is_program(&spl_associated_token_account::ID)?;

    // Quote-expiry guard. The oracle attaches a slot horizon so a stale signed
    // quote can't be replayed after the price has moved.
    if clock.slot > expiry_slot {
        return Err(GodlError::OtcQuoteExpired.into());
    }

    // Basic input validation.
    if sol_in == 0 || godl_out == 0 {
        return Err(GodlError::AmountTooSmall.into());
    }

    // Lock duration must be at least 1 month and at most MAX_LOCK_DURATION.
    if lock_duration < ONE_MONTH || lock_duration > MAX_LOCK_DURATION {
        return Err(GodlError::InvalidLockDuration.into());
    }

    // Fresh stake account is required (we always mint a new locked stake).
    if !stake_info.data_is_empty() {
        return Err(GodlError::StakeAlreadyExists.into());
    }

    // === Validate + apply all OTC state changes in a single mut borrow ===
    // We deliberately update sol_balance/godl_balance before the CPIs that move
    // tokens. The transaction is atomic, so a CPI failure rolls everything back.
    // This keeps `otc_treasury_info`'s data borrow short, leaving the later
    // `collect` and `transfer_signed` CPIs free to re-borrow it.
    let total_godl_required = godl_out
        .checked_add(godl_bonus)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    {
        let otc_treasury = otc_treasury_info.as_account_mut::<OtcTreasury>(&godl_api::ID)?;
        if otc_treasury.godl_balance < total_godl_required {
            return Err(GodlError::InsufficientOtcGodlBalance.into());
        }
        otc_treasury.sol_balance = otc_treasury
            .sol_balance
            .checked_add(sol_in)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        otc_treasury.godl_balance = otc_treasury
            .godl_balance
            .checked_sub(total_godl_required)
            .ok_or(GodlError::InsufficientOtcGodlBalance)?;
    }

    // === SOL leg ===
    // Move sol_in lamports from the buyer to the OTC treasury PDA.
    otc_treasury_info.collect(sol_in, buyer_info)?;

    // === Lazy-create miner if missing ===
    let miner = if miner_info.data_is_empty() {
        create_program_account::<Miner>(
            miner_info,
            system_program,
            buyer_info,
            &godl_api::ID,
            &[MINER, &buyer_info.key.to_bytes()],
        )?;
        let miner = miner_info.as_account_mut::<Miner>(&godl_api::ID)?;
        miner.authority = *buyer_info.key;
        miner.referrer = Pubkey::default();
        miner.deployed = [0; 25];
        miner.cumulative = [0; 25];
        miner.rewards_sol = 0;
        miner.rewards_godl = 0;
        miner.round_id = 0;
        miner.checkpoint_id = 0;
        miner.lifetime_deployed = 0;
        miner.lifetime_rewards_sol = 0;
        miner.lifetime_rewards_godl = 0;
        miner
    } else {
        let miner = miner_info.as_account_mut::<Miner>(&godl_api::ID)?;
        miner.assert(|m| m.authority == *buyer_info.key)?;
        miner
    };

    // === Lazy-create OtcUser if missing ===
    let otc_user = if otc_user_info.data_is_empty() {
        create_program_account::<OtcUser>(
            otc_user_info,
            system_program,
            buyer_info,
            &godl_api::ID,
            &[OTC_USER, &buyer_info.key.to_bytes()],
        )?;
        let otc_user = otc_user_info.as_account_mut::<OtcUser>(&godl_api::ID)?;
        otc_user.authority = *buyer_info.key;
        otc_user.total_sol_in = 0;
        otc_user.total_godl_out = 0;
        otc_user.total_godl_bonus = 0;
        otc_user.buffer = [0; 32];
        otc_user
    } else {
        let otc_user = otc_user_info.as_account_mut::<OtcUser>(&godl_api::ID)?;
        otc_user.assert(|u| u.authority == *buyer_info.key)?;
        otc_user
    };

    // === Create the new locked StakeV2 (1 month) ===
    create_program_account::<StakeV2>(
        stake_info,
        system_program,
        buyer_info,
        &godl_api::ID,
        &[STAKE_V2, &buyer_info.key.to_bytes(), &stake_id.to_le_bytes()],
    )?;
    let treasury = treasury_info.as_account_mut::<Treasury>(&godl_api::ID)?;
    let stake = stake_info.as_account_mut::<StakeV2>(&godl_api::ID)?;
    stake.id = stake_id;
    stake.authority = *buyer_info.key;
    stake.balance = 0;
    stake.last_claim_at = 0;
    stake.last_deposit_at = 0;
    stake.last_withdraw_at = 0;
    stake.rewards_factor = treasury.stake_rewards_factor;
    stake.rewards = 0;
    stake.multiplier = stake_multiplier(lock_duration);
    stake.lifetime_rewards = 0;
    stake.lock_duration = lock_duration;
    stake.executor = *buyer_info.key;
    stake.created_at = clock.unix_timestamp;
    stake.is_nft_staked = 0;
    stake.buffer = [0; 31];

    // Settle rewards before any balance change.
    stake.update_rewards(treasury)?;

    // Create the stake's GODL ATA (it's brand new).
    if stake_tokens_info.data_is_empty() {
        create_associated_token_account(
            buyer_info,
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

    let amount = stake.deposit(godl_out, &clock, treasury, &otc_treasury_tokens)?;

    // Move godl_out from OTC ATA into the stake's ATA, signed by the OTC PDA.
    transfer_signed(
        otc_treasury_info,
        otc_treasury_tokens_info,
        stake_tokens_info,
        token_program,
        amount,
        &[OTC_TREASURY],
    )?;

    // === Bonus leg (mirrors inject_unrefined_rewards) ===
    if godl_bonus > 0 {
        // Move godl_bonus from OTC ATA to the main treasury ATA so claim_godl can pay it out.
        transfer_signed(
            otc_treasury_info,
            otc_treasury_tokens_info,
            treasury_tokens_info,
            token_program,
            godl_bonus,
            &[OTC_TREASURY],
        )?;

        miner.update_rewards(treasury);
        miner.rewards_godl = miner
            .rewards_godl
            .checked_add(godl_bonus)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        miner.lifetime_rewards_godl = miner
            .lifetime_rewards_godl
            .checked_add(godl_bonus)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        treasury.total_unclaimed = treasury
            .total_unclaimed
            .checked_add(godl_bonus)
            .ok_or(ProgramError::ArithmeticOverflow)?;
    }

    // === Update per-user totals ===
    otc_user.total_sol_in = otc_user
        .total_sol_in
        .checked_add(sol_in)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    otc_user.total_godl_out = otc_user
        .total_godl_out
        .checked_add(godl_out)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    otc_user.total_godl_bonus = otc_user
        .total_godl_bonus
        .checked_add(godl_bonus)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    // Safety check: stake's ATA must hold at least its tracked balance.
    let stake_tokens =
        stake_tokens_info.as_associated_token_account(stake_info.key, mint_info.key)?;
    let stake = stake_info.as_account::<StakeV2>(&godl_api::ID)?;
    assert!(stake_tokens.amount() >= stake.balance);

    sol_log(
        format!(
            "OTC trade: {} SOL in, {} GODL locked, {} GODL bonus",
            lamports_to_sol(sol_in),
            amount_to_ui_amount(godl_out, TOKEN_DECIMALS),
            amount_to_ui_amount(godl_bonus, TOKEN_DECIMALS),
        )
        .as_str(),
    );

    Ok(())
}
