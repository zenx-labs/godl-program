//! Integration tests for the sqrt stake-weight migration (§5.2 of
//! SQRT-REWRITE-PLAN.md). Runs under both `cargo test -p godl-program`
//! (native processor) and `cargo test-sbf` (BPF binary).
//!
//! Fixtures are crafted directly with `ProgramTest::add_account` rather than
//! driven through `Initialize` (which requires the mainnet deployer key).
//!
//! Coverage notes:
//! - `stake_nft`/`unstake_nft` auto-migration is exercised at the unit level
//!   (`StakeV2::migrate_weight` with `is_nft_staked = 1`) and via crafted
//!   NFT-flagged stakes here; the CPI-level path needs an mpl-core fixture and
//!   a real Core asset, which is out of scope for this harness.
//! - The positive `ExecuteOtcTrade` born-v1 test requires the OTC oracle
//!   keypair (a mainnet secret): it runs only when `OTC_ORACLE_KEYPAIR` is set
//!   and is otherwise skipped. The oracle gate itself is tested negatively.

mod common;

use common::*;
use godl_api::prelude::*;
use solana_sdk::{
    account::AccountSharedData,
    signature::{read_keypair_file, Keypair, Signer},
};
use spl_associated_token_account::get_associated_token_address;
use steel::*;

// ---------------------------------------------------------------------------
// MigrateStakeWeight
// ---------------------------------------------------------------------------

#[tokio::test]
async fn migrate_settles_at_old_weight_then_flips() {
    let user = Keypair::new();
    let admin = Keypair::new();
    let s = spec(user.pubkey(), 0, 100 * GODL, 20 * SCALE, false, 0);
    let stake_addr = s.address();
    let linear = s.units() as u128; // 2e14

    let mut ctx = EnvBuilder::new(admin.pubkey())
        .stake(s)
        .rewards_factor(Numeric::from_fraction(1, 4))
        .start()
        .await;
    let payer = ctx.payer.insecure_clone();

    // Permissionless: the random test payer signs, not the authority/admin.
    send(
        &mut ctx,
        &[&payer],
        &[godl_api::sdk::migrate_stake_weight(payer.pubkey(), stake_addr)],
    )
    .await
    .unwrap();

    let stake: StakeV2 = get(&mut ctx, stake_addr).await;
    assert_eq!(stake.weight_version, 1);
    assert_eq!(stake.multiplier, 20 * SCALE, "stored multiplier must not change");
    // Pending rewards settled at the OLD (linear) weight.
    assert_eq!(stake.rewards as u128, linear / 4);
    let treasury: Treasury = get(&mut ctx, treasury_pda().0).await;
    assert_eq!(
        treasury.total_staked as u128,
        (100 * GODL) as u128 * SQRT20 / SCALE as u128
    );
    assert_invariant(&mut ctx, &[stake_addr]).await;
}

#[tokio::test]
async fn migrate_is_idempotent_onchain() {
    let user = Keypair::new();
    let admin = Keypair::new();
    let s = spec(user.pubkey(), 0, 100 * GODL, 20 * SCALE, false, 0);
    let stake_addr = s.address();

    let mut ctx = EnvBuilder::new(admin.pubkey())
        .stake(s)
        .rewards_factor(Numeric::from_fraction(1, 4))
        .start()
        .await;
    let payer = ctx.payer.insecure_clone();

    let ix = godl_api::sdk::migrate_stake_weight(payer.pubkey(), stake_addr);
    send(&mut ctx, &[&payer], &[ix.clone()]).await.unwrap();
    let stake_before: StakeV2 = get(&mut ctx, stake_addr).await;
    let treasury_before: Treasury = get(&mut ctx, treasury_pda().0).await;

    // Second migrate (fresh blockhash) must succeed as a pure no-op.
    send(&mut ctx, &[&payer], &[ix]).await.unwrap();
    let stake_after: StakeV2 = get(&mut ctx, stake_addr).await;
    let treasury_after: Treasury = get(&mut ctx, treasury_pda().0).await;
    assert_eq!(stake_before, stake_after);
    assert_eq!(treasury_before.total_staked, treasury_after.total_staked);
}

#[tokio::test]
async fn migrate_reaches_noncanonical_stake_accounts() {
    // Exploit-era stakes live at on-curve, non-PDA addresses; the seedless
    // handler must still migrate them.
    let user = Keypair::new();
    let admin = Keypair::new();
    let odd_address = Keypair::new().pubkey();
    let s = spec(user.pubkey(), 0, 50 * GODL, 20 * SCALE, false, 0);

    let mut ctx = EnvBuilder::new(admin.pubkey())
        .stake_at(odd_address, s)
        .start()
        .await;
    let payer = ctx.payer.insecure_clone();

    send(
        &mut ctx,
        &[&payer],
        &[godl_api::sdk::migrate_stake_weight(payer.pubkey(), odd_address)],
    )
    .await
    .unwrap();
    let stake: StakeV2 = get(&mut ctx, odd_address).await;
    assert_eq!(stake.weight_version, 1);
    assert_invariant(&mut ctx, &[odd_address]).await;
}

#[tokio::test]
async fn migrate_rejects_invalid_accounts() {
    let user = Keypair::new();
    let admin = Keypair::new();
    let s = spec(user.pubkey(), 0, 100 * GODL, 20 * SCALE, false, 0);
    let stake_addr = s.address();
    // A Treasury-shaped account at a NON-canonical address.
    let fake_treasury_addr = Keypair::new().pubkey();

    let mut ctx = EnvBuilder::new(admin.pubkey())
        .stake(s)
        .account(fake_treasury_addr, pod_account(&Treasury::zeroed()))
        .start()
        .await;
    let payer = ctx.payer.insecure_clone();

    // Wrong discriminant in the stake slot (config is program-owned).
    let mut ix = godl_api::sdk::migrate_stake_weight(payer.pubkey(), config_pda().0);
    assert!(send(&mut ctx, &[&payer], &[ix]).await.is_err());

    // Non-program-owned account in the stake slot.
    ix = godl_api::sdk::migrate_stake_weight(payer.pubkey(), payer.pubkey());
    assert!(send(&mut ctx, &[&payer], &[ix]).await.is_err());

    // Treasury substituted with a copy at the wrong address (has_seeds).
    ix = godl_api::sdk::migrate_stake_weight(payer.pubkey(), stake_addr);
    ix.accounts[2] = AccountMeta::new(fake_treasury_addr, false);
    assert!(send(&mut ctx, &[&payer], &[ix]).await.is_err());

    // Nothing was mutated by the failed attempts.
    let stake: StakeV2 = get(&mut ctx, stake_addr).await;
    assert_eq!(stake.weight_version, 0);
    assert_invariant(&mut ctx, &[stake_addr]).await;
}

// ---------------------------------------------------------------------------
// Auto-migrate on touch
// ---------------------------------------------------------------------------

#[tokio::test]
async fn withdraw_auto_migrates_and_matches_crank_then_withdraw() {
    let user = Keypair::new();
    let admin = Keypair::new();
    let s = spec(user.pubkey(), 3, 100 * GODL, 20 * SCALE, false, 0);
    let stake_addr = s.address();
    let factor = Numeric::from_fraction(1, 4);

    // Env A: withdraw directly (auto-migrate inside the handler).
    let mut a = EnvBuilder::new(admin.pubkey())
        .stake(s)
        .rewards_factor(factor)
        .start()
        .await;
    send(&mut a, &[&user], &[ix_withdraw_v2(user.pubkey(), 3, 40 * GODL)])
        .await
        .unwrap();
    let stake_a: StakeV2 = get(&mut a, stake_addr).await;
    let treasury_a: Treasury = get(&mut a, treasury_pda().0).await;

    assert_eq!(stake_a.weight_version, 1);
    assert_eq!(stake_a.balance, 60 * GODL);
    // Settled at linear weight during the auto-migrate.
    assert_eq!(stake_a.rewards as u128, (100 * GODL) as u128 * 20 / 4);
    assert_eq!(
        treasury_a.total_staked as u128,
        (60 * GODL) as u128 * SQRT20 / SCALE as u128
    );
    assert_eq!(
        token_balance(&mut a, get_associated_token_address(&user.pubkey(), &MINT_ADDRESS)).await,
        40 * GODL
    );
    assert_invariant(&mut a, &[stake_addr]).await;

    // Env B: crank first, then withdraw — must land in the identical state.
    let mut b = EnvBuilder::new(admin.pubkey())
        .stake(s)
        .rewards_factor(factor)
        .start()
        .await;
    let payer_b = b.payer.insecure_clone();
    send(
        &mut b,
        &[&payer_b],
        &[godl_api::sdk::migrate_stake_weight(payer_b.pubkey(), stake_addr)],
    )
    .await
    .unwrap();
    send(&mut b, &[&user], &[ix_withdraw_v2(user.pubkey(), 3, 40 * GODL)])
        .await
        .unwrap();
    let stake_b: StakeV2 = get(&mut b, stake_addr).await;
    let treasury_b: Treasury = get(&mut b, treasury_pda().0).await;

    assert_eq!(stake_a.balance, stake_b.balance);
    assert_eq!(stake_a.weight_version, stake_b.weight_version);
    assert_eq!(stake_a.rewards, stake_b.rewards);
    assert_eq!(treasury_a.total_staked, treasury_b.total_staked);
}

#[tokio::test]
async fn compound_auto_migrates_and_compounds_linear_settled_rewards() {
    let user = Keypair::new();
    let admin = Keypair::new();
    let s = spec(user.pubkey(), 1, 100 * GODL, 20 * SCALE, false, 0);
    let stake_addr = s.address();

    let mut ctx = EnvBuilder::new(admin.pubkey())
        .stake(s)
        .rewards_factor(Numeric::from_fraction(1, 4))
        .start()
        .await;

    // Pending at linear weight: 2e14 / 4 = 500 GODL. 1% compound fee.
    let pending = (100 * GODL) * 20 / 4;
    let fee = pending / 100;
    send(
        &mut ctx,
        &[&user],
        &[godl_api::sdk::compound_yield_v2(user.pubkey(), user.pubkey(), 1)],
    )
    .await
    .unwrap();

    let stake: StakeV2 = get(&mut ctx, stake_addr).await;
    assert_eq!(stake.weight_version, 1);
    assert_eq!(stake.rewards, 0);
    assert_eq!(stake.balance, 100 * GODL + pending - fee);
    assert_eq!(
        token_balance(&mut ctx, get_associated_token_address(&stake_addr, &MINT_ADDRESS)).await,
        stake.balance
    );
    assert_invariant(&mut ctx, &[stake_addr]).await;
}

// ---------------------------------------------------------------------------
// Born version 1
// ---------------------------------------------------------------------------

#[tokio::test]
async fn deposit_v2_creates_version_1_stake() {
    let user = Keypair::new();
    let admin = Keypair::new();
    let mut ctx = EnvBuilder::new(admin.pubkey())
        .fund(user.pubkey())
        .token(user.pubkey(), 100 * GODL)
        .start()
        .await;

    let total_before = get::<Treasury>(&mut ctx, treasury_pda().0).await.total_staked;
    send(
        &mut ctx,
        &[&user],
        &[ix_deposit_v2(user.pubkey(), 7, 100 * GODL, MAX_LOCK_DURATION)],
    )
    .await
    .unwrap();

    let stake_addr = stake_v2_pda(user.pubkey(), 7).0;
    let stake: StakeV2 = get(&mut ctx, stake_addr).await;
    assert_eq!(stake.weight_version, 1, "new deposits must be born sqrt");
    assert_eq!(stake.multiplier, 20 * SCALE, "displayed multiplier stays linear-scaled");
    let treasury: Treasury = get(&mut ctx, treasury_pda().0).await;
    assert_eq!(
        (treasury.total_staked - total_before) as u128,
        (100 * GODL) as u128 * SQRT20 / SCALE as u128,
        "born-v1 weight must enter total_staked on the sqrt curve"
    );
    assert_invariant(&mut ctx, &[stake_addr]).await;
}

#[tokio::test]
async fn execute_otc_trade_gate_holds_for_fake_oracle() {
    let buyer = Keypair::new();
    let fake_oracle = Keypair::new();
    let admin = Keypair::new();
    let mut ctx = EnvBuilder::new(admin.pubkey())
        .fund(buyer.pubkey())
        .fund(fake_oracle.pubkey())
        .start()
        .await;

    let mut ix = godl_api::sdk::execute_otc_trade(
        buyer.pubkey(),
        0,
        1_000_000,
        10 * GODL,
        0,
        u64::MAX,
        ONE_MONTH,
    );
    // Swap the hardcoded oracle signer for an impostor.
    ix.accounts[1] = AccountMeta::new_readonly(fake_oracle.pubkey(), true);
    let result = send(&mut ctx, &[&buyer, &fake_oracle], &[ix]).await;
    assert!(result.is_err(), "non-oracle signer must be rejected");
}

/// Positive born-v1 coverage for the OTC creation site. Requires the real OTC
/// oracle keypair, so it only runs when `OTC_ORACLE_KEYPAIR` points at it;
/// otherwise the test is skipped (the surfpool rehearsal covers this path
/// against mainnet state).
#[tokio::test]
async fn execute_otc_trade_creates_version_1_stake() {
    let Ok(path) = std::env::var("OTC_ORACLE_KEYPAIR") else {
        eprintln!("skipped: set OTC_ORACLE_KEYPAIR to run the OTC born-v1 test");
        return;
    };
    let oracle = read_keypair_file(&path).expect("failed to read OTC_ORACLE_KEYPAIR");
    assert_eq!(oracle.pubkey(), OTC_ORACLE_SIGNER, "keypair is not the OTC oracle");

    let buyer = Keypair::new();
    let admin = Keypair::new();
    let otc_treasury_addr = otc_treasury_pda().0;
    let mut otc_treasury = OtcTreasury::zeroed();
    otc_treasury.godl_balance = 10_000 * GODL;

    let mut ctx = EnvBuilder::new(admin.pubkey())
        .fund(buyer.pubkey())
        .account(otc_treasury_addr, pod_account(&otc_treasury))
        .account(
            get_associated_token_address(&otc_treasury_addr, &MINT_ADDRESS),
            token_account(&otc_treasury_addr, 10_000 * GODL),
        )
        .start()
        .await;

    let total_before = get::<Treasury>(&mut ctx, treasury_pda().0).await.total_staked;
    let godl_out = 500 * GODL;
    let ix = godl_api::sdk::execute_otc_trade(
        buyer.pubkey(),
        0,
        1_000_000_000,
        godl_out,
        GODL,
        u64::MAX,
        ONE_MONTH,
    );
    send(&mut ctx, &[&buyer, &oracle], &[ix]).await.unwrap();

    let stake_addr = stake_v2_pda(buyer.pubkey(), 0).0;
    let stake: StakeV2 = get(&mut ctx, stake_addr).await;
    assert_eq!(stake.weight_version, 1, "OTC stakes must be born sqrt");
    let expected = spec(buyer.pubkey(), 0, godl_out, stake.multiplier, false, 1).units();
    let treasury: Treasury = get(&mut ctx, treasury_pda().0).await;
    assert_eq!(treasury.total_staked - total_before, expected);
    assert_invariant(&mut ctx, &[stake_addr]).await;
}

// ---------------------------------------------------------------------------
// Claim path stays treasury-write-free (§2.3 regression guard)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn claim_with_readonly_treasury_succeeds_and_keeps_v0() {
    let user = Keypair::new();
    let admin = Keypair::new();
    let s = spec(user.pubkey(), 2, 100 * GODL, 20 * SCALE, false, 0);
    let stake_addr = s.address();

    let mut ctx = EnvBuilder::new(admin.pubkey())
        .stake(s)
        .rewards_factor(Numeric::from_fraction(1, 4))
        .start()
        .await;

    let treasury_bytes_before = ctx
        .banks_client
        .get_account(treasury_pda().0)
        .await
        .unwrap()
        .unwrap()
        .data;

    // Treasury passed READONLY: if any code path (e.g. a future auto-migrate)
    // writes treasury data during claim, this transaction fails.
    send(
        &mut ctx,
        &[&user],
        &[ix_claim_yield_v2(user.pubkey(), 2, u64::MAX, false)],
    )
    .await
    .expect("claim with readonly treasury must keep working for live clients");

    let stake: StakeV2 = get(&mut ctx, stake_addr).await;
    assert_eq!(stake.weight_version, 0, "claim must NOT auto-migrate");
    // Settled at the account's own (linear) weight.
    let linear_pending = (100 * GODL) * 20 / 4;
    assert_eq!(
        token_balance(&mut ctx, get_associated_token_address(&user.pubkey(), &MINT_ADDRESS)).await,
        linear_pending
    );
    let treasury_bytes_after = ctx
        .banks_client
        .get_account(treasury_pda().0)
        .await
        .unwrap()
        .unwrap()
        .data;
    assert_eq!(treasury_bytes_before, treasury_bytes_after);
    assert_invariant(&mut ctx, &[stake_addr]).await;
}

// ---------------------------------------------------------------------------
// RebaseTotalStaked
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rebase_cas_success_mismatch_and_admin_gate() {
    let user = Keypair::new();
    let admin = Keypair::new();
    let intruder = Keypair::new();
    let s = spec(user.pubkey(), 0, 100 * GODL, 20 * SCALE, false, 1);
    let stake_addr = s.address();

    let mut ctx = EnvBuilder::new(admin.pubkey())
        .stake(s)
        .fund(intruder.pubkey())
        .start()
        .await;
    let true_sum = s.units();

    // Non-admin signer is rejected.
    assert_custom_err(
        send(
            &mut ctx,
            &[&intruder],
            &[godl_api::sdk::rebase_total_staked(intruder.pubkey(), true_sum, true_sum)],
        )
        .await,
        1, // GodlError::NotAuthorized
    );

    // Mismatched expected value fails and mutates nothing.
    assert_custom_err(
        send(
            &mut ctx,
            &[&admin],
            &[godl_api::sdk::rebase_total_staked(admin.pubkey(), true_sum + 999, 0)],
        )
        .await,
        15, // GodlError::RebaseMismatch
    );
    assert_eq!(
        get::<Treasury>(&mut ctx, treasury_pda().0).await.total_staked,
        true_sum
    );

    // Backstop no-op CAS (expected == new_value) succeeds.
    send(
        &mut ctx,
        &[&admin],
        &[godl_api::sdk::rebase_total_staked(admin.pubkey(), true_sum, true_sum)],
    )
    .await
    .unwrap();

    // Repair path: corrupt total_staked out-of-band, then CAS it back.
    let mut treasury: Treasury = get(&mut ctx, treasury_pda().0).await;
    treasury.total_staked = true_sum + 5;
    let mut acc = ctx
        .banks_client
        .get_account(treasury_pda().0)
        .await
        .unwrap()
        .unwrap();
    acc.data[8..].copy_from_slice(bytemuck::bytes_of(&treasury));
    ctx.set_account(&treasury_pda().0, &AccountSharedData::from(acc));

    send(
        &mut ctx,
        &[&admin],
        &[godl_api::sdk::rebase_total_staked(admin.pubkey(), true_sum + 5, true_sum)],
    )
    .await
    .unwrap();
    assert_invariant(&mut ctx, &[stake_addr]).await;
}

// ---------------------------------------------------------------------------
// Mixed population + reward conservation through the real bury path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mixed_population_invariant_across_op_storm() {
    let (ua, ub, uc, ud) = (Keypair::new(), Keypair::new(), Keypair::new(), Keypair::new());
    let admin = Keypair::new();
    let sa = spec(ua.pubkey(), 0, 100 * GODL, 20 * SCALE, false, 0);
    let sb = spec(ub.pubkey(), 0, 200 * GODL, 3_340_000_000, true, 0);
    let sc = spec(uc.pubkey(), 0, 50 * GODL, SCALE, false, 0);
    let mut addrs = vec![sa.address(), sb.address(), sc.address()];

    let mut ctx = EnvBuilder::new(admin.pubkey())
        .stake(sa)
        .stake(sb)
        .stake(sc)
        .fund(ud.pubkey())
        .token(ud.pubkey(), 300 * GODL)
        .rewards_factor(Numeric::from_fraction(1, 8))
        .start()
        .await;
    let payer = ctx.payer.insecure_clone();
    assert_invariant(&mut ctx, &addrs).await;

    // Crank one account.
    send(
        &mut ctx,
        &[&payer],
        &[godl_api::sdk::migrate_stake_weight(payer.pubkey(), sa.address())],
    )
    .await
    .unwrap();
    assert_invariant(&mut ctx, &addrs).await;

    // Withdraw from the NFT-boosted v0 account (auto-migrates; boost after curve).
    send(&mut ctx, &[&ub], &[ix_withdraw_v2(ub.pubkey(), 0, 80 * GODL)])
        .await
        .unwrap();
    assert_eq!(get::<StakeV2>(&mut ctx, sb.address()).await.weight_version, 1);
    assert_invariant(&mut ctx, &addrs).await;

    // Claim from a v0 account (stays v0, weight untouched).
    send(&mut ctx, &[&uc], &[ix_claim_yield_v2(uc.pubkey(), 0, u64::MAX, true)])
        .await
        .unwrap();
    assert_eq!(get::<StakeV2>(&mut ctx, sc.address()).await.weight_version, 0);
    assert_invariant(&mut ctx, &addrs).await;

    // Fresh deposit is born v1.
    send(
        &mut ctx,
        &[&ud],
        &[ix_deposit_v2(ud.pubkey(), 1, 300 * GODL, MAX_LOCK_DURATION)],
    )
    .await
    .unwrap();
    addrs.push(stake_v2_pda(ud.pubkey(), 1).0);
    assert_eq!(
        get::<StakeV2>(&mut ctx, stake_v2_pda(ud.pubkey(), 1).0).await.weight_version,
        1
    );
    assert_invariant(&mut ctx, &addrs).await;

    // A real bury distribution over the mixed population.
    send(
        &mut ctx,
        &[&admin],
        &[godl_api::sdk::bury_tokens(admin.pubkey(), 1_000 * GODL, false)],
    )
    .await
    .unwrap();
    assert_invariant(&mut ctx, &addrs).await;

    // NFT unstake auto-migration is covered at the unit level; here we at
    // least migrate the remaining v0 account and confirm the terminal state.
    send(
        &mut ctx,
        &[&payer],
        &[godl_api::sdk::migrate_stake_weight(payer.pubkey(), sc.address())],
    )
    .await
    .unwrap();
    for addr in &addrs {
        assert_eq!(get::<StakeV2>(&mut ctx, *addr).await.weight_version, 1);
    }
    assert_invariant(&mut ctx, &addrs).await;
}

#[tokio::test]
async fn reward_conservation_through_bury_and_claims() {
    let (ua, ub, uc) = (Keypair::new(), Keypair::new(), Keypair::new());
    let admin = Keypair::new();
    let specs = [
        spec(ua.pubkey(), 0, 100 * GODL, 20 * SCALE, false, 0),
        spec(ub.pubkey(), 0, 200 * GODL, 20 * SCALE, false, 1),
        spec(uc.pubkey(), 0, 50 * GODL, SCALE, true, 1),
    ];
    let addrs: Vec<Pubkey> = specs.iter().map(|s| s.address()).collect();

    let mut ctx = EnvBuilder::new(admin.pubkey())
        .stake(specs[0])
        .stake(specs[1])
        .stake(specs[2])
        .start()
        .await;

    // Three distribution cycles through the REAL bury path (STAKERS_BPS = 3%).
    const ROUNDS: u64 = 3;
    const BURY: u64 = 1_000 * GODL;
    for _ in 0..ROUNDS {
        send(
            &mut ctx,
            &[&admin],
            &[godl_api::sdk::bury_tokens(admin.pubkey(), BURY, false)],
        )
        .await
        .unwrap();
        assert_invariant(&mut ctx, &addrs).await;
    }
    let funded = ROUNDS * (BURY * STAKERS_BPS / DENOMINATOR_BPS); // 90 GODL

    // Everyone claims everything.
    let mut claimed_total: u64 = 0;
    for (user, s) in [(&ua, specs[0]), (&ub, specs[1]), (&uc, specs[2])] {
        send(
            &mut ctx,
            &[user],
            &[ix_claim_yield_v2(user.pubkey(), s.id, u64::MAX, true)],
        )
        .await
        .unwrap();
        claimed_total += token_balance(
            &mut ctx,
            get_associated_token_address(&user.pubkey(), &MINT_ADDRESS),
        )
        .await;
    }

    // Conservation: never over-pay; residual dust is Numeric floor rounding only.
    assert!(claimed_total <= funded, "over-distribution: {claimed_total} > {funded}");
    assert!(
        funded - claimed_total <= 1_000,
        "dust too large: {} units",
        funded - claimed_total
    );
    assert_invariant(&mut ctx, &addrs).await;
}
