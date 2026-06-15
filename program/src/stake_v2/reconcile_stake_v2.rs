use godl_api::prelude::*;
use solana_program::log::sol_log;
use spl_associated_token_account::get_associated_token_address;
use spl_token::amount_to_ui_amount;
use steel::*;

/// Admin-only fix for the PDA-substitution exploit. Reconciles a stake's
/// recorded balance down to the GODL actually held in its vault and removes the
/// freed (phantom) weight from `treasury.total_staked`.
///
/// Unlike every other stake handler this MUST NOT enforce the canonical PDA via
/// `has_seeds`: the accounts it needs to repair are precisely the on-curve,
/// non-canonical accounts created by the exploit, which would fail that check.
/// Authorization comes from the `config.admin` gate instead, and the account is
/// identified by the address passed in.
pub fn process_reconcile_stake_v2(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    ReconcileStakeV2::try_from_bytes(data)?;

    let [signer_info, config_info, stake_info, stake_tokens_info, mint_info, treasury_info] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;

    // Admin gate.
    let config = config_info.as_account::<Config>(&godl_api::ID)?;
    if config.admin != *signer_info.key {
        return Err(GodlError::NotAuthorized.into());
    }

    mint_info.has_address(&MINT_ADDRESS)?;

    // Load the stake. Owner + discriminant are validated by as_account_mut, but
    // intentionally NOT the PDA seeds (see the note above).
    let stake = stake_info.is_writable()?.as_account_mut::<StakeV2>(&godl_api::ID)?;

    let treasury = treasury_info
        .is_writable()?
        .has_seeds(&[TREASURY], &godl_api::ID)?
        .as_account_mut::<Treasury>(&godl_api::ID)?;

    // The real backing is whatever GODL sits in the stake's own vault. Verify
    // the passed account is the stake's canonical ATA, but tolerate it being
    // closed/uninitialized: fully-drained phantom stakes have their vault closed
    // by the attacker, so a missing vault means zero backing (not an error).
    let expected_vault = get_associated_token_address(stake_info.key, &MINT_ADDRESS);
    stake_tokens_info.has_address(&expected_vault)?;
    let vault_balance = if stake_tokens_info.data_is_empty() {
        0
    } else {
        stake_tokens_info
            .as_associated_token_account(stake_info.key, mint_info.key)?
            .amount()
    };

    // Remove only the unbacked (phantom) portion by reusing the staking
    // withdraw accounting: settle rewards, drop the balance, and subtract the
    // weighted delta from total_staked. We deliberately move no tokens — a
    // phantom stake's vault is already empty. saturating_sub leaves a
    // fully-backed stake untouched (withdraws 0).
    let clock = Clock::get()?;
    let phantom_amount = stake.balance.saturating_sub(vault_balance);
    let before = stake.balance;
    stake.withdraw(phantom_amount, &clock, treasury)?;

    sol_log(
        &format!(
            "Reconciled stake {}: balance {} -> {} GODL",
            stake_info.key,
            amount_to_ui_amount(before, TOKEN_DECIMALS),
            amount_to_ui_amount(stake.balance, TOKEN_DECIMALS),
        )
        .as_str(),
    );

    Ok(())
}
