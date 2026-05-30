use godl_api::prelude::*;
use solana_program::log::sol_log;
use spl_token::amount_to_ui_amount;
use steel::*;

/// Share a fixed amount of GODL with stakers and burn the remainder.
///
/// Only the configured bury authority may invoke this instruction.
pub fn process_bury_tokens(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    // Parse data.
    let args = BuryTokens::try_from_bytes(data)?;
    let amount = u64::from_le_bytes(args.amount);

    // Load accounts.
    let [signer_info, board_info, config_info, mint_info, treasury_info, treasury_godl_info, admin_info, admin_godl_info, warchest_info, warchest_godl_info, token_program, godl_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    signer_info.is_signer()?;
    board_info.as_account_mut::<Board>(&godl_api::ID)?;
    config_info
        .as_account::<Config>(&godl_api::ID)?
        .assert(|c| c.bury_authority == *signer_info.key)?;
    mint_info.has_address(&MINT_ADDRESS)?.as_mint()?;
    let treasury = treasury_info.as_account_mut::<Treasury>(&godl_api::ID)?;
    let treasury_godl =
        treasury_godl_info.as_associated_token_account(treasury_info.key, &MINT_ADDRESS)?;
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

    // Basic safety checks.
    assert!(amount > 0, "Amount must be greater than zero");
    assert!(
        treasury_godl.amount() >= amount,
        "Treasury GODL balance too low for bury_tokens",
    );

    // Share some GODL with stakers.
    let mut shared_amount = 0;
    if treasury.total_staked > 0 {
        shared_amount = (amount * STAKERS_BPS) / DENOMINATOR_BPS; // Share 4% with stakers
        if shared_amount > 0 {
            treasury.stake_rewards_factor +=
                Numeric::from_fraction(shared_amount, treasury.total_staked);
        }
    }

    // Share some GODL with admin.
    let admin_amount = (amount * ADMIN_BPS) / DENOMINATOR_BPS; // Share 3% with admin
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

    sol_log(
        &format!(
            "Shared {} GODL (manual bury)",
            amount_to_ui_amount(shared_amount, TOKEN_DECIMALS)
        )
        .as_str(),
    );

    // Burn remaining GODL from the treasury's token account.
    let burn_amount = amount - shared_amount - admin_amount;
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
                    "Sent {} GODL to warchest (manual)",
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
                    "Buried {} GODL (manual)",
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
            sol_amount: 0,
            new_circulating_supply: mint.supply(),
            ts: Clock::get()?.unix_timestamp,
        }
        .to_bytes(),
    )?;

    Ok(())
}
