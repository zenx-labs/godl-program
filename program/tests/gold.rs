//! Smoke tests for the SOL-motherlode -> XAUt0 gold rewards flow.
//!
//! Covers: pro-rata distribution by unrefined GODL, lazy/late `MinerExtended`
//! creation, the gold sync at the mutation sites (inject, claim GODL,
//! checkpoint), pending XAUt0 when nothing is unrefined, the legacy checkpoint
//! expiry / distribution lock, and `BuyGold` end-to-end against a mock swap
//! program.

mod common;

use common::*;
use godl_api::prelude::*;
use solana_program::{
    account_info::AccountInfo,
    entrypoint::ProgramResult,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
};
use solana_program_test::{processor, ProgramTestContext};
use solana_sdk::{
    account::Account,
    instruction::AccountMeta,
    native_token::LAMPORTS_PER_SOL,
    program_option::COption,
    program_pack::Pack,
    pubkey::Pubkey,
    rent::Rent,
    signature::{Keypair, Signer},
};
use spl_associated_token_account::get_associated_token_address;
use steel::*;

/// 1 XAUt0 in base units.
const XAUT: u64 = 1_000_000;

const MOCK_SWAP_ID: Pubkey = Pubkey::new_from_array([7u8; 32]);

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn spl_mint(decimals: u8) -> Account {
    let mut data = vec![0u8; spl_token::state::Mint::LEN];
    spl_token::state::Mint {
        mint_authority: COption::None,
        supply: 0,
        decimals,
        is_initialized: true,
        freeze_authority: COption::None,
    }
    .pack_into_slice(&mut data);
    Account {
        lamports: 10_000_000_000,
        data,
        owner: spl_token::ID,
        executable: false,
        rent_epoch: 0,
    }
}

fn spl_token_account(mint: Pubkey, owner: &Pubkey, amount: u64, native: bool) -> Account {
    let rent = Rent::default().minimum_balance(spl_token::state::Account::LEN);
    let mut data = vec![0u8; spl_token::state::Account::LEN];
    spl_token::state::Account {
        mint,
        owner: *owner,
        amount,
        delegate: COption::None,
        state: spl_token::state::AccountState::Initialized,
        is_native: if native {
            COption::Some(rent)
        } else {
            COption::None
        },
        delegated_amount: 0,
        close_authority: COption::None,
    }
    .pack_into_slice(&mut data);
    Account {
        lamports: if native {
            rent + amount
        } else {
            10_000_000_000
        },
        data,
        owner: spl_token::ID,
        executable: false,
        rent_epoch: 0,
    }
}

fn xaut_ata(owner: &Pubkey) -> Pubkey {
    get_associated_token_address(owner, &XAUT_MINT)
}

fn miner_state(authority: Pubkey, unrefined: u64) -> Miner {
    let mut m = Miner::zeroed();
    m.authority = authority;
    m.rewards_godl = unrefined;
    m.lifetime_rewards_godl = unrefined;
    m
}

/// A resolved round 1 (rng = 1 -> winning square 1, split GODL reward) plus a
/// board already on round 2, and a miner that deployed 1 SOL on `square`.
fn resolved_round(admin: &Pubkey) -> (Round, Board) {
    let mut round = Round::zeroed();
    round.id = 1;
    round.slot_hash[0] = 1;
    round.deployed[1] = LAMPORTS_PER_SOL;
    round.count[1] = 1;
    round.total_deployed = LAMPORTS_PER_SOL;
    round.expires_at = 10_000_000_000;
    round.rent_payer = *admin;
    round.top_miner = SPLIT_ADDRESS;
    round.top_miner_reward = 10 * GODL;
    assert_eq!(round.winning_square(round.rng().unwrap()), 1);
    let mut board = Board::zeroed();
    board.round_id = 2;
    (round, board)
}

fn round_miner(authority: Pubkey, square: usize) -> Miner {
    let mut m = miner_state(authority, 0);
    m.round_id = 1;
    m.deployed[square] = LAMPORTS_PER_SOL;
    m.checkpoint_fee = CHECKPOINT_FEE;
    m
}

fn pool_authority() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"pool"], &MOCK_SWAP_ID)
}

/// Mock swap program: drains the taker's WSOL into the pool and pays out
/// `xaut_out` (instruction data, u64 LE) from the pool's XAUt0 account.
fn mock_swap(_program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    // Trailing accounts (the swap program's own account, like Jupiter's `program`) are ignored.
    let [taker, taker_sol, taker_xaut, pool_sol, pool_xaut, pool_auth, token_program, ..] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    let xaut_out = u64::from_le_bytes(data[..8].try_into().unwrap());
    let sol_in = spl_token::state::Account::unpack(&taker_sol.data.borrow())?.amount;
    invoke(
        &spl_token::instruction::transfer(
            token_program.key,
            taker_sol.key,
            pool_sol.key,
            taker.key,
            &[],
            sol_in,
        )?,
        &[
            taker_sol.clone(),
            pool_sol.clone(),
            taker.clone(),
            token_program.clone(),
        ],
    )?;
    let (_, bump) = pool_authority();
    invoke_signed(
        &spl_token::instruction::transfer(
            token_program.key,
            pool_xaut.key,
            taker_xaut.key,
            pool_auth.key,
            &[],
            xaut_out,
        )?,
        &[
            pool_xaut.clone(),
            taker_xaut.clone(),
            pool_auth.clone(),
            token_program.clone(),
        ],
        &[&[b"pool", &[bump]]],
    )?;
    Ok(())
}

/// Environment with the gold vault initialized, an XAUt0 mint, `admin` holding
/// 100 XAUt0 and 10_000 GODL (for deposits / injects), and the given miners
/// crafted with unrefined GODL balances (treasury.total_unclaimed = their sum).
fn gold_env(admin: &Keypair, miners: &[(Pubkey, u64)]) -> EnvBuilder {
    let gold_vault = gold_vault_pda().0;
    let mut env = EnvBuilder::new(admin.pubkey())
        .token(admin.pubkey(), 10_000 * GODL)
        .total_unclaimed(miners.iter().map(|(_, u)| *u).sum())
        .account(XAUT_MINT, spl_mint(XAUT_DECIMALS))
        .account(gold_vault, pod_account(&GoldVault::zeroed()))
        .account(
            gold_vault_xaut_address(),
            spl_token_account(XAUT_MINT, &gold_vault, 0, false),
        )
        .account(
            xaut_ata(&admin.pubkey()),
            spl_token_account(XAUT_MINT, &admin.pubkey(), 100 * XAUT, false),
        );
    for (authority, unrefined) in miners {
        env = env.fund(*authority).account(
            miner_pda(*authority).0,
            pod_account(&miner_state(*authority, *unrefined)),
        );
    }
    env
}

/// Pins the clock sysvar's unix timestamp (slot and epoch untouched).
async fn set_time(ctx: &mut ProgramTestContext, unix_timestamp: i64) {
    let mut clock: Clock = ctx.banks_client.get_sysvar().await.unwrap();
    clock.unix_timestamp = unix_timestamp;
    ctx.set_sysvar(&clock);
}

/// Gold distributions are locked until the legacy checkpoint expires; every
/// test that distributes starts at that time.
async fn start_unlocked(env: EnvBuilder) -> ProgramTestContext {
    let mut ctx = env.start().await;
    set_time(&mut ctx, LEGACY_CHECKPOINT_EXPIRY_TS).await;
    ctx
}

async fn claimable_xaut(ctx: &mut ProgramTestContext, who: &Pubkey) -> u64 {
    token_balance(ctx, xaut_ata(who)).await
}

/// The rewards factor is a truncating I80F48 fixed-point, so every accrual
/// event may floor away up to one base unit (1e-6 XAUt0). Dust stays in the
/// vault; it is never over-paid.
fn assert_dust(actual: u64, expected: u64, events: u64) {
    assert!(
        actual <= expected && actual + events >= expected,
        "expected {expected} (minus at most {events} dust), got {actual}"
    );
}

fn ix_create_ext(payer: &Keypair, authority: Pubkey) -> Instruction {
    godl_api::sdk::create_miner_extended(payer.pubkey(), authority)
}

fn ix_deposit(admin: &Keypair, amount: u64) -> Instruction {
    godl_api::sdk::deposit_gold(admin.pubkey(), amount)
}

fn ix_claim_gold(who: &Keypair) -> Instruction {
    godl_api::sdk::claim_gold(who.pubkey())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn distributes_pro_rata_to_unrefined_holders() {
    let admin = Keypair::new();
    let a = Keypair::new();
    let b = Keypair::new();
    let mut ctx = start_unlocked(gold_env(
        &admin,
        &[(a.pubkey(), 100 * GODL), (b.pubkey(), 300 * GODL)],
    ))
    .await;

    // Bulk-create both extended accounts in one tx (permissionless, admin pays).
    send(
        &mut ctx,
        &[&admin],
        &[
            ix_create_ext(&admin, a.pubkey()),
            ix_create_ext(&admin, b.pubkey()),
        ],
    )
    .await
    .unwrap();
    // Idempotent re-run is a no-op.
    send(&mut ctx, &[&admin], &[ix_create_ext(&admin, a.pubkey())])
        .await
        .unwrap();

    send(&mut ctx, &[&admin], &[ix_deposit(&admin, XAUT)])
        .await
        .unwrap();
    let vault: GoldVault = get(&mut ctx, gold_vault_pda().0).await;
    assert_eq!(vault.total_xaut_distributed, XAUT);
    assert_eq!(vault.pending_xaut, 0);
    assert!(vault.xaut_rewards_factor > Numeric::ZERO);

    send(&mut ctx, &[&a], &[ix_claim_gold(&a)]).await.unwrap();
    send(&mut ctx, &[&b], &[ix_claim_gold(&b)]).await.unwrap();
    let paid_a = claimable_xaut(&mut ctx, &a.pubkey()).await;
    let paid_b = claimable_xaut(&mut ctx, &b.pubkey()).await;
    assert_dust(paid_a, 250_000, 1);
    assert_dust(paid_b, 750_000, 1);
    assert_eq!(
        token_balance(&mut ctx, gold_vault_xaut_address()).await,
        XAUT - paid_a - paid_b
    );

    let ext_a: MinerExtended = get(&mut ctx, miner_extended_pda(a.pubkey()).0).await;
    assert_eq!(ext_a.authority, a.pubkey());
    assert_eq!(ext_a.xaut_rewards, 0);
    assert_eq!(ext_a.lifetime_xaut, paid_a);
    let vault: GoldVault = get(&mut ctx, gold_vault_pda().0).await;
    assert_eq!(vault.total_xaut_claimed, paid_a + paid_b);

    // A second claim pays nothing more.
    send(&mut ctx, &[&a], &[ix_claim_gold(&a)]).await.unwrap();
    assert_eq!(claimable_xaut(&mut ctx, &a.pubkey()).await, paid_a);
}

#[tokio::test]
async fn late_extended_account_earns_nothing_from_the_past() {
    let admin = Keypair::new();
    let a = Keypair::new();
    let b = Keypair::new();
    let mut ctx = start_unlocked(gold_env(
        &admin,
        &[(a.pubkey(), 100 * GODL), (b.pubkey(), 300 * GODL)],
    ))
    .await;

    send(&mut ctx, &[&admin], &[ix_create_ext(&admin, a.pubkey())])
        .await
        .unwrap();
    send(&mut ctx, &[&admin], &[ix_deposit(&admin, XAUT)])
        .await
        .unwrap();

    // B opts in late through ClaimGold itself (lazy creation) and gets nothing for the past.
    assert!(!account_exists(&mut ctx, miner_extended_pda(b.pubkey()).0).await);
    send(&mut ctx, &[&b], &[ix_claim_gold(&b)]).await.unwrap();
    assert!(account_exists(&mut ctx, miner_extended_pda(b.pubkey()).0).await);
    assert_eq!(claimable_xaut(&mut ctx, &b.pubkey()).await, 0);

    // From now on both share.
    send(&mut ctx, &[&admin], &[ix_deposit(&admin, XAUT)])
        .await
        .unwrap();
    send(&mut ctx, &[&a], &[ix_claim_gold(&a)]).await.unwrap();
    send(&mut ctx, &[&b], &[ix_claim_gold(&b)]).await.unwrap();
    let paid_a = claimable_xaut(&mut ctx, &a.pubkey()).await;
    let paid_b = claimable_xaut(&mut ctx, &b.pubkey()).await;
    assert_dust(paid_a, 500_000, 2);
    assert_dust(paid_b, 750_000, 1);
    // B's share of the first deposit is stranded in the vault, never over-claimed.
    assert_eq!(
        token_balance(&mut ctx, gold_vault_xaut_address()).await,
        2 * XAUT - paid_a - paid_b
    );
}

#[tokio::test]
async fn inject_unrefined_rewards_syncs_gold_before_changing_weight() {
    let admin = Keypair::new();
    let a = Keypair::new();
    let b = Keypair::new();
    let mut ctx = start_unlocked(gold_env(
        &admin,
        &[(a.pubkey(), 100 * GODL), (b.pubkey(), 100 * GODL)],
    ))
    .await;
    send(
        &mut ctx,
        &[&admin],
        &[
            ix_create_ext(&admin, a.pubkey()),
            ix_create_ext(&admin, b.pubkey()),
        ],
    )
    .await
    .unwrap();

    send(&mut ctx, &[&admin], &[ix_deposit(&admin, XAUT)])
        .await
        .unwrap();

    // Inject 200 GODL to A: A now holds 300 of 400.
    send(
        &mut ctx,
        &[&admin],
        &[godl_api::sdk::inject_unrefined_rewards(
            admin.pubkey(),
            a.pubkey(),
            200 * GODL,
        )],
    )
    .await
    .unwrap();
    let treasury: Treasury = get(&mut ctx, treasury_pda().0).await;
    assert_eq!(treasury.total_unclaimed, 400 * GODL);
    let ext_a: MinerExtended = get(&mut ctx, miner_extended_pda(a.pubkey()).0).await;
    assert_dust(ext_a.xaut_rewards, 500_000, 1); // settled at the old weight

    send(&mut ctx, &[&admin], &[ix_deposit(&admin, XAUT)])
        .await
        .unwrap();
    send(&mut ctx, &[&a], &[ix_claim_gold(&a)]).await.unwrap();
    send(&mut ctx, &[&b], &[ix_claim_gold(&b)]).await.unwrap();
    let paid_a = claimable_xaut(&mut ctx, &a.pubkey()).await;
    let paid_b = claimable_xaut(&mut ctx, &b.pubkey()).await;
    assert_dust(paid_a, 1_250_000, 2);
    assert_dust(paid_b, 750_000, 2);
    assert_eq!(
        token_balance(&mut ctx, gold_vault_xaut_address()).await,
        2 * XAUT - paid_a - paid_b
    );
}

#[tokio::test]
async fn claim_godl_settles_gold_then_drops_weight() {
    let admin = Keypair::new();
    let a = Keypair::new();
    let b = Keypair::new();
    let mut ctx = start_unlocked(gold_env(
        &admin,
        &[(a.pubkey(), 100 * GODL), (b.pubkey(), 100 * GODL)],
    ))
    .await;
    send(
        &mut ctx,
        &[&admin],
        &[
            ix_create_ext(&admin, a.pubkey()),
            ix_create_ext(&admin, b.pubkey()),
        ],
    )
    .await
    .unwrap();
    send(&mut ctx, &[&admin], &[ix_deposit(&admin, XAUT)])
        .await
        .unwrap();

    // A claims all its GODL: weight -> 0, but the gold earned so far is settled first.
    send(&mut ctx, &[&a], &[godl_api::sdk::claim_godl(a.pubkey())])
        .await
        .unwrap();
    let miner_a: Miner = get(&mut ctx, miner_pda(a.pubkey()).0).await;
    assert_eq!(miner_a.rewards_godl, 0);
    let treasury: Treasury = get(&mut ctx, treasury_pda().0).await;
    assert_eq!(treasury.total_unclaimed, 100 * GODL);
    let ext_a: MinerExtended = get(&mut ctx, miner_extended_pda(a.pubkey()).0).await;
    assert_dust(ext_a.xaut_rewards, 500_000, 1);

    // Everything from here goes to B.
    send(&mut ctx, &[&admin], &[ix_deposit(&admin, XAUT)])
        .await
        .unwrap();
    send(&mut ctx, &[&a], &[ix_claim_gold(&a)]).await.unwrap();
    send(&mut ctx, &[&b], &[ix_claim_gold(&b)]).await.unwrap();
    assert_dust(claimable_xaut(&mut ctx, &a.pubkey()).await, 500_000, 1);
    assert_dust(claimable_xaut(&mut ctx, &b.pubkey()).await, 1_500_000, 2);
}

#[tokio::test]
async fn miner_without_unrefined_needs_no_extended_account() {
    let admin = Keypair::new();
    let c = Keypair::new();
    let mut ctx = start_unlocked(gold_env(&admin, &[(c.pubkey(), 0)])).await;

    // ClaimGODL with nothing unrefined and no extended account: allowed, creates nothing.
    send(&mut ctx, &[&c], &[godl_api::sdk::claim_godl(c.pubkey())])
        .await
        .unwrap();
    assert!(!account_exists(&mut ctx, miner_extended_pda(c.pubkey()).0).await);

    // A deposit with nothing unrefined is parked, not lost.
    send(&mut ctx, &[&admin], &[ix_deposit(&admin, XAUT)])
        .await
        .unwrap();
    let vault: GoldVault = get(&mut ctx, gold_vault_pda().0).await;
    assert_eq!(vault.pending_xaut, XAUT);
    assert_eq!(vault.xaut_rewards_factor, Numeric::ZERO);

    // Inject creates the extended account for C; the next deposit includes the pending XAUt0.
    send(
        &mut ctx,
        &[&admin],
        &[godl_api::sdk::inject_unrefined_rewards(
            admin.pubkey(),
            c.pubkey(),
            50 * GODL,
        )],
    )
    .await
    .unwrap();
    assert!(account_exists(&mut ctx, miner_extended_pda(c.pubkey()).0).await);
    send(&mut ctx, &[&admin], &[ix_deposit(&admin, XAUT)])
        .await
        .unwrap();
    let vault: GoldVault = get(&mut ctx, gold_vault_pda().0).await;
    assert_eq!(vault.pending_xaut, 0);
    assert_eq!(vault.total_xaut_distributed, 2 * XAUT);
    send(&mut ctx, &[&c], &[ix_claim_gold(&c)]).await.unwrap();
    assert_dust(claimable_xaut(&mut ctx, &c.pubkey()).await, 2 * XAUT, 1);
}

#[tokio::test]
async fn checkpoint_creates_extended_account_only_for_winners() {
    let admin = Keypair::new();
    let winner = Keypair::new();
    let loser = Keypair::new();
    let (mut round, board) = resolved_round(&admin.pubkey());
    round.deployed[0] = LAMPORTS_PER_SOL;
    round.count[0] = 1;
    round.total_deployed = 2 * LAMPORTS_PER_SOL;

    let mut ctx = start_unlocked(
        gold_env(&admin, &[])
            .fund(winner.pubkey())
            .fund(loser.pubkey())
            .account(board_pda().0, pod_account(&board))
            .account(round_pda(1).0, pod_account(&round))
            .account(
                miner_pda(winner.pubkey()).0,
                pod_account(&round_miner(winner.pubkey(), 1)),
            )
            .account(
                miner_pda(loser.pubkey()).0,
                pod_account(&round_miner(loser.pubkey(), 0)),
            ),
    )
    .await;

    send(
        &mut ctx,
        &[&loser],
        &[godl_api::sdk::checkpoint_with_miner_extended(
            loser.pubkey(),
            loser.pubkey(),
            1,
        )],
    )
    .await
    .unwrap();
    assert!(!account_exists(&mut ctx, miner_extended_pda(loser.pubkey()).0).await);
    let m: Miner = get(&mut ctx, miner_pda(loser.pubkey()).0).await;
    assert_eq!(m.checkpoint_id, 1);
    assert_eq!(m.rewards_godl, 0);

    send(
        &mut ctx,
        &[&winner],
        &[godl_api::sdk::checkpoint_with_miner_extended(
            winner.pubkey(),
            winner.pubkey(),
            1,
        )],
    )
    .await
    .unwrap();
    assert!(account_exists(&mut ctx, miner_extended_pda(winner.pubkey()).0).await);
    let m: Miner = get(&mut ctx, miner_pda(winner.pubkey()).0).await;
    assert_eq!(m.checkpoint_id, 1);
    assert_eq!(m.rewards_godl, 10 * GODL);
    let treasury: Treasury = get(&mut ctx, treasury_pda().0).await;
    assert_eq!(treasury.total_unclaimed, 10 * GODL);

    // The winner is now the only unrefined holder and receives the whole distribution.
    send(&mut ctx, &[&admin], &[ix_deposit(&admin, XAUT)])
        .await
        .unwrap();
    send(&mut ctx, &[&winner], &[ix_claim_gold(&winner)])
        .await
        .unwrap();
    assert_dust(claimable_xaut(&mut ctx, &winner.pubkey()).await, XAUT, 1);
}

#[tokio::test]
async fn buy_gold_swaps_motherlode_sol_and_distributes() {
    let admin = Keypair::new();
    let a = Keypair::new();
    let gold_vault = gold_vault_pda().0;
    let (pool_auth, _) = pool_authority();
    let pool_sol = Pubkey::new_unique();
    let pool_xaut = Pubkey::new_unique();

    let mut config = Config::zeroed();
    config.admin = admin.pubkey();
    config.bury_authority = admin.pubkey();
    config.fee_collector = admin.pubkey();
    config.swap_program = MOCK_SWAP_ID;

    let mut motherlode = SolMotherlode::zeroed();
    motherlode.amount = 5 * LAMPORTS_PER_SOL;

    let mut ctx = gold_env(&admin, &[(a.pubkey(), 100 * GODL)])
        .account(config_pda().0, pod_account(&config))
        .account(sol_motherlode_pda().0, pod_account(&motherlode))
        .account(SOL_MINT, spl_mint(9))
        .account(
            gold_vault_sol_address(),
            spl_token_account(SOL_MINT, &gold_vault, 0, true),
        )
        .account(pool_sol, spl_token_account(SOL_MINT, &pool_auth, 0, true))
        .account(
            pool_xaut,
            spl_token_account(XAUT_MINT, &pool_auth, 10 * XAUT, false),
        )
        .start_with(|pt| pt.add_program("mock_swap", MOCK_SWAP_ID, processor!(mock_swap)))
        .await;

    let swap_accounts = vec![
        AccountMeta::new(gold_vault, false),
        AccountMeta::new(gold_vault_sol_address(), false),
        AccountMeta::new(gold_vault_xaut_address(), false),
        AccountMeta::new(pool_sol, false),
        AccountMeta::new(pool_xaut, false),
        AccountMeta::new_readonly(pool_auth, false),
        AccountMeta::new_readonly(spl_token::ID, false),
        // The callee program must be in the instruction's account list for the CPI (Jupiter
        // routes include their own program account the same way).
        AccountMeta::new_readonly(MOCK_SWAP_ID, false),
    ];
    let swap_data = XAUT.to_le_bytes();
    let buy = |amount: u64, min_out: u64| {
        godl_api::sdk::buy_gold(admin.pubkey(), amount, min_out, &swap_accounts, &swap_data)
    };

    // Locked while the legacy checkpoint is still alive.
    set_time(&mut ctx, LEGACY_CHECKPOINT_EXPIRY_TS - 1).await;
    assert_custom_err(
        send(&mut ctx, &[&admin], &[buy(LAMPORTS_PER_SOL, 0)]).await,
        GodlError::GoldDistributionLocked as u32,
    );
    set_time(&mut ctx, LEGACY_CHECKPOINT_EXPIRY_TS).await;
    send(&mut ctx, &[&admin], &[ix_create_ext(&admin, a.pubkey())])
        .await
        .unwrap();
    let motherlode_lamports_before = lamports(&mut ctx, sol_motherlode_pda().0).await;

    // Over-spending the motherlode ledger is rejected.
    assert_custom_err(
        send(&mut ctx, &[&admin], &[buy(6 * LAMPORTS_PER_SOL, 0)]).await,
        GodlError::InsufficientMotherlodeBalance as u32,
    );

    // Slippage guard: the mock pays 1 XAUt0, we demand 2.
    assert_custom_err(
        send(&mut ctx, &[&admin], &[buy(2 * LAMPORTS_PER_SOL, 2 * XAUT)]).await,
        GodlError::GoldSlippageExceeded as u32,
    );
    let m: SolMotherlode = get(&mut ctx, sol_motherlode_pda().0).await;
    assert_eq!(m.amount, 5 * LAMPORTS_PER_SOL, "failed swap rolled back");

    // Only the bury authority may call.
    let res = send(
        &mut ctx,
        &[&a],
        &[godl_api::sdk::buy_gold(
            a.pubkey(),
            LAMPORTS_PER_SOL,
            0,
            &swap_accounts,
            &swap_data,
        )],
    )
    .await;
    assert!(res.is_err());

    send(&mut ctx, &[&admin], &[buy(2 * LAMPORTS_PER_SOL, XAUT)])
        .await
        .unwrap();

    let m: SolMotherlode = get(&mut ctx, sol_motherlode_pda().0).await;
    assert_eq!(m.amount, 3 * LAMPORTS_PER_SOL);
    assert_eq!(
        lamports(&mut ctx, sol_motherlode_pda().0).await,
        motherlode_lamports_before - 2 * LAMPORTS_PER_SOL
    );
    assert_eq!(token_balance(&mut ctx, gold_vault_sol_address()).await, 0);
    assert_eq!(
        token_balance(&mut ctx, pool_sol).await,
        2 * LAMPORTS_PER_SOL
    );
    assert_eq!(
        token_balance(&mut ctx, gold_vault_xaut_address()).await,
        XAUT
    );
    let vault: GoldVault = get(&mut ctx, gold_vault).await;
    assert_eq!(vault.total_sol_swapped, 2 * LAMPORTS_PER_SOL);
    assert_eq!(vault.total_xaut_distributed, XAUT);

    send(&mut ctx, &[&a], &[ix_claim_gold(&a)]).await.unwrap();
    assert_dust(claimable_xaut(&mut ctx, &a.pubkey()).await, XAUT, 1);
}

#[tokio::test]
async fn rebase_total_unclaimed_is_a_compare_and_swap() {
    let admin = Keypair::new();
    let a = Keypair::new();
    let mut ctx = start_unlocked(gold_env(&admin, &[(a.pubkey(), 100 * GODL)])).await;

    assert_custom_err(
        send(
            &mut ctx,
            &[&admin],
            &[godl_api::sdk::rebase_total_unclaimed(
                admin.pubkey(),
                99 * GODL,
                100 * GODL,
            )],
        )
        .await,
        GodlError::RebaseMismatch as u32,
    );
    send(
        &mut ctx,
        &[&admin],
        &[godl_api::sdk::rebase_total_unclaimed(
            admin.pubkey(),
            100 * GODL,
            120 * GODL,
        )],
    )
    .await
    .unwrap();
    let treasury: Treasury = get(&mut ctx, treasury_pda().0).await;
    assert_eq!(treasury.total_unclaimed, 120 * GODL);

    // Non-admin is rejected.
    assert_custom_err(
        send(
            &mut ctx,
            &[&a],
            &[godl_api::sdk::rebase_total_unclaimed(
                a.pubkey(),
                120 * GODL,
                100 * GODL,
            )],
        )
        .await,
        GodlError::NotAuthorized as u32,
    );
}

/// The legacy `Checkpoint` layout keeps working for un-migrated clients until the expiry
/// time: it credits GODL exactly as before and never touches gold accounts. Distributions are
/// locked for as long as it is alive, so the bulk creation with factor 0 keeps miners whole.
#[tokio::test]
async fn legacy_checkpoint_layout_works_until_expiry_and_locks_distribution() {
    let admin = Keypair::new();
    let winner = Keypair::new();
    let (round, board) = resolved_round(&admin.pubkey());

    let mut ctx = gold_env(&admin, &[])
        .fund(winner.pubkey())
        .account(board_pda().0, pod_account(&board))
        .account(round_pda(1).0, pod_account(&round))
        .account(
            miner_pda(winner.pubkey()).0,
            pod_account(&round_miner(winner.pubkey(), 1)),
        )
        .start()
        .await;
    set_time(&mut ctx, LEGACY_CHECKPOINT_EXPIRY_TS - 1).await;

    // Eight-account legacy layout.
    let ix = godl_api::sdk::checkpoint(winner.pubkey(), winner.pubkey(), 1);
    assert_eq!(ix.accounts.len(), 8);
    send(&mut ctx, &[&winner], &[ix]).await.unwrap();
    let m: Miner = get(&mut ctx, miner_pda(winner.pubkey()).0).await;
    assert_eq!(m.checkpoint_id, 1);
    assert_eq!(m.rewards_godl, 10 * GODL);
    assert!(!account_exists(&mut ctx, miner_extended_pda(winner.pubkey()).0).await);

    // Distribution is locked while the legacy layout is alive.
    assert_custom_err(
        send(&mut ctx, &[&admin], &[ix_deposit(&admin, XAUT)]).await,
        GodlError::GoldDistributionLocked as u32,
    );

    // The bulk job creates the extended account with factor 0; after expiry the miner is whole.
    send(
        &mut ctx,
        &[&admin],
        &[ix_create_ext(&admin, winner.pubkey())],
    )
    .await
    .unwrap();
    set_time(&mut ctx, LEGACY_CHECKPOINT_EXPIRY_TS).await;
    send(&mut ctx, &[&admin], &[ix_deposit(&admin, XAUT)])
        .await
        .unwrap();
    send(&mut ctx, &[&winner], &[ix_claim_gold(&winner)])
        .await
        .unwrap();
    assert_dust(claimable_xaut(&mut ctx, &winner.pubkey()).await, XAUT, 1);
}

/// After the expiry time the legacy layout is rejected and only the gold-aware checkpoint works.
#[tokio::test]
async fn legacy_checkpoint_expires_at_cutoff_time() {
    let admin = Keypair::new();
    let winner = Keypair::new();
    let (round, board) = resolved_round(&admin.pubkey());

    let mut ctx = start_unlocked(
        gold_env(&admin, &[])
            .fund(winner.pubkey())
            .account(board_pda().0, pod_account(&board))
            .account(round_pda(1).0, pod_account(&round))
            .account(
                miner_pda(winner.pubkey()).0,
                pod_account(&round_miner(winner.pubkey(), 1)),
            ),
    )
    .await;

    assert_custom_err(
        send(
            &mut ctx,
            &[&winner],
            &[godl_api::sdk::checkpoint(
                winner.pubkey(),
                winner.pubkey(),
                1,
            )],
        )
        .await,
        GodlError::LegacyCheckpointExpired as u32,
    );
    let m: Miner = get(&mut ctx, miner_pda(winner.pubkey()).0).await;
    assert_eq!(m.checkpoint_id, 0, "rejected before touching the miner");

    send(
        &mut ctx,
        &[&winner],
        &[godl_api::sdk::checkpoint_with_miner_extended(
            winner.pubkey(),
            winner.pubkey(),
            1,
        )],
    )
    .await
    .unwrap();
    let m: Miner = get(&mut ctx, miner_pda(winner.pubkey()).0).await;
    assert_eq!(m.checkpoint_id, 1);
    assert_eq!(m.rewards_godl, 10 * GODL);
    assert!(account_exists(&mut ctx, miner_extended_pda(winner.pubkey()).0).await);
}
