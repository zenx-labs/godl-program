use godl_api::prelude::*;
use solana_program::{log::sol_log, native_token::lamports_to_sol};
use spl_token::amount_to_ui_amount;
use steel::*;

/// Swap the gold vault's WSOL (funded by `PreBuyGold`) into XAUt0 and distribute it to unrefined
/// GODL holders through the gold vault rewards factor.
///
/// Mirrors `Bury`: the instruction data after the args is forwarded verbatim to the configured
/// swap program, with the gold vault PDA as the signing taker. Only the bury authority may call.
/// The whole WSOL balance must be consumed by the swap, so the route has to be quoted for the
/// full balance including any dust that landed in the account.
pub fn process_buy_gold(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    // Parse data: fixed args followed by the raw swap instruction data.
    let args_len = std::mem::size_of::<BuyGold>();
    if data.len() < args_len {
        return Err(ProgramError::InvalidInstructionData);
    }
    let args = BuyGold::try_from_bytes(&data[..args_len])?;
    let min_xaut_out = u64::from_le_bytes(args.min_xaut_out);
    let swap_data = &data[args_len..];

    // Load accounts.
    let clock = Clock::get()?;
    // No distribution while the legacy instructions (which do not settle gold) can still run.
    if clock.unix_timestamp < LEGACY_INSTRUCTION_EXPIRY_TS {
        return Err(GodlError::GoldDistributionLocked.into());
    }
    let (godl_accounts, swap_accounts) = accounts.split_at(10);
    let [signer_info, board_info, config_info, treasury_info, gold_vault_info, gold_vault_sol_info, gold_vault_xaut_info, xaut_mint_info, token_program, godl_program] =
        godl_accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    board_info.as_account_mut::<Board>(&godl_api::ID)?;
    let config = config_info
        .as_account::<Config>(&godl_api::ID)?
        .assert(|c| c.bury_authority == *signer_info.key)?;
    let treasury = treasury_info.as_account::<Treasury>(&godl_api::ID)?;
    let gold_vault = gold_vault_info
        .is_writable()?
        .has_seeds(&[GOLD_VAULT], &godl_api::ID)?
        .as_account_mut::<GoldVault>(&godl_api::ID)?;
    gold_vault_sol_info
        .is_writable()?
        .as_associated_token_account(gold_vault_info.key, &SOL_MINT)?;
    gold_vault_xaut_info
        .is_writable()?
        .as_associated_token_account(gold_vault_info.key, &XAUT_MINT)?;
    xaut_mint_info.has_address(&XAUT_MINT)?.as_mint()?;
    token_program.is_program(&spl_token::ID)?;
    godl_program.is_program(&godl_api::ID)?;

    // Sync native token balance.
    sync_native(gold_vault_sol_info)?;

    // Record pre-swap balances.
    let pre_swap_sol_balance = gold_vault_sol_info
        .as_associated_token_account(gold_vault_info.key, &SOL_MINT)?
        .amount();
    let pre_swap_xaut_balance = gold_vault_xaut_info
        .as_associated_token_account(gold_vault_info.key, &XAUT_MINT)?
        .amount();
    let pre_swap_vault_lamports = gold_vault_info.lamports();
    assert!(pre_swap_sol_balance > 0);

    // Build swap accounts.
    let accounts: Vec<AccountMeta> = swap_accounts
        .iter()
        .map(|acc| AccountMeta {
            pubkey: *acc.key,
            is_signer: acc.key == gold_vault_info.key,
            is_writable: acc.is_writable,
        })
        .collect();
    let accounts_infos: Vec<AccountInfo> = swap_accounts.iter().cloned().collect();

    // Invoke swap program.
    invoke_signed(
        &Instruction {
            program_id: config.swap_program,
            accounts,
            data: swap_data.to_vec(),
        },
        &accounts_infos,
        &godl_api::ID,
        &[GOLD_VAULT],
    )?;

    // Verify the swap only touched the token accounts.
    assert_eq!(
        gold_vault_info.lamports(),
        pre_swap_vault_lamports,
        "Gold vault lamports changed during swap"
    );
    let post_swap_sol_balance = gold_vault_sol_info
        .as_associated_token_account(gold_vault_info.key, &SOL_MINT)?
        .amount();
    let post_swap_xaut_balance = gold_vault_xaut_info
        .as_associated_token_account(gold_vault_info.key, &XAUT_MINT)?
        .amount();
    assert_eq!(post_swap_sol_balance, 0, "WSOL was not fully swapped");
    assert!(
        post_swap_xaut_balance >= pre_swap_xaut_balance,
        "XAUt0 balance decreased during swap: {} -> {}",
        pre_swap_xaut_balance,
        post_swap_xaut_balance
    );
    let xaut_amount = post_swap_xaut_balance - pre_swap_xaut_balance;
    if xaut_amount < min_xaut_out || xaut_amount == 0 {
        return Err(GodlError::GoldSlippageExceeded.into());
    }
    sol_log(
        &format!(
            "Swapped {} SOL into {} XAUt0",
            lamports_to_sol(pre_swap_sol_balance),
            amount_to_ui_amount(xaut_amount, XAUT_DECIMALS),
        )
        .as_str(),
    );

    // Distribute to unrefined GODL holders.
    let distributed = gold_vault.distribute(xaut_amount, treasury.total_unclaimed)?;
    gold_vault.total_sol_swapped = gold_vault
        .total_sol_swapped
        .checked_add(pre_swap_sol_balance)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    sol_log(
        &format!(
            "Distributed {} XAUt0 across {} unrefined GODL",
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
            sol_amount: pre_swap_sol_balance,
            xaut_amount,
            xaut_distributed: distributed,
            total_unclaimed: treasury.total_unclaimed,
            ts: clock.unix_timestamp,
        }
        .to_bytes(),
    )?;

    Ok(())
}
