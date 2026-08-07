use godl_api::prelude::*;
use solana_program::log::sol_log;
use spl_associated_token_account::get_associated_token_address;
use spl_token::amount_to_ui_amount;
use steel::*;

/// Closes an empty stake account: claims all pending rewards, sweeps any
/// residual vault tokens, then returns the vault and account rent to the
/// authority.
///
/// The treasury is deliberately loaded read-only (like `ClaimYieldV2`): a
/// balance-0 stake has zero weighted units under every weight version, so
/// deleting it never moves `treasury.total_staked`, and migrating the weight
/// curve on an account about to be deleted would be a pointless write.
pub fn process_close_stake_v2(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    // Parse data.
    let args = CloseStakeV2::try_from_bytes(data)?;
    let id = u64::from_le_bytes(args.id);

    // Load accounts.
    let clock = Clock::get()?;
    let [signer_info, mint_info, recipient_info, stake_info, stake_tokens_info, treasury_info, treasury_tokens_info, system_program, token_program, associated_token_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    mint_info.has_address(&MINT_ADDRESS)?.as_mint()?;
    recipient_info.is_writable()?;
    stake_info.is_writable()?.has_seeds(
        &[STAKE_V2, &signer_info.key.to_bytes(), &id.to_le_bytes()],
        &godl_api::ID,
    )?;
    let stake = stake_info
        .as_account_mut::<StakeV2>(&godl_api::ID)?
        .assert_mut(|s| s.authority == *signer_info.key)?
        .assert_mut(|s| s.id == id)?;
    let treasury = treasury_info.as_account::<Treasury>(&godl_api::ID)?;
    treasury_tokens_info
        .is_writable()?
        .as_associated_token_account(&treasury_info.key, &mint_info.key)?;
    system_program.is_program(&system_program::ID)?;
    token_program.is_program(&spl_token::ID)?;
    associated_token_program.is_program(&spl_associated_token_account::ID)?;

    // Only empty stakes can be closed.
    if stake.balance != 0 {
        return Err(GodlError::StakeNotEmpty.into());
    }

    // A staked NFT is owned by the stake PDA and would be stranded; unstake it
    // first.
    if stake.is_nft_staked != 0 {
        return Err(GodlError::NftAlreadyStaked.into());
    }

    // Verify the vault is the stake's canonical ATA, but tolerate it being
    // closed/uninitialized (zero residual).
    stake_tokens_info.has_address(&get_associated_token_address(
        stake_info.key,
        &MINT_ADDRESS,
    ))?;
    let residual = if stake_tokens_info.data_is_empty() {
        0
    } else {
        stake_tokens_info
            .as_associated_token_account(stake_info.key, mint_info.key)?
            .amount()
    };

    // Settle and take all pending rewards.
    let rewards = stake.claim(u64::MAX, &clock, treasury)?;

    // Pay out rewards and residual vault tokens before closing anything. When
    // there is nothing to pay, skip the recipient ATA entirely so a rewardless
    // close costs the signer no ATA rent.
    let total_out = rewards
        .checked_add(residual)
        .ok_or(GodlError::StakeOverflow)?;
    if total_out > 0 {
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
    }

    // Transfer rewards from the treasury vault.
    if rewards > 0 {
        transfer_signed(
            treasury_info,
            treasury_tokens_info,
            recipient_info,
            token_program,
            rewards,
            &[TREASURY],
        )?;
    }

    // Sweep residual vault tokens (donations/dust above the zero balance).
    if residual > 0 {
        transfer_signed(
            stake_info,
            stake_tokens_info,
            recipient_info,
            token_program,
            residual,
            &[STAKE_V2, &signer_info.key.to_bytes(), &id.to_le_bytes()],
        )?;
    }

    // Close the vault, returning its rent to the signer.
    if !stake_tokens_info.data_is_empty() {
        close_token_account_signed(
            stake_tokens_info,
            signer_info,
            stake_info,
            token_program,
            &[STAKE_V2, &signer_info.key.to_bytes(), &id.to_le_bytes()],
        )?;
    }

    // Log close.
    sol_log(
        &format!(
            "Closing stake {}: claimed {} GODL",
            stake_info.key,
            amount_to_ui_amount(rewards, TOKEN_DECIMALS)
        )
        .as_str(),
    );

    // Close the stake account, returning its rent to the signer.
    stake_info.close(signer_info)?;

    Ok(())
}
