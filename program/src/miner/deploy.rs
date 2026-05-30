use entropy_api::state::Var;
use godl_api::prelude::*;
use solana_program::{
    keccak::hashv,
    log::sol_log,
    native_token::{lamports_to_sol, LAMPORTS_PER_SOL},
};
use steel::*;

/// Deploys capital to prospect on a square with optional pooling.
pub fn process_deploy(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    // Parse data.
    let args = Deploy::try_from_bytes(data)?;
    let mut amount = u64::from_le_bytes(args.amount);
    let mask = u32::from_le_bytes(args.squares);
    let mut is_pooled = args.is_pooled != 0;

    // Load accounts.
    let clock = Clock::get()?;
    let (godl_accounts, entropy_accounts) = accounts.split_at(10);
    sol_log(&format!("Godl accounts: {:?}", godl_accounts.len()).to_string());
    sol_log(&format!("Entropy accounts: {:?}", entropy_accounts.len()).to_string());
    let [signer_info, authority_info, automation_v2_info, board_info, miner_info, round_info, pool_round_info, pool_member_info, config_info, system_program] =
        godl_accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    authority_info.is_writable()?;
    automation_v2_info.is_writable()?.has_seeds(
        &[AUTOMATION_V2, &authority_info.key.to_bytes()],
        &godl_api::ID,
    )?;
    let config = config_info.as_account::<Config>(&godl_api::ID)?;
    let board = board_info
        .as_account_mut::<Board>(&godl_api::ID)?
        .assert_mut(|b| clock.slot >= b.start_slot && clock.slot < b.end_slot)?;
    let round = round_info
        .as_account_mut::<Round>(&godl_api::ID)?
        .assert_mut(|r| r.id == board.round_id)?;
    let pool_round = pool_round_info
        .as_account_mut::<PoolRound>(&godl_api::ID)?
        .assert_mut(|p| p.id == round.id)?;
    miner_info
        .is_writable()?
        .has_seeds(&[MINER, &authority_info.key.to_bytes()], &godl_api::ID)?;
    pool_member_info.is_writable()?.has_seeds(
        &[POOL_MEMBER, &authority_info.key.to_bytes()],
        &godl_api::ID,
    )?;
    system_program.is_program(&system_program::ID)?;

    // Wait until first deploy to start round.
    if board.end_slot == u64::MAX {
        board.start_slot = clock.slot;
        board.end_slot = board.start_slot + 150;
        round.expires_at = board.end_slot + ONE_DAY_SLOTS;

        // Bump var to the next value.
        let [var_info, entropy_program] = entropy_accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };
        var_info
            .has_address(&config.var_address)?
            .as_account::<Var>(&entropy_api::ID)?
            .assert(|v| v.authority == *board_info.key)?;
        entropy_program.is_program(&entropy_api::ID)?;

        // Bump var to the next value.
        invoke_signed(
            &entropy_api::sdk::next(*board_info.key, *var_info.key, board.end_slot),
            &[board_info.clone(), var_info.clone()],
            &godl_api::ID,
            &[BOARD],
        )?;
    }

    // Check if signer is the automation executor.
    let automation_v2 = if !automation_v2_info.data_is_empty() {
        let automation_v2 = automation_v2_info
            .as_account_mut::<AutomationV2>(&godl_api::ID)?
            .assert_mut(|a| a.executor == *signer_info.key)?
            .assert_mut(|a| a.authority == *authority_info.key)?;
        is_pooled = automation_v2.get_is_pooled();
        Some(automation_v2)
    } else {
        // Without automation, the signer must be the authority.
        if signer_info.key != authority_info.key {
            return Err(GodlError::NotAuthorized.into());
        }
        None
    };

    // Update amount and mask for automation.
    let mut squares = [false; 25];
    if let Some(automation_v2) = &automation_v2 {
        // Set amount
        amount = automation_v2.amount;

        // Set squares
        match automation_v2.get_strategy() {
            AutomationV2Strategy::Preferred => {
                // Preferred automation strategy. Use the miner authority's provided mask.
                for i in 0..25 {
                    squares[i] = (automation_v2.mask & (1 << i)) != 0;
                }
            }
            AutomationV2Strategy::Random => {
                // Random automation strategy. Generate a random mask based on number of squares user wants to deploy to.
                let num_squares = ((automation_v2.mask & 0xFF) as u64).min(25);
                let r = hashv(&[&automation_v2.authority.to_bytes(), &round.id.to_le_bytes()]).0;
                squares = generate_random_mask(num_squares, &r);
            }
        }
    } else {
        // Convert provided 32-bit mask into array of 25 booleans, where each bit in the mask
        // determines if that square index is selected (true) or not (false)
        for i in 0..25 {
            squares[i] = (mask & (1 << i)) != 0;
        }
    }

    // Open miner account.
    let miner = if miner_info.data_is_empty() {
        create_program_account::<Miner>(
            miner_info,
            system_program,
            signer_info,
            &godl_api::ID,
            &[MINER, &authority_info.key.to_bytes()],
        )?;
        let miner = miner_info.as_account_mut::<Miner>(&godl_api::ID)?;
        miner.authority = *authority_info.key;
        miner.referrer = Pubkey::default();
        miner.deployed = [0; 25];
        miner.cumulative = [0; 25];
        miner.rewards_sol = 0;
        miner.rewards_godl = 0;
        miner.round_id = 0;
        miner.checkpoint_id = 0;
        miner.lifetime_deployed = 0;
        miner.lifetime_rewards_sol = 0;
        miner.lifetime_rewards_godl = 0;
        miner
    } else {
        miner_info
            .as_account_mut::<Miner>(&godl_api::ID)?
            .assert_mut(|m| {
                if let Some(automation_v2) = &automation_v2 {
                    m.authority == automation_v2.authority
                } else {
                    m.authority == *authority_info.key
                }
            })?
    };

    // Reset miner
    if miner.round_id != round.id {
        // Assert miner has checkpointed prior round.
        assert!(
            miner.checkpoint_id == miner.round_id,
            "Miner has not checkpointed"
        );

        // Reset miner for new round.
        miner.deployed = [0; 25];
        miner.cumulative = round.deployed;
        miner.round_id = round.id;
    } else {
        // Do not allow multiple deployments in the same round.
        if miner.deployed != [0; 25] {
            return Err(GodlError::AlreadyDeployedThisRound.into());
        }
    }

    // Calculate how many new squares this automation intends to play this round.
    let mut squares_to_deploy: u64 = 0;
    for (square_id, &should_deploy) in squares.iter().enumerate() {
        if square_id > 24 {
            break;
        }
        if !should_deploy {
            continue;
        }
        // Only count squares we haven't already deployed to this round.
        if miner.deployed[square_id] > 0 {
            continue;
        }
        squares_to_deploy += 1;
    }

    // If this is an automation, ensure it can fund all requested squares this round.
    if let Some(automation_v2) = &automation_v2 {
        let required_for_round = amount
            .checked_mul(squares_to_deploy)
            .and_then(|v| v.checked_add(automation_v2.fee))
            .ok_or(ProgramError::InvalidInstructionData)?;
        if automation_v2.balance < required_for_round {
            // Not enough balance to fund all requested squares: close automation and do not place a partial bet.
            automation_v2_info.close(authority_info)?;
            return Ok(());
        }
    }

    // Calculate all deployments.
    let mut total_amount = 0;
    let mut total_squares = 0;
    for (square_id, &should_deploy) in squares.iter().enumerate() {
        // Skip if square index is out of bounds.
        if square_id > 24 {
            break;
        }

        // Skip if square is not deployed to.
        if !should_deploy {
            continue;
        }

        // Skip if miner already deployed to this square.
        if miner.deployed[square_id] > 0 {
            continue;
        }

        // Record cumulative amount.
        miner.cumulative[square_id] = round.deployed[square_id];

        // Update miner
        miner.deployed[square_id] = amount;

        // Update board
        round.deployed[square_id] += amount;
        round.total_deployed += amount;
        round.count[square_id] += 1;

        if is_pooled {
            let pool_member = if pool_member_info.data_is_empty() {
                create_program_account::<PoolMember>(
                    pool_member_info,
                    system_program,
                    signer_info,
                    &godl_api::ID,
                    &[POOL_MEMBER, &authority_info.key.to_bytes()],
                )?;
                let pool_member = pool_member_info.as_account_mut::<PoolMember>(&godl_api::ID)?;
                pool_member.authority = *authority_info.key;
                pool_member.round_id = 0;
                pool_member.deployed = [0; 25];
                pool_member.total_deployed = 0;
                pool_member
            } else {
                pool_member_info
                    .as_account_mut::<PoolMember>(&godl_api::ID)?
                    .assert_mut(|p| p.authority == *authority_info.key)?
            };
            if pool_member.round_id != round.id {
                pool_member.reset_for_round(round.id);
            }
            let was_zero = pool_member.deployed[square_id] == 0;
            pool_member.record_deploy(square_id, amount);
            pool_round.record_deploy(square_id, amount, was_zero);
        }

        // Update totals.
        total_amount += amount;
        total_squares += 1;

        // Exit early if automation does not have enough balance for another square.
        if let Some(automation_v2) = &automation_v2 {
            if total_amount + automation_v2.fee + amount > automation_v2.balance {
                break;
            }
        }
    }

    // Update lifetime deployed.
    miner.lifetime_deployed += total_amount;

    // Top up checkpoint fee.
    if miner.checkpoint_fee == 0 {
        miner.checkpoint_fee = CHECKPOINT_FEE;
        miner_info.collect(CHECKPOINT_FEE, &signer_info)?;
    }

    if is_pooled {
        let pool_member_total = pool_member_info
            .as_account::<PoolMember>(&godl_api::ID)?
            .total_deployed;
        if pool_member_total > LAMPORTS_PER_SOL {
            return Err(ProgramError::InvalidInstructionData);
        }
    }

    // Transfer SOL.
    if let Some(automation_v2) = automation_v2 {
        automation_v2.balance -= total_amount + automation_v2.fee;
        automation_v2_info.send(total_amount, &round_info);
        automation_v2_info.send(automation_v2.fee, &signer_info);

        // Close automation if balance is less than what's required to deploy the next round.
        if automation_v2.balance < automation_v2.amount + automation_v2.fee {
            automation_v2_info.close(authority_info)?;
        }
    } else {
        round_info.collect(total_amount, &signer_info)?;
    }

    // Log
    let mode = if is_pooled { "pooled" } else { "solo" };
    sol_log(
        &format!(
            "Round #{}: deploying {} SOL to {} squares ({})",
            round.id,
            lamports_to_sol(amount),
            total_squares,
            mode,
        )
        .as_str(),
    );

    Ok(())
}

fn generate_random_mask(num_squares: u64, r: &[u8]) -> [bool; 25] {
    let mut new_mask = [false; 25];
    let mut selected = 0;
    for i in 0..25 {
        let rand_byte = r[i];
        let remaining_needed = num_squares as u64 - selected as u64;
        let remaining_positions = 25 - i;
        if remaining_needed > 0
            && (rand_byte as u64) * (remaining_positions as u64) < (remaining_needed * 256)
        {
            new_mask[i] = true;
            selected += 1;
        }
    }
    new_mask
}
