use godl_api::prelude::*;
use steel::*;

/// Initializes the core program PDAs.
pub fn process_initialize(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    // Parse data.
    let args = Initialize::try_from_bytes(data)?;
    let admin = Pubkey::new_from_array(args.admin);
    let bury_authority = Pubkey::new_from_array(args.bury_authority);
    let fee_collector = Pubkey::new_from_array(args.fee_collector);
    let godl_per_round = u64::from_le_bytes(args.godl_per_round);

    // Sanity check.
    assert!(
        godl_per_round <= MAX_GODL_PER_ROUND,
        "GODL per round cannot be greater than MAX_GODL_PER_ROUND"
    );

    // Load accounts.
    let [signer_info, config_info, treasury_info, board_info, round_info, system_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    signer_info.has_address(&DEPLOYER_ADDRESS)?;
    system_program.is_program(&system_program::ID)?;

    config_info
        .is_empty()?
        .is_writable()?
        .has_seeds(&[CONFIG], &godl_api::ID)?;
    treasury_info
        .is_empty()?
        .is_writable()?
        .has_seeds(&[TREASURY], &godl_api::ID)?;
    board_info
        .is_empty()?
        .is_writable()?
        .has_seeds(&[BOARD], &godl_api::ID)?;
    round_info
        .is_empty()?
        .is_writable()?
        .has_seeds(&[ROUND, &0u64.to_le_bytes()], &godl_api::ID)?;

    // Create accounts.
    create_program_account::<Config>(
        config_info,
        system_program,
        signer_info,
        &godl_api::ID,
        &[CONFIG],
    )?;
    create_program_account::<Treasury>(
        treasury_info,
        system_program,
        signer_info,
        &godl_api::ID,
        &[TREASURY],
    )?;
    create_program_account::<Board>(
        board_info,
        system_program,
        signer_info,
        &godl_api::ID,
        &[BOARD],
    )?;
    create_program_account::<Round>(
        round_info,
        system_program,
        signer_info,
        &godl_api::ID,
        &[ROUND, &0u64.to_le_bytes()],
    )?;

    // Initialize config.
    let config = config_info.as_account_mut::<Config>(&godl_api::ID)?;
    config.admin = admin;
    config.bury_authority = bury_authority;
    config.fee_collector = fee_collector;
    config.swap_program = Pubkey::default();
    config.var_address = Pubkey::default();
    config.godl_per_round = godl_per_round;
    config.motherlode_denominator = 5;
    config.round_slots = ROUND_SLOTS;
    config.intermission_slots = INTERMISSION_SLOTS;

    // Initialize treasury.
    let treasury = treasury_info.as_account_mut::<Treasury>(&godl_api::ID)?;
    treasury.balance = 0;
    treasury.motherlode = 0;
    treasury.miner_rewards_factor = Numeric::ZERO;
    treasury.stake_rewards_factor = Numeric::ZERO;
    treasury.total_staked = 0;
    treasury.total_unclaimed = 0;
    treasury.total_refined = 0;

    // Initialize board.
    let board = board_info.as_account_mut::<Board>(&godl_api::ID)?;
    board.round_id = 0;
    board.start_slot = 0;
    board.end_slot = u64::MAX;

    // Initialize current round.
    let round = round_info.as_account_mut::<Round>(&godl_api::ID)?;
    round.id = 0;
    round.deployed = [0; 25];
    round.slot_hash = [0; 32];
    round.count = [0; 25];
    round.expires_at = u64::MAX;
    round.motherlode = 0;
    round.rent_payer = *signer_info.key;
    round.top_miner = Pubkey::default();
    round.top_miner_reward = 0;
    round.total_deployed = 0;
    round.total_vaulted = 0;
    round.total_winnings = 0;

    Ok(())
}
