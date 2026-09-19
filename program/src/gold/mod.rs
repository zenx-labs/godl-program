mod buy_gold;
mod claim_gold;
mod create_miner_extended;
mod deposit_gold;
mod initialize_gold_vault;

pub use buy_gold::*;
pub use claim_gold::*;
pub use create_miner_extended::*;
pub use deposit_gold::*;
pub use initialize_gold_vault::*;

use godl_api::prelude::*;
use steel::*;

/// Loads a miner's `MinerExtended` account, verifying its seeds against `authority`.
///
/// When the account does not exist yet it is created with rent paid by `payer` if `create` is
/// set, otherwise `None` is returned. Callers pass `create = false` only when the miner holds no
/// unrefined GODL and none is about to be credited, so nothing could accrue on it.
pub fn load_or_create_miner_extended<'a, 'info>(
    miner_extended_info: &'a AccountInfo<'info>,
    gold_vault: &GoldVault,
    authority: Pubkey,
    payer: &'a AccountInfo<'info>,
    system_program: &'a AccountInfo<'info>,
    create: bool,
) -> Result<Option<&'a mut MinerExtended>, ProgramError> {
    miner_extended_info
        .is_writable()?
        .has_seeds(&[MINER_EXTENDED, &authority.to_bytes()], &godl_api::ID)?;
    if miner_extended_info.data_is_empty() {
        if !create {
            return Ok(None);
        }
        create_program_account::<MinerExtended>(
            miner_extended_info,
            system_program,
            payer,
            &godl_api::ID,
            &[MINER_EXTENDED, &authority.to_bytes()],
        )?;
        let extended = miner_extended_info.as_account_mut::<MinerExtended>(&godl_api::ID)?;
        extended.init(authority, gold_vault);
        return Ok(Some(extended));
    }
    let extended = miner_extended_info
        .as_account_mut::<MinerExtended>(&godl_api::ID)?
        .assert_mut(|e| e.authority == authority)?;
    Ok(Some(extended))
}

/// Settles the gold rewards a miner has accrued on its current unrefined GODL. Call it right
/// after `Miner::update_rewards` and before `rewards_godl` changes.
///
/// `extended` may only be `None` when the miner holds no unrefined GODL: nothing could have
/// accrued on a zero balance, so there is nothing to settle.
pub fn sync_gold_rewards(
    extended: Option<&mut MinerExtended>,
    gold_vault: &GoldVault,
    miner: &Miner,
) {
    match extended {
        Some(extended) => {
            assert!(
                extended.authority == miner.authority,
                "Miner extended account authority mismatch"
            );
            extended.update_rewards(gold_vault, miner.rewards_godl);
        }
        None => assert!(
            miner.rewards_godl == 0,
            "Miner has unrefined GODL but no extended account"
        ),
    }
}
