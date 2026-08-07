use godl_api::prelude::*;
use solana_program::log::sol_log;
use spl_token::amount_to_ui_amount;
use steel::*;

/// Admin-only sweep of exploit-era phantom stake accounts: forfeits their
/// pending rewards and closes the account, sweeping the rent into the treasury.
///
/// Unlike every other stake handler this MUST NOT enforce the canonical PDA via
/// `has_seeds`: the accounts it removes are precisely the on-curve,
/// non-canonical accounts created by the exploit, which would fail that check.
/// Authorization comes from the `config.admin` gate instead, and the account is
/// identified by the address passed in. Canonical accounts are explicitly
/// rejected — their owners close them (and keep their rewards) via
/// `CloseStakeV2`.
///
/// The stake must already be empty (`ReconcileStakeV2` zeroes the unbacked
/// balance), so removing it changes `treasury.total_staked` by exactly zero.
/// Its vault ATA is left alone: the program can never sign for an on-curve
/// address, so the vault is uncloseable — and already emptied by the attacker.
pub fn process_close_phantom_stake_v2(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    ClosePhantomStakeV2::try_from_bytes(data)?;

    let [signer_info, config_info, stake_info, treasury_info] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;

    // Admin gate.
    let config = config_info.as_account::<Config>(&godl_api::ID)?;
    if config.admin != *signer_info.key {
        return Err(GodlError::NotAuthorized.into());
    }

    // Load the stake. Owner + discriminant are validated by as_account, but
    // intentionally NOT the PDA seeds (see the note above).
    let stake = stake_info
        .is_writable()?
        .as_account::<StakeV2>(&godl_api::ID)?;

    // Only non-canonical (phantom) accounts may be swept.
    if *stake_info.key == stake_v2_pda(stake.authority, stake.id).0 {
        return Err(GodlError::StakeNotPhantom.into());
    }

    // The stake must be empty; run ReconcileStakeV2 first otherwise. This is
    // what keeps total_staked untouched by the close.
    if stake.balance != 0 {
        return Err(GodlError::StakeNotEmpty.into());
    }

    let treasury = treasury_info
        .is_writable()?
        .has_seeds(&[TREASURY], &godl_api::ID)?
        .as_account_mut::<Treasury>(&godl_api::ID)?;

    // Log before closing (the account data is gone afterwards). A staked-NFT
    // flag does not block the sweep: the asset belongs to an on-curve address
    // the program cannot sign for, so it is stranded either way.
    sol_log(
        &format!(
            "Closing phantom stake {}: forfeiting {} GODL rewards{}",
            stake_info.key,
            amount_to_ui_amount(stake.rewards, TOKEN_DECIMALS),
            if stake.is_nft_staked != 0 {
                " (stranded NFT flag set)"
            } else {
                ""
            },
        )
        .as_str(),
    );

    // Close the account, sweeping all lamports into the treasury's buy-bury
    // balance (miner close precedent).
    let lamports = stake_info.lamports();
    stake_info.close(treasury_info)?;
    treasury.balance += lamports;

    Ok(())
}
