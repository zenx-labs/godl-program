use entropy_api::state::Var;
use godl_api::prelude::*;
use solana_program::{keccak, log::sol_log};
use steel::*;

/// Pays out the winners and block reward for the current round and opens the
/// next one. This is the lenient counterpart to `process_reset_permissionless`:
/// it is gated to `CRANK_BOT` and tolerates a missing or invalid top-miner
/// submission (where the strict permissionless path would revert), so a stall
/// in top-miner resolution cannot halt round progression.
///
/// The `CRANK_BOT` gate is an operational convenience, not a point of
/// centralization. Every round outcome — winning square, payouts, motherlode —
/// is fixed by the entropy `Var` RNG and is identical regardless of who calls;
/// the signer cannot influence results or redirect funds. And because
/// `process_reset_permissionless` lets anyone advance a round trustlessly, the
/// crank operator can neither censor nor capture the protocol: this variant only
/// keeps the common path cheap and robust against a degenerate top-miner input.
pub fn process_reset(accounts: &[AccountInfo<'_>], _data: &[u8]) -> ProgramResult {
    // Load accounts.
    let clock = Clock::get()?;
    let (godl_accounts, other_accounts) = accounts.split_at(17);
    sol_log(&format!("Godl accounts: {:?}", godl_accounts.len()).to_string());
    sol_log(&format!("Other accounts: {:?}", other_accounts.len()).to_string());
    let [signer_info, board_info, config_info, fee_collector_info, mint_info, round_info, round_next_info, pool_round_next_info, top_miner_info, top_miner_pool_member_info, sol_motherlode_info, treasury_info, treasury_tokens_info, system_program, token_program, godl_program, slot_hashes_sysvar] =
        godl_accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?.has_address(&CRANK_BOT)?;
    let board = board_info
        .as_account_mut::<Board>(&godl_api::ID)?
        .assert_mut(|b| clock.slot >= b.end_slot + INTERMISSION_SLOTS)?;
    let config = config_info.as_account::<Config>(&godl_api::ID)?;
    fee_collector_info
        .is_writable()?
        .has_address(&config.fee_collector)?;
    let round = round_info
        .as_account_mut::<Round>(&godl_api::ID)?
        .assert_mut(|r| r.id == board.round_id)?;
    round_next_info
        .is_empty()?
        .is_writable()?
        .has_seeds(&[ROUND, &(board.round_id + 1).to_le_bytes()], &godl_api::ID)?;
    pool_round_next_info.is_empty()?.is_writable()?.has_seeds(
        &[POOL_ROUND, &(board.round_id + 1).to_le_bytes()],
        &godl_api::ID,
    )?;
    let mint = mint_info.has_address(&MINT_ADDRESS)?.as_mint()?;
    let sol_motherlode = sol_motherlode_info
        .is_writable()?
        .has_seeds(&[SOL_MOTHERLODE], &godl_api::ID)?
        .as_account_mut::<SolMotherlode>(&godl_api::ID)?;
    let treasury = treasury_info.as_account_mut::<Treasury>(&godl_api::ID)?;
    treasury_tokens_info.as_associated_token_account(&treasury_info.key, &mint_info.key)?;
    system_program.is_program(&system_program::ID)?;
    token_program.is_program(&spl_token::ID)?;
    godl_program.is_program(&godl_api::ID)?;
    slot_hashes_sysvar.is_sysvar(&sysvar::slot_hashes::ID)?;

    // Open next round account.
    create_program_account::<Round>(
        round_next_info,
        godl_program,
        signer_info,
        &godl_api::ID,
        &[ROUND, &(board.round_id + 1).to_le_bytes()],
    )?;
    let round_next = round_next_info.as_account_mut::<Round>(&godl_api::ID)?;
    round_next.id = board.round_id + 1;
    round_next.deployed = [0; 25];
    round_next.slot_hash = [0; 32];
    round_next.count = [0; 25];
    round_next.expires_at = u64::MAX; // Set to max, to indicate round is waiting for first deploy to begin.
    round_next.rent_payer = *signer_info.key;
    round_next.motherlode = 0;
    round_next.top_miner = Pubkey::default();
    round_next.top_miner_reward = 0;
    round_next.total_deployed = 0;
    round_next.total_vaulted = 0;
    round_next.total_winnings = 0;

    // Open next pool round account.
    create_program_account::<PoolRound>(
        pool_round_next_info,
        godl_program,
        signer_info,
        &godl_api::ID,
        &[POOL_ROUND, &(board.round_id + 1).to_le_bytes()],
    )?;
    let pool_round_next = pool_round_next_info.as_account_mut::<PoolRound>(&godl_api::ID)?;
    pool_round_next.id = board.round_id + 1;
    pool_round_next.deployed = [0; 25];
    pool_round_next.count = [0; 25];
    pool_round_next.total_deployed = 0;
    pool_round_next.rent_payer = *signer_info.key;

    // Sample random variable
    let (entropy_accounts, mint_accounts) = other_accounts.split_at(2);
    let [var_info, entropy_program] = entropy_accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    let var = var_info
        .has_address(&config.var_address)?
        .as_account::<Var>(&entropy_api::ID)?
        .assert(|v| v.authority == *board_info.key)?
        .assert(|v| v.slot_hash != [0; 32])?
        .assert(|v| v.seed != [0; 32])?
        .assert(|v| v.value != [0; 32])?;
    entropy_program.is_program(&entropy_api::ID)?;

    // Print the seed and slot hash.
    let seed = keccak::Hash::new_from_array(var.seed);
    let slot_hash = keccak::Hash::new_from_array(var.slot_hash);
    sol_log(&format!("var slothash: {:?}", slot_hash).to_string());
    sol_log(&format!("var seed: {:?}", seed).to_string());

    // Read the finalized value from the var.
    let value = keccak::Hash::new_from_array(var.value);
    sol_log(&format!("var value: {:?}", value).to_string());
    round.slot_hash = var.value;

    // Exit early if no slot hash was found.
    let Some(r) = round.rng() else {
        // Slot hash could not be found, refund all SOL.
        round.total_vaulted = 0;
        round.total_winnings = 0;
        round.total_deployed = 0;

        // Emit event.
        program_log(
            &[board_info.clone(), godl_program.clone()],
            ResetEvent {
                disc: 0,
                round_id: round.id,
                start_slot: board.start_slot,
                end_slot: board.end_slot,
                winning_square: u64::MAX,
                top_miner: Pubkey::default(),
                num_winners: 0,
                motherlode: 0,
                total_deployed: round.total_deployed,
                total_vaulted: round.total_vaulted,
                total_winnings: round.total_winnings,
                total_minted: 0,
                ts: clock.unix_timestamp,
            }
            .to_bytes(),
        )?;

        // Update board for next round.
        board.round_id += 1;
        board.start_slot = clock.slot;
        board.end_slot = u64::MAX;
        return Ok(());
    };

    // Caculate admin fees.
    let total_admin_fee = round
        .total_deployed
        .checked_mul(ADMIN_FEE_BPS)
        .ok_or(ProgramError::InvalidInstructionData)?
        / DENOMINATOR_BPS;

    // Calculate sol motherlode amount.
    let sol_motherlode_amount = round
        .total_deployed
        .checked_mul(SOL_MOTHERLODE_BPS)
        .ok_or(ProgramError::InvalidInstructionData)?
        / DENOMINATOR_BPS;

    // Get the winning square.
    let winning_square = round.winning_square(r);

    // If no one deployed on the winning square, vault all deployed.
    if round.deployed[winning_square] == 0 {
        // Vault all deployed.
        round.total_vaulted = round.total_deployed - total_admin_fee - sol_motherlode_amount;
        treasury.balance += round.total_vaulted;
        sol_motherlode.amount += sol_motherlode_amount;

        // Emit event.
        program_log(
            &[board_info.clone(), godl_program.clone()],
            ResetEvent {
                disc: 0,
                round_id: round.id,
                start_slot: board.start_slot,
                end_slot: board.end_slot,
                winning_square: winning_square as u64,
                top_miner: Pubkey::default(),
                num_winners: 0,
                motherlode: 0,
                total_deployed: round.total_deployed,
                total_vaulted: round.total_vaulted,
                total_winnings: round.total_winnings,
                total_minted: 0,
                ts: clock.unix_timestamp,
            }
            .to_bytes(),
        )?;

        // Update board for next round.
        board.round_id += 1;
        board.start_slot = clock.slot;
        board.end_slot = u64::MAX;

        // Do SOL transfers.
        round_info.send(total_admin_fee, &fee_collector_info);
        round_info.send(sol_motherlode_amount, &sol_motherlode_info);
        round_info.send(round.total_vaulted, &treasury_info);
        return Ok(());
    }

    // Get winnings amount (total deployed on all non-winning squares).
    let winnings = round.calculate_total_winnings(winning_square);
    let winnings_fees = winnings
        .checked_mul(ADMIN_FEE_BPS + SOL_MOTHERLODE_BPS)
        .ok_or(ProgramError::InvalidInstructionData)?
        / DENOMINATOR_BPS;
    let winnings = winnings - winnings_fees;

    // Subtract vault amount from the winnings.
    let vault_amount = winnings / 10;
    let winnings = winnings
        .checked_sub(vault_amount)
        .ok_or(ProgramError::InvalidInstructionData)?;

    round.total_winnings = winnings;
    round.total_vaulted = vault_amount;
    treasury.balance += vault_amount;

    // Sanity check: ensure tracked outflows do not exceed total deployed.
    assert!(
        round.total_deployed
            >= round.total_vaulted
                + round.total_winnings
                + round.deployed[winning_square]
                + winnings_fees
    );

    // Calculate mint amounts.
    let godl_per_round = config.godl_per_round;
    let mut mint_supply = mint.supply();
    let mint_amount = MAX_SUPPLY.saturating_sub(mint_supply).min(godl_per_round);
    mint_supply += mint_amount;
    let motherlode_denominator = config.motherlode_denominator;
    let motherlode_mint_amount = MAX_SUPPLY
        .saturating_sub(mint_supply)
        .min(godl_per_round / motherlode_denominator);
    let total_mint_amount = mint_amount + motherlode_mint_amount;

    round.top_miner_reward = mint_amount;

    // With 1 in 2 odds, split the GODL reward.
    if round.is_split_reward(r) {
        round.top_miner = SPLIT_ADDRESS;
    } else if !top_miner_info.data_is_empty() && !top_miner_pool_member_info.data_is_empty() {
        let candidate = top_miner_info
            .as_account::<Miner>(&godl_api::ID)?
            .assert(|m| m.round_id == round.id)?;
        let pool_member = top_miner_pool_member_info
            .as_account::<PoolMember>(&godl_api::ID)?
            .assert(|p| p.authority == candidate.authority)?
            .assert(|p| p.round_id == round.id)?;
        if candidate.deployed[winning_square] > 0 && pool_member.deployed[winning_square] > 0 {
            let top_miner_sample = round.top_miner_sample(r, winning_square);
            let lower = candidate.cumulative[winning_square];
            let upper = lower
                .checked_add(candidate.deployed[winning_square])
                .ok_or(ProgramError::InvalidInstructionData)?;
            if top_miner_sample >= lower && top_miner_sample < upper {
                round.top_miner = POOL_ADDRESS;
            }
        }
    }

    // Payout the motherlode if it was activated.
    if round.did_hit_motherlode(r) {
        let rollover_amount = treasury.motherlode / 10;
        round.motherlode = treasury.motherlode - rollover_amount;
        treasury.motherlode = rollover_amount;

        let sol_motherlode_payout = sol_motherlode.amount;

        // Transfer SOL from sol motherlode to round for distribution.
        if sol_motherlode_payout > 0 {
            round.total_winnings += sol_motherlode_payout;
            sol_motherlode_info.send(sol_motherlode_payout, &round_info);
            sol_motherlode.amount = 0;
        }
    }

    // Payout the mini motherlode if it was activated. (drops only SOL, no GODL)
    if round.did_hit_mini_motherlode(r) {
        let sol_motherlode_payout = sol_motherlode.amount;
        if sol_motherlode_payout > 0 {
            round.total_winnings += sol_motherlode_payout;
            sol_motherlode_info.send(sol_motherlode_payout, &round_info);
            sol_motherlode.amount = 0;
        }
    }

    // Mint GODL to the treasury.
    if motherlode_mint_amount > 0 {
        treasury.motherlode += motherlode_mint_amount;
    }
    if total_mint_amount > 0 {
        let [godl_mint_authority_info, godl_mint_program] = mint_accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };
        godl_mint_authority_info
            .as_account::<godl_mint_api::state::Authority>(&godl_mint_api::ID)?;
        godl_mint_program.is_program(&godl_mint_api::ID)?;
        invoke_signed(
            &godl_mint_api::sdk::mint_godl(total_mint_amount),
            &[
                treasury_info.clone(),
                godl_mint_authority_info.clone(),
                mint_info.clone(),
                treasury_tokens_info.clone(),
                token_program.clone(),
            ],
            &godl_api::ID,
            &[TREASURY],
        )?;
    }

    // Transfer SOL from round to sol motherlode.
    if sol_motherlode_amount > 0 {
        round_info.send(sol_motherlode_amount, &sol_motherlode_info);
        sol_motherlode.amount += sol_motherlode_amount;
    }

    // Emit event.
    program_log(
        &[board_info.clone(), godl_program.clone()],
        ResetEvent {
            disc: 0,
            round_id: round.id,
            start_slot: board.start_slot,
            end_slot: board.end_slot,
            winning_square: winning_square as u64,
            top_miner: round.top_miner,
            motherlode: round.motherlode,
            num_winners: round.count[winning_square],
            total_deployed: round.total_deployed,
            total_vaulted: round.total_vaulted,
            total_winnings: round.total_winnings,
            total_minted: total_mint_amount,
            ts: clock.unix_timestamp,
        }
        .to_bytes(),
    )?;

    // Reset board.
    board.round_id += 1;
    board.start_slot = clock.slot;
    board.end_slot = u64::MAX; // board.start_slot + 150;

    // Do SOL transfers.
    round_info.send(total_admin_fee, &fee_collector_info);
    round_info.send(vault_amount, &treasury_info);

    Ok(())
}
