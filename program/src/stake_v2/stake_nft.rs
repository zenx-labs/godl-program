use godl_api::prelude::*;
use mpl_core::instructions::TransferV1CpiBuilder;
use steel::*;

/// Stakes a Metaplex Core NFT from the boost collection to a StakeV2 account,
/// granting a 10% boost to the stake weight.
pub fn process_stake_nft(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    let args = StakeNft::try_from_bytes(data)?;
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

    let stake = stake_info
        .as_account_mut::<StakeV2>(&godl_api::ID)?
        .assert_mut(|s| s.id == id)?
        .assert_mut(|s| s.authority == *signer_info.key)?;

    let treasury = treasury_info.as_account_mut::<Treasury>(&godl_api::ID)?;
    stake.stake_nft(treasury)?;

    TransferV1CpiBuilder::new(mpl_core_program)
        .asset(asset_info)
        .collection(Some(collection_info))
        .payer(signer_info)
        .authority(Some(signer_info))
        .new_owner(stake_info)
        .system_program(Some(system_program))
        .invoke()?;

    Ok(())
}
