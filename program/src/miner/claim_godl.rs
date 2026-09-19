use godl_api::prelude::*;
use solana_program::log::sol_log;
use spl_token::amount_to_ui_amount;
use steel::*;

use crate::gold::{load_or_create_miner_extended, sync_gold_rewards};

/// Claims a block reward.
pub fn process_claim_godl(accounts: &[AccountInfo<'_>], _data: &[u8]) -> ProgramResult {
    // Load accounts.
    let clock = Clock::get()?;
    let [signer_info, _config_info, miner_info, mint_info, recipient_info, treasury_info, treasury_tokens_info, system_program, token_program, associated_token_program, gold_vault_info, miner_extended_info, rest @ ..] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    let miner = miner_info
        .as_account_mut::<Miner>(&godl_api::ID)?
        .assert_mut(|m| m.authority == *signer_info.key)?;
    mint_info.has_address(&MINT_ADDRESS)?.as_mint()?;
    recipient_info.is_writable()?;
    let treasury = treasury_info.as_account_mut::<Treasury>(&godl_api::ID)?;
    treasury_tokens_info.as_associated_token_account(&treasury_info.key, &mint_info.key)?;
    system_program.is_program(&system_program::ID)?;
    token_program.is_program(&spl_token::ID)?;
    associated_token_program.is_program(&spl_associated_token_account::ID)?;
    let gold_vault = gold_vault_info
        .has_seeds(&[GOLD_VAULT], &godl_api::ID)?
        .as_account::<GoldVault>(&godl_api::ID)?;
    let miner_extended = load_or_create_miner_extended(
        miner_extended_info,
        gold_vault,
        miner.authority,
        signer_info,
        system_program,
        miner.rewards_godl > 0,
    )?;

    // Load recipient.
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
        recipient_info.as_associated_token_account(signer_info.key, mint_info.key)?;
    }

    // Settle gold rewards on the pre-claim unrefined balance, then claim (which zeroes it).
    sync_gold_rewards(miner_extended, gold_vault, miner);

    // Normalize amount.
    let amount = miner.claim_godl(&clock, treasury);
    let referrer_pda = miner.referrer_account();
    let referral_amount = if referrer_pda.is_some() {
        amount
            .checked_mul(REFERRAL_BPS)
            .ok_or(ProgramError::InvalidInstructionData)?
            / DENOMINATOR_BPS
    } else {
        0
    };
    let miner_amount = amount
        .checked_sub(referral_amount)
        .ok_or(ProgramError::InvalidInstructionData)?;

    sol_log(
        &format!(
            "Claiming {} GODL",
            amount_to_ui_amount(miner_amount, TOKEN_DECIMALS)
        )
        .as_str(),
    );
    if referral_amount > 0 {
        sol_log(
            &format!(
                "Referral amount: {} GODL",
                amount_to_ui_amount(referral_amount, TOKEN_DECIMALS)
            )
            .as_str(),
        );
    }

    if let Some(referrer_pubkey) = referrer_pda {
        let [referrer_info, referrer_tokens_info] = rest else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };
        let referrer = referrer_info.as_account_mut::<Referrer>(&godl_api::ID)?;
        referrer_info
            .is_writable()?
            .has_address(&referrer_pubkey)?
            .has_seeds(&[REFERRER, &referrer.authority.to_bytes()], &godl_api::ID)?;
        referrer_tokens_info
            .is_writable()?
            .as_associated_token_account(referrer_info.key, mint_info.key)?;

        referrer.rewards_godl = referrer
            .rewards_godl
            .checked_add(referral_amount)
            .ok_or(ProgramError::InvalidInstructionData)?;
        referrer.cumulative_rewards_godl = referrer
            .cumulative_rewards_godl
            .checked_add(referral_amount)
            .ok_or(ProgramError::InvalidInstructionData)?;

        transfer_signed(
            treasury_info,
            treasury_tokens_info,
            referrer_tokens_info,
            token_program,
            referral_amount,
            &[TREASURY],
        )?;
    } else if !rest.is_empty() {
        return Err(trace(
            "Unexpected referrer accounts",
            GodlError::InvalidReferrerAccount.into(),
        ));
    }

    if miner_amount > 0 {
        transfer_signed(
            treasury_info,
            treasury_tokens_info,
            recipient_info,
            token_program,
            miner_amount,
            &[TREASURY],
        )?;
    }

    Ok(())
}
