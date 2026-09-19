use godl_api::prelude::*;
use steel::*;

/// Initializes the GoldVault PDA and its WSOL / XAUt0 associated token accounts.
pub fn process_initialize_gold_vault(accounts: &[AccountInfo<'_>], _data: &[u8]) -> ProgramResult {
    // Load accounts.
    let [signer_info, gold_vault_info, gold_vault_sol_info, gold_vault_xaut_info, sol_mint_info, xaut_mint_info, system_program, token_program, associated_token_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    signer_info.has_address(&DEPLOYER_ADDRESS)?;
    gold_vault_info
        .is_empty()?
        .is_writable()?
        .has_seeds(&[GOLD_VAULT], &godl_api::ID)?;
    gold_vault_sol_info.is_writable()?;
    gold_vault_xaut_info.is_writable()?;
    sol_mint_info.has_address(&SOL_MINT)?.as_mint()?;
    xaut_mint_info.has_address(&XAUT_MINT)?.as_mint()?;
    system_program.is_program(&system_program::ID)?;
    token_program.is_program(&spl_token::ID)?;
    associated_token_program.is_program(&spl_associated_token_account::ID)?;

    // Create account.
    create_program_account::<GoldVault>(
        gold_vault_info,
        system_program,
        signer_info,
        &godl_api::ID,
        &[GOLD_VAULT],
    )?;

    // Initialize gold vault.
    let gold_vault = gold_vault_info.as_account_mut::<GoldVault>(&godl_api::ID)?;
    gold_vault.xaut_rewards_factor = Numeric::ZERO;
    gold_vault.pending_xaut = 0;
    gold_vault.total_xaut_distributed = 0;
    gold_vault.total_xaut_claimed = 0;
    gold_vault.total_sol_swapped = 0;
    gold_vault.buffer = [0; 32];

    // Create the vault's token accounts (swap input and swap output / claim source).
    for (token_account_info, mint_info) in [
        (gold_vault_sol_info, sol_mint_info),
        (gold_vault_xaut_info, xaut_mint_info),
    ] {
        if token_account_info.data_is_empty() {
            create_associated_token_account(
                signer_info,
                gold_vault_info,
                token_account_info,
                mint_info,
                system_program,
                token_program,
                associated_token_program,
            )?;
        } else {
            token_account_info.as_associated_token_account(gold_vault_info.key, mint_info.key)?;
        }
    }

    Ok(())
}
