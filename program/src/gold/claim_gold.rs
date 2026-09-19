use godl_api::prelude::*;
use solana_program::log::sol_log;
use spl_token::amount_to_ui_amount;
use steel::*;

use crate::gold::load_or_create_miner_extended;

/// Claims the XAUt0 a miner has earned on their unrefined GODL.
pub fn process_claim_gold(accounts: &[AccountInfo<'_>], _data: &[u8]) -> ProgramResult {
    // Load accounts.
    let [signer_info, miner_info, miner_extended_info, gold_vault_info, gold_vault_xaut_info, recipient_info, xaut_mint_info, system_program, token_program, associated_token_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    let miner = miner_info
        .as_account::<Miner>(&godl_api::ID)?
        .assert(|m| m.authority == *signer_info.key)?;
    miner_info.has_seeds(&[MINER, &miner.authority.to_bytes()], &godl_api::ID)?;
    let gold_vault = gold_vault_info
        .is_writable()?
        .has_seeds(&[GOLD_VAULT], &godl_api::ID)?
        .as_account_mut::<GoldVault>(&godl_api::ID)?;
    gold_vault_xaut_info
        .is_writable()?
        .as_associated_token_account(gold_vault_info.key, &XAUT_MINT)?;
    recipient_info.is_writable()?;
    xaut_mint_info.has_address(&XAUT_MINT)?.as_mint()?;
    system_program.is_program(&system_program::ID)?;
    token_program.is_program(&spl_token::ID)?;
    associated_token_program.is_program(&spl_associated_token_account::ID)?;

    // Load (or lazily create) the extended account and settle accrued gold.
    let extended = load_or_create_miner_extended(
        miner_extended_info,
        gold_vault,
        miner.authority,
        signer_info,
        system_program,
        true,
    )?
    .ok_or(ProgramError::InvalidAccountData)?;
    extended.update_rewards(gold_vault, miner.rewards_godl);
    let amount = extended.claim();
    gold_vault.total_xaut_claimed = gold_vault
        .total_xaut_claimed
        .checked_add(amount)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    sol_log(
        &format!(
            "Claiming {} XAUt0",
            amount_to_ui_amount(amount, XAUT_DECIMALS)
        )
        .as_str(),
    );
    if amount == 0 {
        return Ok(());
    }

    // Load recipient.
    if recipient_info.data_is_empty() {
        create_associated_token_account(
            signer_info,
            signer_info,
            recipient_info,
            xaut_mint_info,
            system_program,
            token_program,
            associated_token_program,
        )?;
    } else {
        recipient_info.as_associated_token_account(signer_info.key, xaut_mint_info.key)?;
    }

    // Transfer XAUt0 from the vault.
    transfer_signed(
        gold_vault_info,
        gold_vault_xaut_info,
        recipient_info,
        token_program,
        amount,
        &[GOLD_VAULT],
    )?;

    Ok(())
}
