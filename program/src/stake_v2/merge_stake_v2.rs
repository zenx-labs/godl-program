use godl_api::prelude::*;
use solana_program::log::sol_log;
use spl_associated_token_account::get_associated_token_address;
use spl_token::amount_to_ui_amount;
use steel::*;

use crate::stake_v2::stake_multiplier;

/// Merges the source stake account into the target (same authority, no NFT on
/// either), then closes the source, returning its vault and account rent to
/// the signer.
///
/// The merged account takes the LONGER `lock_duration` of the two, with
/// `created_at` reset to now and `multiplier` recomputed from that duration —
/// i.e. exactly the terms a fresh deposit of the combined balance with that
/// lock would get. Restarting the lock is what makes the multiplier upgrade of
/// the shorter-locked balance sound: the whole balance re-commits for the full
/// duration, so merging grants no weight the user could not get by depositing
/// anew.
pub fn process_merge_stake_v2(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    // Parse data.
    let args = MergeStakeV2::try_from_bytes(data)?;
    let target_id = u64::from_le_bytes(args.target_id);
    let source_id = u64::from_le_bytes(args.source_id);

    // Distinct accounts (the PDAs are distinct iff the ids are).
    if target_id == source_id {
        return Err(GodlError::MergeSameStake.into());
    }

    // Load accounts.
    let clock = Clock::get()?;
    let [signer_info, mint_info, target_stake_info, target_tokens_info, source_stake_info, source_tokens_info, treasury_info, system_program, token_program, associated_token_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    mint_info.has_address(&MINT_ADDRESS)?.as_mint()?;
    target_stake_info.is_writable()?.has_seeds(
        &[STAKE_V2, &signer_info.key.to_bytes(), &target_id.to_le_bytes()],
        &godl_api::ID,
    )?;
    source_stake_info.is_writable()?.has_seeds(
        &[STAKE_V2, &signer_info.key.to_bytes(), &source_id.to_le_bytes()],
        &godl_api::ID,
    )?;
    let target = target_stake_info
        .as_account_mut::<StakeV2>(&godl_api::ID)?
        .assert_mut(|s| s.authority == *signer_info.key)?
        .assert_mut(|s| s.id == target_id)?;
    let source = source_stake_info
        .as_account_mut::<StakeV2>(&godl_api::ID)?
        .assert_mut(|s| s.authority == *signer_info.key)?
        .assert_mut(|s| s.id == source_id)?;
    let treasury = treasury_info.as_account_mut::<Treasury>(&godl_api::ID)?;
    // Verify the source vault is the source's canonical ATA, but tolerate it
    // being closed/uninitialized (the drained-vault shape close_stake_v2 also
    // tolerates): a missing vault simply backs zero tokens.
    source_tokens_info.has_address(&get_associated_token_address(
        source_stake_info.key,
        &MINT_ADDRESS,
    ))?;
    let source_vault_amount = if source_tokens_info.data_is_empty() {
        0
    } else {
        source_tokens_info
            .as_associated_token_account(source_stake_info.key, mint_info.key)?
            .amount()
    };
    system_program.is_program(&system_program::ID)?;
    token_program.is_program(&spl_token::ID)?;
    associated_token_program.is_program(&spl_associated_token_account::ID)?;

    // A staked NFT is owned by its stake PDA (closing the source would strand
    // it) and its boost must not double-count through a merge; unstake first.
    if target.is_nft_staked != 0 || source.is_nft_staked != 0 {
        return Err(GodlError::NftAlreadyStaked.into());
    }

    // The source vault must back the recorded balance (handler invariant).
    if source_vault_amount < source.balance {
        return Err(ProgramError::InvalidAccountData);
    }

    // Target vault may be missing; create it lazily like deposit_v2.
    if target_tokens_info.data_is_empty() {
        create_associated_token_account(
            signer_info,
            target_stake_info,
            target_tokens_info,
            mint_info,
            system_program,
            token_program,
            associated_token_program,
        )?;
    } else {
        target_tokens_info.as_associated_token_account(target_stake_info.key, mint_info.key)?;
    }

    // Auto-migrate both to the sqrt weight curve, then settle both at their
    // CURRENT weights before any balance/multiplier change.
    target.migrate_weight(treasury)?;
    source.migrate_weight(treasury)?;
    target.update_rewards(treasury)?;
    source.update_rewards(treasury)?;

    let prev_units = target
        .weighted_units()?
        .checked_add(source.weighted_units()?)
        .ok_or(GodlError::StakeOverflow)?;

    // Fold the source into the target on fresh-deposit-equivalent lock terms.
    target.balance = target
        .balance
        .checked_add(source.balance)
        .ok_or(GodlError::StakeOverflow)?;
    target.rewards = target
        .rewards
        .checked_add(source.rewards)
        .ok_or(GodlError::StakeOverflow)?;
    target.lifetime_rewards = target
        .lifetime_rewards
        .checked_add(source.lifetime_rewards)
        .ok_or(GodlError::StakeOverflow)?;
    target.lock_duration = target.lock_duration.max(source.lock_duration);
    target.created_at = clock.unix_timestamp;
    target.multiplier = stake_multiplier(target.lock_duration);
    target.last_deposit_at = clock.unix_timestamp;
    source.balance = 0;
    source.rewards = 0;

    // The source now contributes zero, so the new sum is the target alone.
    let new_units = target.weighted_units()?;
    if new_units >= prev_units {
        treasury.total_staked = treasury
            .total_staked
            .checked_add(new_units - prev_units)
            .ok_or(GodlError::StakeOverflow)?;
    } else {
        treasury.total_staked = treasury
            .total_staked
            .checked_sub(prev_units - new_units)
            .ok_or(GodlError::StakeOverflow)?;
    }

    // Move the entire source vault (balance + any dust) into the target vault.
    if source_vault_amount > 0 {
        transfer_signed(
            source_stake_info,
            source_tokens_info,
            target_tokens_info,
            token_program,
            source_vault_amount,
            &[STAKE_V2, &signer_info.key.to_bytes(), &source_id.to_le_bytes()],
        )?;
    }

    // Close the source vault (if it exists) and account, returning the rents
    // to the signer.
    if !source_tokens_info.data_is_empty() {
        close_token_account_signed(
            source_tokens_info,
            signer_info,
            source_stake_info,
            token_program,
            &[STAKE_V2, &signer_info.key.to_bytes(), &source_id.to_le_bytes()],
        )?;
    }

    // Log merge.
    sol_log(
        &format!(
            "Merging stake {} into {}: {} GODL at lock {}s",
            source_stake_info.key,
            target_stake_info.key,
            amount_to_ui_amount(target.balance, TOKEN_DECIMALS),
            target.lock_duration,
        )
        .as_str(),
    );

    source_stake_info.close(signer_info)?;

    // Safety check.
    let target_tokens =
        target_tokens_info.as_associated_token_account(target_stake_info.key, mint_info.key)?;
    if target_tokens.amount() < target.balance {
        return Err(ProgramError::InvalidAccountData);
    }

    Ok(())
}
