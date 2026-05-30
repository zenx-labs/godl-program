use godl_api::prelude::*;
use solana_program::rent::Rent;
use steel::*;

/// Closes a round and its associated pool round, returning rent to the rent payer.
pub fn process_close(accounts: &[AccountInfo<'_>], _data: &[u8]) -> ProgramResult {
    let clock = Clock::get()?;
    let [signer_info, board_info, rent_payer_info, round_info, pool_round_info, treasury_info, system_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    rent_payer_info.is_writable()?;
    system_program.is_program(&system_program::ID)?;

    let board = board_info.as_account_mut::<Board>(&godl_api::ID)?;
    let treasury = treasury_info.as_account_mut::<Treasury>(&godl_api::ID)?;

    let round = round_info
        .as_account_mut::<Round>(&godl_api::ID)?
        .assert_mut(|r| r.id < board.round_id)?
        .assert_mut(|r| r.expires_at < clock.slot)?
        .assert_mut(|r| r.rent_payer == *rent_payer_info.key)?;
    let round_id = round.id;

    // Vault all unclaimed rewards from the round account.
    let round_size = 8 + std::mem::size_of::<Round>();
    let round_min_rent = Rent::get()?.minimum_balance(round_size);
    let round_unclaimed = round_info.lamports().saturating_sub(round_min_rent);
    if round_unclaimed > 0 {
        round_info.send(round_unclaimed, treasury_info);
        treasury.balance += round_unclaimed;
    }

    // Close main round account.
    round_info.close(rent_payer_info)?;

    // Close the pool round if it exists.
    pool_round_info
        .is_writable()?
        .has_seeds(&[POOL_ROUND, &round_id.to_le_bytes()], &godl_api::ID)?;
    if !pool_round_info.data_is_empty() {
        pool_round_info
            .as_account::<PoolRound>(&godl_api::ID)?
            .assert(|p| p.rent_payer == *rent_payer_info.key)?;
        let pool_size = 8 + std::mem::size_of::<PoolRound>();
        let pool_min_rent = Rent::get()?.minimum_balance(pool_size);
        let pool_unclaimed = pool_round_info.lamports().saturating_sub(pool_min_rent);
        if pool_unclaimed > 0 {
            pool_round_info.send(pool_unclaimed, treasury_info);
            treasury.balance += pool_unclaimed;
        }
        pool_round_info.close(rent_payer_info)?;
    }

    Ok(())
}
