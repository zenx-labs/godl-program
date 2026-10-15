//! `UpdateProtocolConfig` and config-driven round timing (ore-style): the round length and
//! intermission live in `Config` so the operator can retune them when Solana's slot time
//! moves. Covers the admin gate and bounds, the one-time in-place growth of a pre-timing
//! config account, `DeployWithMinerExtended` opening a round with `config.round_slots`, and
//! the permissionless reset honouring `config.intermission_slots`.

mod common;

use common::*;
use godl_api::prelude::*;
use solana_program::pubkey::Pubkey;
use solana_program_test::{processor, ProgramTestContext};
use solana_sdk::{
    account::Account,
    instruction::InstructionError,
    native_token::LAMPORTS_PER_SOL,
    rent::Rent,
    signature::{Keypair, Signer},
    transaction::TransactionError,
};
use steel::*;

/// On-chain length of the config account before the timing fields were appended.
const LEGACY_CONFIG_LEN: usize = 8 + 176;

fn ix_update(signer: &Keypair, round_slots: u64, intermission_slots: u64) -> Instruction {
    godl_api::sdk::update_protocol_config(signer.pubkey(), round_slots, intermission_slots)
}

fn assert_ix_err(result: Result<(), TransactionError>, expected: InstructionError) {
    match result {
        Err(TransactionError::InstructionError(_, e)) if e == expected => {}
        other => panic!("expected {expected:?}, got {other:?}"),
    }
}

async fn raw_account(ctx: &mut ProgramTestContext, addr: Pubkey) -> Account {
    ctx.banks_client.get_account(addr).await.unwrap().unwrap()
}

#[tokio::test]
async fn update_sets_timing_and_is_admin_gated() {
    let admin = Keypair::new();
    let stranger = Keypair::new();
    let mut ctx = EnvBuilder::new(admin.pubkey())
        .fund(admin.pubkey())
        .fund(stranger.pubkey())
        .start()
        .await;

    // The harness config is zeroed: the accessors fall back to the compile-time defaults.
    let c: Config = get(&mut ctx, config_pda().0).await;
    assert_eq!((c.round_slots, c.intermission_slots), (0, 0));
    assert_eq!(
        (c.round_slots(), c.intermission_slots()),
        (ROUND_SLOTS, INTERMISSION_SLOTS)
    );

    // Only the config admin may update.
    assert_custom_err(
        send(&mut ctx, &[&stranger], &[ix_update(&stranger, 300, 50)]).await,
        GodlError::NotAuthorized as u32,
    );

    send(&mut ctx, &[&admin], &[ix_update(&admin, 300, 50)])
        .await
        .unwrap();
    let c: Config = get(&mut ctx, config_pda().0).await;
    assert_eq!((c.round_slots, c.intermission_slots), (300, 50));
    assert_eq!((c.round_slots(), c.intermission_slots()), (300, 50));
    assert_eq!(c.admin, admin.pubkey());

    // Zero (reserved for "unset") and more than a day are rejected, leaving the values alone.
    for (r, i) in [
        (0, 50),
        (300, 0),
        (ONE_DAY_SLOTS + 1, 50),
        (300, ONE_DAY_SLOTS + 1),
    ] {
        assert_custom_err(
            send(&mut ctx, &[&admin], &[ix_update(&admin, r, i)]).await,
            GodlError::InvalidProtocolConfig as u32,
        );
    }
    let c: Config = get(&mut ctx, config_pda().0).await;
    assert_eq!((c.round_slots, c.intermission_slots), (300, 50));
}

/// The mainnet config account predates the timing fields. The first update grows it in place
/// (signer pays the rent delta); every earlier field keeps its value and offset.
#[tokio::test]
async fn first_update_grows_a_legacy_config_account() {
    let admin = Keypair::new();
    let mut legacy = Config::zeroed();
    legacy.admin = admin.pubkey();
    legacy.godl_per_round = 7 * GODL;
    legacy.bury_authority = Pubkey::new_unique();
    legacy.fee_collector = Pubkey::new_unique();
    legacy.swap_program = Pubkey::new_unique();
    legacy.var_address = Pubkey::new_unique();
    legacy.motherlode_denominator = 5;
    let mut data = vec![0u8; LEGACY_CONFIG_LEN];
    data[0] = Config::discriminator();
    data[8..].copy_from_slice(&bytemuck::bytes_of(&legacy)[..LEGACY_CONFIG_LEN - 8]);
    let old_lamports = Rent::default().minimum_balance(LEGACY_CONFIG_LEN);
    let mut ctx = EnvBuilder::new(admin.pubkey())
        .fund(admin.pubkey())
        .account(
            config_pda().0,
            Account {
                lamports: old_lamports,
                data,
                owner: godl_api::ID,
                executable: false,
                rent_epoch: 0,
            },
        )
        .start()
        .await;
    let rent: Rent = ctx.banks_client.get_rent().await.unwrap();
    assert_eq!(rent.minimum_balance(LEGACY_CONFIG_LEN), old_lamports);

    // Until it is grown, the new program cannot read the config at all.
    assert_ix_err(
        send(
            &mut ctx,
            &[&admin],
            &[godl_api::sdk::set_godl_per_round(admin.pubkey(), GODL)],
        )
        .await,
        InstructionError::InvalidAccountData,
    );

    let admin_before = lamports(&mut ctx, admin.pubkey()).await;
    send(&mut ctx, &[&admin], &[ix_update(&admin, 300, 50)])
        .await
        .unwrap();

    let new_len = 8 + core::mem::size_of::<Config>();
    let raw = raw_account(&mut ctx, config_pda().0).await;
    assert_eq!(raw.data.len(), new_len);
    assert_eq!(raw.lamports, rent.minimum_balance(new_len));
    let rent_delta = rent.minimum_balance(new_len) - old_lamports;
    let admin_paid = admin_before - lamports(&mut ctx, admin.pubkey()).await;
    assert!(
        admin_paid >= rent_delta && admin_paid <= rent_delta + 10_000,
        "admin paid {admin_paid}, rent delta {rent_delta}"
    );

    let c = *Config::try_from_bytes(&raw.data).unwrap();
    assert_eq!(c.admin, legacy.admin);
    assert_eq!(c.godl_per_round, legacy.godl_per_round);
    assert_eq!(c.bury_authority, legacy.bury_authority);
    assert_eq!(c.fee_collector, legacy.fee_collector);
    assert_eq!(c.swap_program, legacy.swap_program);
    assert_eq!(c.var_address, legacy.var_address);
    assert_eq!(c.motherlode_denominator, legacy.motherlode_denominator);
    assert_eq!((c.round_slots, c.intermission_slots), (300, 50));
    assert_eq!(c.buffer, [0; 32]);

    // From here on the account is a plain update target, and other config reads work again.
    send(&mut ctx, &[&admin], &[ix_update(&admin, 150, 25)])
        .await
        .unwrap();
    send(
        &mut ctx,
        &[&admin],
        &[godl_api::sdk::set_godl_per_round(admin.pubkey(), GODL)],
    )
    .await
    .unwrap();
    let c: Config = get(&mut ctx, config_pda().0).await;
    assert_eq!((c.round_slots, c.intermission_slots), (150, 25));
    assert_eq!(c.godl_per_round, GODL);
    assert_eq!(
        raw_account(&mut ctx, config_pda().0).await.data.len(),
        new_len
    );
}

/// Round 1 ended at slot 1000 with nobody deployed. With `intermission_slots = 50` the
/// permissionless reset is refused at 1049 and lands at 1050; the first deploy on round 2 then
/// closes the board at `start_slot + round_slots` (300), not the old hardcoded 150.
#[tokio::test]
async fn deploy_and_reset_use_config_timing() {
    let admin = Keypair::new();
    let miner = Keypair::new();
    let var = Pubkey::new_unique();
    let mut config = round_config(admin.pubkey(), var);
    config.round_slots = 300;
    config.intermission_slots = 50;

    let mut board = Board::zeroed();
    board.round_id = 1;
    board.start_slot = 850;
    board.end_slot = 1000;
    let mut round = Round::zeroed();
    round.id = 1;
    round.expires_at = 1000 + ONE_DAY_SLOTS;
    round.rent_payer = admin.pubkey();

    let mut ctx = EnvBuilder::new(admin.pubkey())
        .fund(admin.pubkey())
        .fund(miner.pubkey())
        .account(config_pda().0, pod_account(&config))
        .account(board_pda().0, pod_account(&board))
        .account(round_pda(1).0, pod_account(&round))
        .account(
            sol_motherlode_pda().0,
            pod_account(&SolMotherlode::zeroed()),
        )
        .account(gold_vault_pda().0, pod_account(&GoldVault::zeroed()))
        .account(RUSH_SOL_VAULT, system_account(LAMPORTS_PER_SOL))
        .account(var, entropy_var_account(board_pda().0, [u8::MAX; 32]))
        .start_with(|pt| {
            pt.add_builtin_program(
                "mock_entropy",
                entropy_api::ID,
                builtin(processor!(mock_entropy)),
            )
        })
        .await;

    let ix_reset = godl_api::sdk::reset_permissionless(
        miner.pubkey(),
        admin.pubkey(),
        1,
        Pubkey::new_unique(),
        var,
    );

    // One slot short of the configured intermission (the default 25 would already allow it).
    ctx.warp_to_slot(1049).unwrap();
    assert_ix_err(
        send(&mut ctx, &[&miner], &[ix_reset.clone()]).await,
        InstructionError::InvalidAccountData,
    );
    let b: Board = get(&mut ctx, board_pda().0).await;
    assert_eq!(b.round_id, 1);

    ctx.warp_to_slot(1050).unwrap();
    send(&mut ctx, &[&miner], &[ix_reset]).await.unwrap();
    let b: Board = get(&mut ctx, board_pda().0).await;
    assert_eq!(b.round_id, 2);
    assert_eq!(b.start_slot, 1050);
    assert_eq!(b.end_slot, u64::MAX);
    let r2: Round = get(&mut ctx, round_pda(2).0).await;
    assert_eq!(r2.id, 2);
    assert_eq!(r2.expires_at, u64::MAX);

    // The first deploy opens round 2 with the configured round length.
    let mut squares = [false; 25];
    squares[3] = true;
    let ix_deploy = godl_api::sdk::deploy_with_miner_extended(
        miner.pubkey(),
        miner.pubkey(),
        var,
        LAMPORTS_PER_SOL / 10,
        2,
        squares,
        false,
    );
    send(&mut ctx, &[&miner], &[ix_deploy]).await.unwrap();
    let b: Board = get(&mut ctx, board_pda().0).await;
    assert_eq!(b.round_id, 2);
    assert_eq!(b.start_slot, 1050);
    assert_eq!(b.end_slot, 1050 + 300);
    let r2: Round = get(&mut ctx, round_pda(2).0).await;
    assert_eq!(r2.expires_at, b.end_slot + ONE_DAY_SLOTS);
    assert_eq!(r2.deployed[3], LAMPORTS_PER_SOL / 10);
    let m: Miner = get(&mut ctx, miner_pda(miner.pubkey()).0).await;
    assert_eq!(m.round_id, 2);
}
