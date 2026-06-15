use godl_api::prelude::*;
use mpl_core::instructions::TransferV1CpiBuilder;
use steel::*;

/// Unstakes a Metaplex Core NFT from a StakeV2 account, returning it to the
/// authority and removing the 10% stake weight boost.
pub fn process_unstake_nft(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    let args = UnstakeNft::try_from_bytes(data)?;
    let id = u64::from_le_bytes(args.id);

    let [signer_info, asset_info, collection_info, stake_info, treasury_info, mpl_core_program, system_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    collection_info.has_address(&NFT_BOOST_COLLECTION)?;
    mpl_core_program.has_address(&MPL_CORE_PROGRAM)?;
    system_program.is_program(&system_program::ID)?;

    stake_info.has_seeds(
        &[STAKE_V2, &signer_info.key.to_bytes(), &id.to_le_bytes()],
        &godl_api::ID,
    )?;

    let stake = stake_info
        .as_account_mut::<StakeV2>(&godl_api::ID)?
        .assert_mut(|s| s.id == id)?
        .assert_mut(|s| s.authority == *signer_info.key)?;

    let treasury = treasury_info.as_account_mut::<Treasury>(&godl_api::ID)?;
    stake.unstake_nft(treasury)?;

    let (_, bump) = stake_v2_pda(*signer_info.key, id);
    let signer_seeds: &[&[u8]] = &[
        STAKE_V2,
        signer_info.key.as_ref(),
        &id.to_le_bytes(),
        &[bump],
    ];
    TransferV1CpiBuilder::new(mpl_core_program)
        .asset(asset_info)
        .collection(Some(collection_info))
        .payer(signer_info)
        .authority(Some(stake_info))
        .new_owner(signer_info)
        .system_program(Some(system_program))
        .invoke_signed(&[signer_seeds])?;

    Ok(())
}
