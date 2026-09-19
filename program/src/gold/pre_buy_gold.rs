use godl_api::prelude::*;
use solana_program::rent::Rent;
use steel::*;

/// Move SOL from the sol motherlode into the gold vault's WSOL account, ahead of `BuyGold`.
///
/// Same split as `PreBury` / `Bury`: the lamport move is a direct debit of a program-owned PDA,
/// and the runtime requires the caller's accounts to be balanced at every nested call, so it
/// cannot share an instruction with the `SyncNative` and swap CPIs. Only the bury authority may
/// call, and only once the legacy instructions have expired.
pub fn process_pre_buy_gold(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    // Parse args.
    let args = PreBuyGold::try_from_bytes(data)?;
    let amount = u64::from_le_bytes(args.amount);

    // Load accounts.
    let clock = Clock::get()?;
    if clock.unix_timestamp < LEGACY_INSTRUCTION_EXPIRY_TS {
        return Err(GodlError::GoldDistributionLocked.into());
    }
    let [signer_info, config_info, sol_motherlode_info, gold_vault_info, gold_vault_sol_info, system_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    config_info
        .as_account::<Config>(&godl_api::ID)?
        .assert(|c| c.bury_authority == *signer_info.key)?;
    let sol_motherlode = sol_motherlode_info
        .is_writable()?
        .has_seeds(&[SOL_MOTHERLODE], &godl_api::ID)?
        .as_account_mut::<SolMotherlode>(&godl_api::ID)?;
    gold_vault_info
        .has_seeds(&[GOLD_VAULT], &godl_api::ID)?
        .as_account::<GoldVault>(&godl_api::ID)?;
    gold_vault_sol_info
        .is_writable()?
        .as_associated_token_account(gold_vault_info.key, &SOL_MINT)?;
    system_program.is_program(&system_program::ID)?;

    // Validate amount against the motherlode ledger and its actual lamports.
    if amount == 0 {
        return Err(GodlError::AmountTooSmall.into());
    }
    if amount > sol_motherlode.amount {
        return Err(GodlError::InsufficientMotherlodeBalance.into());
    }
    let min_balance = Rent::get()?.minimum_balance(8 + std::mem::size_of::<SolMotherlode>());
    if sol_motherlode_info.lamports() < min_balance + amount {
        return Err(GodlError::InsufficientMotherlodeBalance.into());
    }

    // Send SOL to the WSOL account.
    sol_motherlode_info.send(amount, gold_vault_sol_info);
    sol_motherlode.amount -= amount;

    Ok(())
}
