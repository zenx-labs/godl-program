use godl_api::prelude::*;
use steel::*;

use crate::gold::load_or_create_miner_extended;

/// Creates the `MinerExtended` account for an existing miner. Permissionless: the signer pays
/// rent for any miner, which is how the one-shot bulk creation before the first `BuyGold` runs.
/// Idempotent: an existing account is left untouched.
pub fn process_create_miner_extended(accounts: &[AccountInfo<'_>], _data: &[u8]) -> ProgramResult {
    // Load accounts.
    let [signer_info, miner_info, miner_extended_info, gold_vault_info, system_program] = accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    let miner = miner_info.as_account::<Miner>(&godl_api::ID)?;
    miner_info.has_seeds(&[MINER, &miner.authority.to_bytes()], &godl_api::ID)?;
    let gold_vault = gold_vault_info
        .has_seeds(&[GOLD_VAULT], &godl_api::ID)?
        .as_account::<GoldVault>(&godl_api::ID)?;
    system_program.is_program(&system_program::ID)?;

    load_or_create_miner_extended(
        miner_extended_info,
        gold_vault,
        miner.authority,
        signer_info,
        system_program,
        true,
    )?;

    Ok(())
}
