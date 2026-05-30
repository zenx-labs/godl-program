use godl_api::prelude::*;
use solana_program::log::sol_log;
use solana_program::native_token::lamports_to_sol;
use spl_token::amount_to_ui_amount;
use steel::*;

/// Swap vaulted SOL to GODL, and burn the GODL.
pub fn process_bury(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    // Load accounts.
    let (godl_accounts, swap_accounts) = accounts.split_at(13);
    let [signer_info, board_info, config_info, mint_info, treasury_info, treasury_godl_info, treasury_sol_info, admin_info, admin_godl_info, warchest_info, warchest_godl_info, token_program, godl_program] =
        godl_accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    board_info.as_account_mut::<Board>(&godl_api::ID)?;
    let config = config_info
        .as_account::<Config>(&godl_api::ID)?
        .assert(|c| c.bury_authority == *signer_info.key)?;
    let godl_mint = mint_info.has_address(&MINT_ADDRESS)?.as_mint()?;
    let treasury = treasury_info.as_account_mut::<Treasury>(&godl_api::ID)?;
    let treasury_godl =
        treasury_godl_info.as_associated_token_account(treasury_info.key, &MINT_ADDRESS)?;
    treasury_sol_info.as_associated_token_account(treasury_info.key, &SOL_MINT)?;
    admin_info.has_address(&ADMIN_GODL_FEE)?;
    admin_godl_info.as_associated_token_account(admin_info.key, &MINT_ADDRESS)?;
    assert!(
        (*warchest_info.key == Pubkey::default()) == (*warchest_godl_info.key == Pubkey::default()),
        "Both warchest accounts must be provided or default",
    );
    let warchest_active = *warchest_info.key != Pubkey::default();
    if warchest_active {
        warchest_info.has_address(&CHEST_ADDRESS)?;
        warchest_godl_info
            .is_writable()?
            .as_associated_token_account(warchest_info.key, &MINT_ADDRESS)?;
    }
    token_program.is_program(&spl_token::ID)?;
    godl_program.is_program(&godl_api::ID)?;

    // Sync native token balance.
    sync_native(treasury_sol_info)?;

    // Record pre-swap balances.
    let treasury_sol =
        treasury_sol_info.as_associated_token_account(treasury_info.key, &SOL_MINT)?;
    let pre_swap_godl_balance = treasury_godl.amount();
    let pre_swap_sol_balance = treasury_sol.amount();
    assert!(pre_swap_sol_balance > 0);

    // Record pre-swap mint supply.
    let pre_swap_mint_supply = godl_mint.supply();

    // Record pre-swap treasury lamports.
    let pre_swap_treasury_lamports = treasury_info.lamports();

    // Build swap accounts.
    let accounts: Vec<AccountMeta> = swap_accounts
        .iter()
        .map(|acc| {
            let is_signer = acc.key == treasury_info.key;
            AccountMeta {
                pubkey: *acc.key,
                is_signer,
                is_writable: acc.is_writable,
            }
        })
        .collect();

    // Build swap accounts infos.
    let accounts_infos: Vec<AccountInfo> = swap_accounts
        .iter()
        .map(|acc| AccountInfo { ..acc.clone() })
        .collect();

    // Invoke swap program.
    invoke_signed(
        &Instruction {
            program_id: config.swap_program,
            accounts,
            data: data.to_vec(),
        },
        &accounts_infos,
        &godl_api::ID,
        &[TREASURY],
    )?;

    // Record post-swap treasury lamports.
    let post_swap_treasury_lamports = treasury_info.lamports();
    assert_eq!(
        post_swap_treasury_lamports, pre_swap_treasury_lamports,
        "Treasury lamports changed during swap: {} -> {}",
        pre_swap_treasury_lamports, post_swap_treasury_lamports
    );

    // Record post-swap mint supply.
    let post_swap_mint_supply = mint_info.as_mint()?.supply();
    assert_eq!(
        post_swap_mint_supply, pre_swap_mint_supply,
        "Mint supply changed during swap: {} -> {}",
        pre_swap_mint_supply, post_swap_mint_supply
    );

    // Record post-swap balances.
    let treasury_godl =
        treasury_godl_info.as_associated_token_account(treasury_info.key, &MINT_ADDRESS)?;
    let treasury_sol =
        treasury_sol_info.as_associated_token_account(treasury_info.key, &SOL_MINT)?;
    let post_swap_godl_balance = treasury_godl.amount();
    let post_swap_sol_balance = treasury_sol.amount();
    let total_godl = post_swap_godl_balance - pre_swap_godl_balance;
    assert_eq!(post_swap_sol_balance, 0);
    assert!(
        post_swap_godl_balance >= pre_swap_godl_balance,
        "GODL balance decreased during swap: {} -> {}",
        pre_swap_godl_balance,
        post_swap_godl_balance
    );
    sol_log(
        &format!(
            "Swapped {} SOL into {} GODL",
            lamports_to_sol(pre_swap_sol_balance),
            amount_to_ui_amount(total_godl, TOKEN_DECIMALS),
        )
        .as_str(),
    );

    // Share some GODL with stakers.
    let mut shared_amount = 0;
    if treasury.total_staked > 0 {
        shared_amount = (total_godl * STAKERS_BPS) / DENOMINATOR_BPS; // Share 4% with stakers
        treasury.stake_rewards_factor +=
            Numeric::from_fraction(shared_amount, treasury.total_staked);
    }

    // Share some GODL with admin.
    let admin_amount = (total_godl * ADMIN_BPS) / DENOMINATOR_BPS; // Share 3% with admin
    if admin_amount > 0 {
        transfer_signed(
            treasury_info,
            treasury_godl_info,
            admin_godl_info,
            token_program,
            admin_amount,
            &[TREASURY],
        )?;
    }

    sol_log(&format!(
        "Shared {} GODL",
        amount_to_ui_amount(shared_amount, TOKEN_DECIMALS)
    ));

    // Burn or send GODL.
    let burn_amount = total_godl - shared_amount - admin_amount;
    let mut godl_buried = burn_amount;
    if burn_amount > 0 {
        if warchest_active {
            transfer_signed(
                treasury_info,
                treasury_godl_info,
                warchest_godl_info,
                token_program,
                burn_amount,
                &[TREASURY],
            )?;
            godl_buried = 0;
            sol_log(
                &format!(
                    "Sent {} GODL to warchest",
                    amount_to_ui_amount(burn_amount, TOKEN_DECIMALS)
                )
                .as_str(),
            );
        } else {
            burn_signed(
                treasury_godl_info,
                mint_info,
                treasury_info,
                token_program,
                burn_amount,
                &[TREASURY],
            )?;

            sol_log(
                &format!(
                    "Buried {} GODL",
                    amount_to_ui_amount(burn_amount, TOKEN_DECIMALS)
                )
                .as_str(),
            );
        }
    }

    // Emit event.
    let mint = mint_info.as_mint()?;
    program_log(
        &[board_info.clone(), godl_program.clone()],
        BuryEvent {
            disc: 1,
            godl_buried,
            godl_shared: shared_amount,
            sol_amount: pre_swap_sol_balance,
            new_circulating_supply: mint.supply(),
            ts: Clock::get()?.unix_timestamp,
        }
        .to_bytes(),
    )?;

    Ok(())
}
