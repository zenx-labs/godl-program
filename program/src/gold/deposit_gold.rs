use godl_api::prelude::*;
use solana_program::log::sol_log;
use spl_token::amount_to_ui_amount;
use steel::*;

/// Manual-input twin of `BuyGold`: moves XAUt0 from the bury authority into the gold vault and
/// distributes it to unrefined GODL holders. Useful when the swap is done off-chain, and as the
/// distribution path exercised by the tests.
pub fn process_deposit_gold(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    // Parse data.
    let args = DepositGold::try_from_bytes(data)?;
    let amount = u64::from_le_bytes(args.amount);

    // Load accounts.
    let clock = Clock::get()?;
    // No distribution while the legacy instructions (which do not settle gold) can still run.
    if clock.unix_timestamp < LEGACY_INSTRUCTION_EXPIRY_TS {
        return Err(GodlError::GoldDistributionLocked.into());
    }
    let [signer_info, board_info, config_info, treasury_info, gold_vault_info, signer_xaut_info, gold_vault_xaut_info, xaut_mint_info, token_program, godl_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    board_info.as_account_mut::<Board>(&godl_api::ID)?;
    config_info
        .as_account::<Config>(&godl_api::ID)?
        .assert(|c| c.bury_authority == *signer_info.key)?;
    let treasury = treasury_info.as_account::<Treasury>(&godl_api::ID)?;
    let gold_vault = gold_vault_info
        .is_writable()?
        .has_seeds(&[GOLD_VAULT], &godl_api::ID)?
        .as_account_mut::<GoldVault>(&godl_api::ID)?;
    signer_xaut_info
        .is_writable()?
        .as_associated_token_account(signer_info.key, &XAUT_MINT)?;
    gold_vault_xaut_info
        .is_writable()?
        .as_associated_token_account(gold_vault_info.key, &XAUT_MINT)?;
    xaut_mint_info.has_address(&XAUT_MINT)?.as_mint()?;
    token_program.is_program(&spl_token::ID)?;
    godl_program.is_program(&godl_api::ID)?;

    if amount == 0 {
        return Err(GodlError::AmountTooSmall.into());
    }

    // Transfer XAUt0 from signer to the vault.
    transfer(
        signer_info,
        signer_xaut_info,
        gold_vault_xaut_info,
        token_program,
        amount,
    )?;

    // Distribute to unrefined GODL holders.
    let distributed = gold_vault.distribute(amount, treasury.total_unclaimed)?;
    sol_log(
        &format!(
            "Deposited {} XAUt0, distributed {} across {} unrefined GODL",
            amount_to_ui_amount(amount, XAUT_DECIMALS),
            amount_to_ui_amount(distributed, XAUT_DECIMALS),
            amount_to_ui_amount(treasury.total_unclaimed, TOKEN_DECIMALS),
        )
        .as_str(),
    );

    // Emit event.
    program_log(
        &[board_info.clone(), godl_program.clone()],
        BuyGoldEvent {
            disc: GodlEvent::BuyGold as u64,
            sol_amount: 0,
            xaut_amount: amount,
            xaut_distributed: distributed,
            total_unclaimed: treasury.total_unclaimed,
            ts: clock.unix_timestamp,
        }
        .to_bytes(),
    )?;

    Ok(())
}
