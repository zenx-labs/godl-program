//! Integration tests for the stake lifecycle instructions: `CloseStakeV2`
//! (user close of empty stakes), `ClosePhantomStakeV2` (admin sweep of
//! exploit-era non-canonical accounts), and `TopUpStakeV2` (deposits into
//! existing never-locked stakes). Runs under both `cargo test -p godl-program`
//! (native processor) and `cargo test-sbf` (BPF binary).

mod common;

use common::*;
use godl_api::prelude::*;
use solana_sdk::signature::{Keypair, Signer};
use spl_associated_token_account::get_associated_token_address;
use std::time::{SystemTime, UNIX_EPOCH};
use steel::*;

fn treasury_vault() -> Pubkey {
    get_associated_token_address(&treasury_pda().0, &MINT_ADDRESS)
}

fn user_ata(user: &Pubkey) -> Pubkey {
    get_associated_token_address(user, &MINT_ADDRESS)
}

// ---------------------------------------------------------------------------
// CloseStakeV2
// ---------------------------------------------------------------------------

#[tokio::test]
async fn close_pays_rewards_sweeps_residual_returns_rent() {
    let user = Keypair::new();
    let admin = Keypair::new();
    let rewards = 500 * GODL;
    let residual = 7 * GODL;
    let s = spec(user.pubkey(), 0, 0, 20 * SCALE, false, 1).with_rewards(rewards);
    let stake_addr = s.address();
    let vault_addr = get_associated_token_address(&stake_addr, &MINT_ADDRESS);

    // Seed residual tokens (donations/dust above the zero balance) into the
    // vault by overriding the builder-created account.
    let mut ctx = EnvBuilder::new(admin.pubkey())
        .stake(s)
        .account(vault_addr, token_account(&stake_addr, residual))
        .start()
        .await;

    let treasury_godl_before = token_balance(&mut ctx, treasury_vault()).await;
    let user_lamports_before = lamports(&mut ctx, user.pubkey()).await;

    // The sdk builder passes the treasury READONLY; success here is also the
    // regression guard that the close path stays treasury-write-free.
    send(
        &mut ctx,
        &[&user],
        &[godl_api::sdk::close_stake_v2(user.pubkey(), 0)],
    )
    .await
    .unwrap();

    // Rewards (from the treasury vault) + residual (from the stake vault) both
    // land in the signer's ATA.
    assert_eq!(
        token_balance(&mut ctx, user_ata(&user.pubkey())).await,
        rewards + residual
    );
    assert_eq!(
        treasury_godl_before - token_balance(&mut ctx, treasury_vault()).await,
        rewards
    );
    // Stake account and vault are gone; their lamports went to the signer.
    assert!(!account_exists(&mut ctx, stake_addr).await);
    assert!(!account_exists(&mut ctx, vault_addr).await);
    assert!(lamports(&mut ctx, user.pubkey()).await > user_lamports_before);
    assert_invariant(&mut ctx, &[]).await;
}

#[tokio::test]
async fn close_zero_rewards_skips_recipient_ata() {
    let user = Keypair::new();
    let admin = Keypair::new();
    let s = spec(user.pubkey(), 1, 0, SCALE, false, 1);
    let stake_addr = s.address();
    let vault_addr = get_associated_token_address(&stake_addr, &MINT_ADDRESS);

    let mut ctx = EnvBuilder::new(admin.pubkey()).stake(s).start().await;

    send(
        &mut ctx,
        &[&user],
        &[godl_api::sdk::close_stake_v2(user.pubkey(), 1)],
    )
    .await
    .unwrap();

    // Nothing to pay out, so no recipient ATA is created on the signer's dime.
    assert!(!account_exists(&mut ctx, user_ata(&user.pubkey())).await);
    assert!(!account_exists(&mut ctx, stake_addr).await);
    assert!(!account_exists(&mut ctx, vault_addr).await);
    assert_invariant(&mut ctx, &[]).await;
}

#[tokio::test]
async fn close_rejects_nonzero_balance() {
    let user = Keypair::new();
    let admin = Keypair::new();
    let s = spec(user.pubkey(), 2, 100 * GODL, 20 * SCALE, false, 1);
    let stake_addr = s.address();

    let mut ctx = EnvBuilder::new(admin.pubkey()).stake(s).start().await;

    assert_custom_err(
        send(
            &mut ctx,
            &[&user],
            &[godl_api::sdk::close_stake_v2(user.pubkey(), 2)],
        )
        .await,
        16, // GodlError::StakeNotEmpty
    );
    assert!(account_exists(&mut ctx, stake_addr).await);
    assert_invariant(&mut ctx, &[stake_addr]).await;
}

#[tokio::test]
async fn close_rejects_nft_staked() {
    let user = Keypair::new();
    let admin = Keypair::new();
    let s = spec(user.pubkey(), 3, 0, 20 * SCALE, true, 1);
    let stake_addr = s.address();

    let mut ctx = EnvBuilder::new(admin.pubkey()).stake(s).start().await;

    assert_custom_err(
        send(
            &mut ctx,
            &[&user],
            &[godl_api::sdk::close_stake_v2(user.pubkey(), 3)],
        )
        .await,
        10, // GodlError::NftAlreadyStaked
    );
    assert!(account_exists(&mut ctx, stake_addr).await);
}

#[tokio::test]
async fn close_rejects_wrong_authority_and_noncanonical() {
    let victim = Keypair::new();
    let mallory = Keypair::new();
    let admin = Keypair::new();
    let s = spec(victim.pubkey(), 4, 0, SCALE, false, 1).with_rewards(GODL);
    let odd_address = Keypair::new().pubkey();
    let s_phantom = spec(victim.pubkey(), 5, 0, SCALE, false, 1).with_rewards(GODL);

    let mut ctx = EnvBuilder::new(admin.pubkey())
        .stake(s)
        .stake_at(odd_address, s_phantom)
        .fund(mallory.pubkey())
        .start()
        .await;

    // A non-authority signer only ever derives their own (empty) PDA.
    assert!(send(
        &mut ctx,
        &[&mallory],
        &[godl_api::sdk::close_stake_v2(mallory.pubkey(), 4)],
    )
    .await
    .is_err());

    // A non-canonical (exploit-shape) address substituted into the stake slot
    // fails the has_seeds check even for the recorded authority.
    let mut ix = godl_api::sdk::close_stake_v2(victim.pubkey(), 5);
    ix.accounts[3] = AccountMeta::new(odd_address, false);
    assert!(send(&mut ctx, &[&victim], &[ix]).await.is_err());

    // Nothing was closed by the failed attempts.
    assert!(account_exists(&mut ctx, s.address()).await);
    assert!(account_exists(&mut ctx, odd_address).await);
}

#[tokio::test]
async fn close_tolerates_missing_vault_ata() {
    // Version 0 on purpose: closing must work without migrating, and a
    // drained-and-closed vault (reconcile-era shape) must read as zero.
    let user = Keypair::new();
    let admin = Keypair::new();
    let rewards = 3 * GODL;
    let s = spec(user.pubkey(), 6, 0, 20 * SCALE, false, 0)
        .with_rewards(rewards)
        .without_vault();
    let stake_addr = s.address();

    let mut ctx = EnvBuilder::new(admin.pubkey()).stake(s).start().await;

    send(
        &mut ctx,
        &[&user],
        &[godl_api::sdk::close_stake_v2(user.pubkey(), 6)],
    )
    .await
    .unwrap();

    assert_eq!(
        token_balance(&mut ctx, user_ata(&user.pubkey())).await,
        rewards
    );
    assert!(!account_exists(&mut ctx, stake_addr).await);
    assert_invariant(&mut ctx, &[]).await;
}

// ---------------------------------------------------------------------------
// ClosePhantomStakeV2
// ---------------------------------------------------------------------------

#[tokio::test]
async fn phantom_close_forfeits_rewards_rent_to_treasury() {
    let victim = Keypair::new();
    let admin = Keypair::new();
    let odd_address = Keypair::new().pubkey();
    let s = spec(victim.pubkey(), 0, 0, 20 * SCALE, false, 0).with_rewards(42 * GODL);

    let mut ctx = EnvBuilder::new(admin.pubkey())
        .stake_at(odd_address, s)
        .start()
        .await;

    let stake_lamports = lamports(&mut ctx, odd_address).await;
    let treasury_lamports_before = lamports(&mut ctx, treasury_pda().0).await;
    let treasury_godl_before = token_balance(&mut ctx, treasury_vault()).await;

    send(
        &mut ctx,
        &[&admin],
        &[godl_api::sdk::close_phantom_stake_v2(admin.pubkey(), odd_address)],
    )
    .await
    .unwrap();

    assert!(!account_exists(&mut ctx, odd_address).await);
    // All lamports swept into the treasury, tracked in its buy-bury balance.
    let treasury: Treasury = get(&mut ctx, treasury_pda().0).await;
    assert_eq!(treasury.balance, stake_lamports);
    assert_eq!(
        lamports(&mut ctx, treasury_pda().0).await - treasury_lamports_before,
        stake_lamports
    );
    // Forfeit: not a single gram of GODL moved.
    assert_eq!(
        token_balance(&mut ctx, treasury_vault()).await,
        treasury_godl_before
    );
    assert_invariant(&mut ctx, &[]).await;
}

#[tokio::test]
async fn phantom_close_rejects_canonical() {
    let user = Keypair::new();
    let admin = Keypair::new();
    let s = spec(user.pubkey(), 0, 0, SCALE, false, 1);
    let stake_addr = s.address();

    let mut ctx = EnvBuilder::new(admin.pubkey()).stake(s).start().await;

    assert_custom_err(
        send(
            &mut ctx,
            &[&admin],
            &[godl_api::sdk::close_phantom_stake_v2(admin.pubkey(), stake_addr)],
        )
        .await,
        17, // GodlError::StakeNotPhantom
    );
    assert!(account_exists(&mut ctx, stake_addr).await);
}

#[tokio::test]
async fn phantom_close_rejects_nonzero_balance() {
    let victim = Keypair::new();
    let admin = Keypair::new();
    let odd_address = Keypair::new().pubkey();
    let s = spec(victim.pubkey(), 0, 50 * GODL, 20 * SCALE, false, 0);

    let mut ctx = EnvBuilder::new(admin.pubkey())
        .stake_at(odd_address, s)
        .start()
        .await;

    assert_custom_err(
        send(
            &mut ctx,
            &[&admin],
            &[godl_api::sdk::close_phantom_stake_v2(admin.pubkey(), odd_address)],
        )
        .await,
        16, // GodlError::StakeNotEmpty — run ReconcileStakeV2 first
    );
    assert!(account_exists(&mut ctx, odd_address).await);
    assert_invariant(&mut ctx, &[odd_address]).await;
}

#[tokio::test]
async fn phantom_close_rejects_non_admin() {
    let victim = Keypair::new();
    let admin = Keypair::new();
    let intruder = Keypair::new();
    let odd_address = Keypair::new().pubkey();
    let s = spec(victim.pubkey(), 0, 0, 20 * SCALE, false, 0);

    let mut ctx = EnvBuilder::new(admin.pubkey())
        .stake_at(odd_address, s)
        .fund(intruder.pubkey())
        .start()
        .await;

    assert_custom_err(
        send(
            &mut ctx,
            &[&intruder],
            &[godl_api::sdk::close_phantom_stake_v2(intruder.pubkey(), odd_address)],
        )
        .await,
        1, // GodlError::NotAuthorized
    );
    assert!(account_exists(&mut ctx, odd_address).await);
}

#[tokio::test]
async fn phantom_close_allows_nft_flag() {
    // The flagged asset belongs to an on-curve address the program can never
    // sign for — stranded either way — so the sweep proceeds.
    let victim = Keypair::new();
    let admin = Keypair::new();
    let odd_address = Keypair::new().pubkey();
    let s = spec(victim.pubkey(), 0, 0, 20 * SCALE, true, 0).with_rewards(GODL);

    let mut ctx = EnvBuilder::new(admin.pubkey())
        .stake_at(odd_address, s)
        .start()
        .await;

    send(
        &mut ctx,
        &[&admin],
        &[godl_api::sdk::close_phantom_stake_v2(admin.pubkey(), odd_address)],
    )
    .await
    .unwrap();
    assert!(!account_exists(&mut ctx, odd_address).await);
    assert_invariant(&mut ctx, &[]).await;
}

// ---------------------------------------------------------------------------
// TopUpStakeV2
// ---------------------------------------------------------------------------

#[tokio::test]
async fn topup_happy_path_1x_delta_equals_amount() {
    let user = Keypair::new();
    let admin = Keypair::new();
    let s = spec(user.pubkey(), 5, 100 * GODL, SCALE, false, 1);
    let stake_addr = s.address();
    let vault_addr = get_associated_token_address(&stake_addr, &MINT_ADDRESS);

    let mut ctx = EnvBuilder::new(admin.pubkey())
        .stake(s)
        .token(user.pubkey(), 50 * GODL)
        .start()
        .await;
    let total_before = get::<Treasury>(&mut ctx, treasury_pda().0).await.total_staked;

    send(
        &mut ctx,
        &[&user],
        &[godl_api::sdk::top_up_stake_v2(user.pubkey(), 5, 30 * GODL)],
    )
    .await
    .unwrap();

    let stake: StakeV2 = get(&mut ctx, stake_addr).await;
    assert_eq!(stake.balance, 130 * GODL);
    // The lock clock restarts on every top-up (crafted created_at was 0).
    assert!(stake.created_at > 0);
    // At 1x / no NFT, weighted units == balance, so the delta is exact.
    let treasury: Treasury = get(&mut ctx, treasury_pda().0).await;
    assert_eq!(treasury.total_staked - total_before, 30 * GODL);
    assert_eq!(token_balance(&mut ctx, user_ata(&user.pubkey())).await, 20 * GODL);
    assert_eq!(token_balance(&mut ctx, vault_addr).await, 130 * GODL);
    assert_invariant(&mut ctx, &[stake_addr]).await;
}

#[tokio::test]
async fn topup_settles_rewards_before_balance_change() {
    let user = Keypair::new();
    let admin = Keypair::new();
    let s = spec(user.pubkey(), 6, 100 * GODL, SCALE, false, 1);
    let stake_addr = s.address();

    let mut ctx = EnvBuilder::new(admin.pubkey())
        .stake(s)
        .token(user.pubkey(), 40 * GODL)
        .rewards_factor(Numeric::from_fraction(1, 4))
        .start()
        .await;

    send(
        &mut ctx,
        &[&user],
        &[godl_api::sdk::top_up_stake_v2(user.pubkey(), 6, 40 * GODL)],
    )
    .await
    .unwrap();

    // Accrued at the PRE-top-up balance (100 / 4), not (140 / 4).
    let stake: StakeV2 = get(&mut ctx, stake_addr).await;
    assert_eq!(stake.rewards, 25 * GODL);
    assert_eq!(stake.balance, 140 * GODL);
    assert!(stake.last_deposit_at > 0);
    assert_invariant(&mut ctx, &[stake_addr]).await;
}

#[tokio::test]
async fn topup_auto_migrates_v0() {
    let user = Keypair::new();
    let admin = Keypair::new();
    let s = spec(user.pubkey(), 7, 100 * GODL, SCALE, false, 0);
    let stake_addr = s.address();

    let mut ctx = EnvBuilder::new(admin.pubkey())
        .stake(s)
        .token(user.pubkey(), 10 * GODL)
        .start()
        .await;

    send(
        &mut ctx,
        &[&user],
        &[godl_api::sdk::top_up_stake_v2(user.pubkey(), 7, 10 * GODL)],
    )
    .await
    .unwrap();

    // Migration is units-neutral at 1x (sqrt(1) == 1) but must still flip the
    // version so the account leaves the v0 population.
    let stake: StakeV2 = get(&mut ctx, stake_addr).await;
    assert_eq!(stake.weight_version, 1);
    assert_eq!(stake.balance, 110 * GODL);
    let treasury: Treasury = get(&mut ctx, treasury_pda().0).await;
    assert_eq!(treasury.total_staked, 110 * GODL);
    assert_invariant(&mut ctx, &[stake_addr]).await;
}

#[tokio::test]
async fn topup_with_nft_boost_delta() {
    let user = Keypair::new();
    let admin = Keypair::new();
    let s = spec(user.pubkey(), 8, 100 * GODL, SCALE, true, 1);
    let stake_addr = s.address();

    let mut ctx = EnvBuilder::new(admin.pubkey())
        .stake(s)
        .token(user.pubkey(), 30 * GODL)
        .start()
        .await;

    send(
        &mut ctx,
        &[&user],
        &[godl_api::sdk::top_up_stake_v2(user.pubkey(), 8, 30 * GODL)],
    )
    .await
    .unwrap();

    // 130 GODL × 11/10 boost = 143 GODL of weight.
    let treasury: Treasury = get(&mut ctx, treasury_pda().0).await;
    assert_eq!(treasury.total_staked, 143 * GODL);
    assert_invariant(&mut ctx, &[stake_addr]).await;
}

#[tokio::test]
async fn topup_into_locked_and_expired_stakes_restarts_lock() {
    let user = Keypair::new();
    let admin = Keypair::new();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    // Expired lock: created at epoch, 2-year lock long over — but still
    // carrying its 20x multiplier. Topping up re-locks it in full.
    let expired = spec(user.pubkey(), 9, 100 * GODL, 20 * SCALE, false, 1)
        .with_lock(0, MAX_LOCK_DURATION);
    // Active mid-lock account: topping up is allowed and restarts the clock.
    let active = spec(user.pubkey(), 10, 100 * GODL, 20 * SCALE, false, 1)
        .with_lock(now, MAX_LOCK_DURATION);

    let mut ctx = EnvBuilder::new(admin.pubkey())
        .stake(expired)
        .stake(active)
        .token(user.pubkey(), 30 * GODL)
        .start()
        .await;

    for id in [9u64, 10] {
        send(
            &mut ctx,
            &[&user],
            &[godl_api::sdk::top_up_stake_v2(user.pubkey(), id, 10 * GODL)],
        )
        .await
        .unwrap();
    }

    // Both accounts re-anchored to now with terms unchanged, so both are
    // locked and withdrawals fail.
    for s in [expired, active] {
        let stake: StakeV2 = get(&mut ctx, s.address()).await;
        assert_eq!(stake.balance, 110 * GODL);
        assert_eq!(stake.lock_duration, MAX_LOCK_DURATION);
        assert_eq!(stake.multiplier, 20 * SCALE);
        assert!(stake.created_at >= now);
        assert_custom_err(
            send(&mut ctx, &[&user], &[ix_withdraw_v2(user.pubkey(), s.id, GODL)]).await,
            4, // GodlError::StakeLocked — the top-up restarted the lock
        );
    }
    // Weighted delta was exact for the sqrt-weighted 20x accounts.
    let treasury: Treasury = get(&mut ctx, treasury_pda().0).await;
    assert_eq!(
        treasury.total_staked as u128,
        2 * ((110 * GODL) as u128 * SQRT20 / SCALE as u128)
    );
    assert_invariant(&mut ctx, &[expired.address(), active.address()]).await;
}

#[tokio::test]
async fn topup_rejects_zero_amount_and_wrong_authority() {
    let user = Keypair::new();
    let mallory = Keypair::new();
    let admin = Keypair::new();
    let s = spec(user.pubkey(), 12, 100 * GODL, SCALE, false, 1);
    let stake_addr = s.address();

    let mut ctx = EnvBuilder::new(admin.pubkey())
        .stake(s)
        .token(user.pubkey(), 10 * GODL)
        .fund(mallory.pubkey())
        .token(mallory.pubkey(), 10 * GODL)
        .start()
        .await;

    assert_custom_err(
        send(
            &mut ctx,
            &[&user],
            &[godl_api::sdk::top_up_stake_v2(user.pubkey(), 12, 0)],
        )
        .await,
        0, // GodlError::AmountTooSmall
    );

    // A non-authority signer only ever derives their own (empty) PDA.
    assert!(send(
        &mut ctx,
        &[&mallory],
        &[godl_api::sdk::top_up_stake_v2(mallory.pubkey(), 12, GODL)],
    )
    .await
    .is_err());

    let stake: StakeV2 = get(&mut ctx, stake_addr).await;
    assert_eq!(stake.balance, 100 * GODL);
    assert_invariant(&mut ctx, &[stake_addr]).await;
}

// ---------------------------------------------------------------------------
// MergeStakeV2
// ---------------------------------------------------------------------------

#[tokio::test]
async fn merge_longer_lock_wins_and_restarts() {
    let user = Keypair::new();
    let admin = Keypair::new();
    // Target: expired 2-year lock still carrying 20x. Source: unlocked 1x with
    // pending rewards.
    let target = spec(user.pubkey(), 0, 100 * GODL, 20 * SCALE, false, 1)
        .with_lock(0, MAX_LOCK_DURATION);
    let source = spec(user.pubkey(), 1, 50 * GODL, SCALE, false, 1).with_rewards(5 * GODL);
    let source_vault = get_associated_token_address(&source.address(), &MINT_ADDRESS);
    let target_vault = get_associated_token_address(&target.address(), &MINT_ADDRESS);

    let mut ctx = EnvBuilder::new(admin.pubkey())
        .stake(target)
        .stake(source)
        .start()
        .await;

    send(
        &mut ctx,
        &[&user],
        &[godl_api::sdk::merge_stake_v2(user.pubkey(), 0, 1)],
    )
    .await
    .unwrap();

    // Merged terms: combined balance, the longer lock restarted from now, the
    // multiplier recomputed from that lock (fresh-deposit-equivalent).
    let merged: StakeV2 = get(&mut ctx, target.address()).await;
    assert_eq!(merged.balance, 150 * GODL);
    assert_eq!(merged.lock_duration, MAX_LOCK_DURATION);
    assert_eq!(merged.multiplier, 20 * SCALE);
    assert!(merged.created_at > 0);
    assert_eq!(merged.rewards, 5 * GODL, "source rewards carried over");

    // Source account and vault are gone; all tokens sit in the target vault.
    assert!(!account_exists(&mut ctx, source.address()).await);
    assert!(!account_exists(&mut ctx, source_vault).await);
    assert_eq!(token_balance(&mut ctx, target_vault).await, 150 * GODL);

    // Weight: the whole 150 GODL at sqrt(20x) — and the restarted lock means a
    // withdrawal is now rejected.
    let treasury: Treasury = get(&mut ctx, treasury_pda().0).await;
    assert_eq!(
        treasury.total_staked as u128,
        (150 * GODL) as u128 * SQRT20 / SCALE as u128
    );
    assert_custom_err(
        send(&mut ctx, &[&user], &[ix_withdraw_v2(user.pubkey(), 0, GODL)]).await,
        4, // GodlError::StakeLocked
    );
    assert_invariant(&mut ctx, &[target.address()]).await;
}

#[tokio::test]
async fn merge_settles_rewards_at_old_weights() {
    let user = Keypair::new();
    let admin = Keypair::new();
    // Target at 1x, source at 20x (expired lock): after the merge everything
    // is 20x, but the pending factor delta must have settled at each account's
    // pre-merge weight.
    let target = spec(user.pubkey(), 0, 100 * GODL, SCALE, false, 1);
    let source = spec(user.pubkey(), 1, 100 * GODL, 20 * SCALE, false, 1)
        .with_lock(0, MAX_LOCK_DURATION);

    let mut ctx = EnvBuilder::new(admin.pubkey())
        .stake(target)
        .stake(source)
        .rewards_factor(Numeric::from_fraction(1, 4))
        .start()
        .await;

    send(
        &mut ctx,
        &[&user],
        &[godl_api::sdk::merge_stake_v2(user.pubkey(), 0, 1)],
    )
    .await
    .unwrap();

    let merged: StakeV2 = get(&mut ctx, target.address()).await;
    let source_units = (100 * GODL) as u128 * SQRT20 / SCALE as u128;
    let expected = 25 * GODL as u128 + source_units / 4;
    assert_eq!(merged.rewards as u128, expected);
    assert_eq!(merged.multiplier, 20 * SCALE);
    assert_eq!(
        get::<Treasury>(&mut ctx, treasury_pda().0).await.total_staked as u128,
        (200 * GODL) as u128 * SQRT20 / SCALE as u128
    );
    assert_invariant(&mut ctx, &[target.address()]).await;
}

#[tokio::test]
async fn merge_auto_migrates_v0_inputs_and_sweeps_dust() {
    let user = Keypair::new();
    let admin = Keypair::new();
    let target = spec(user.pubkey(), 0, 100 * GODL, SCALE, false, 0);
    let source = spec(user.pubkey(), 1, 50 * GODL, SCALE, false, 0);
    let source_vault = get_associated_token_address(&source.address(), &MINT_ADDRESS);
    let target_vault = get_associated_token_address(&target.address(), &MINT_ADDRESS);

    // Seed 3 GODL of unaccounted dust into the source vault.
    let mut ctx = EnvBuilder::new(admin.pubkey())
        .stake(target)
        .stake(source)
        .account(source_vault, token_account(&source.address(), 53 * GODL))
        .start()
        .await;

    send(
        &mut ctx,
        &[&user],
        &[godl_api::sdk::merge_stake_v2(user.pubkey(), 0, 1)],
    )
    .await
    .unwrap();

    let merged: StakeV2 = get(&mut ctx, target.address()).await;
    assert_eq!(merged.weight_version, 1, "both inputs migrated on touch");
    assert_eq!(merged.balance, 150 * GODL);
    // Dust rides along into the target vault (vault >= balance invariant).
    assert_eq!(token_balance(&mut ctx, target_vault).await, 153 * GODL);
    // Units-neutral at 1x: total is exactly the combined balance.
    assert_eq!(
        get::<Treasury>(&mut ctx, treasury_pda().0).await.total_staked,
        150 * GODL
    );
    assert_invariant(&mut ctx, &[target.address()]).await;
}

#[tokio::test]
async fn merge_rejects_nft_same_id_wrong_authority_noncanonical() {
    let user = Keypair::new();
    let mallory = Keypair::new();
    let admin = Keypair::new();
    let nft_stake = spec(user.pubkey(), 2, 10 * GODL, SCALE, true, 1);
    let plain = spec(user.pubkey(), 3, 10 * GODL, SCALE, false, 1);
    let plain_b = spec(user.pubkey(), 4, 10 * GODL, SCALE, false, 1);

    let mut ctx = EnvBuilder::new(admin.pubkey())
        .stake(nft_stake)
        .stake(plain)
        .stake(plain_b)
        .fund(mallory.pubkey())
        .start()
        .await;

    // NFT on the target, then on the source.
    assert_custom_err(
        send(
            &mut ctx,
            &[&user],
            &[godl_api::sdk::merge_stake_v2(user.pubkey(), 2, 3)],
        )
        .await,
        10, // GodlError::NftAlreadyStaked
    );
    assert_custom_err(
        send(
            &mut ctx,
            &[&user],
            &[godl_api::sdk::merge_stake_v2(user.pubkey(), 3, 2)],
        )
        .await,
        10,
    );

    // Merging a stake with itself.
    assert_custom_err(
        send(
            &mut ctx,
            &[&user],
            &[godl_api::sdk::merge_stake_v2(user.pubkey(), 3, 3)],
        )
        .await,
        18, // GodlError::MergeSameStake
    );

    // A non-authority signer only ever derives their own (empty) PDAs.
    assert!(send(
        &mut ctx,
        &[&mallory],
        &[godl_api::sdk::merge_stake_v2(mallory.pubkey(), 3, 4)],
    )
    .await
    .is_err());

    // A non-canonical address substituted into the source slot fails has_seeds.
    let mut ix = godl_api::sdk::merge_stake_v2(user.pubkey(), 3, 4);
    ix.accounts[4] = AccountMeta::new(Keypair::new().pubkey(), false);
    assert!(send(&mut ctx, &[&user], &[ix]).await.is_err());

    // Nothing was merged or closed by the failed attempts.
    for s in [nft_stake, plain, plain_b] {
        assert_eq!(get::<StakeV2>(&mut ctx, s.address()).await.balance, 10 * GODL);
    }
    assert_invariant(&mut ctx, &[nft_stake.address(), plain.address(), plain_b.address()]).await;
}
