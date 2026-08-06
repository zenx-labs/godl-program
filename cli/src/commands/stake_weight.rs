use anyhow::{bail, Context, Result};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, signature::Signer};

use godl_api::state::{Stake, StakeV2};

use crate::rpc::{get_program_accounts, get_treasury};
use crate::transaction::submit_transaction;

/// True weight sums recomputed off-chain with the exact on-chain math.
struct WeightAudit {
    total_staked: u64,
    /// Per-account by stored weight version (the invariant target), incl. v1.
    true_sum: u128,
    /// Hypothetical: every v2 account at linear weight, incl. v1.
    all_linear: u128,
    /// Hypothetical: every v2 account at sqrt weight, incl. v1.
    all_sqrt: u128,
    n_v0: usize,
    n_v1: usize,
    n_legacy: usize,
}

/// Weighted units of a StakeV2 as if it were at `version`.
fn weight_as(stake: &StakeV2, version: u8) -> u128 {
    let mut s = *stake;
    s.weight_version = version;
    s.weighted_units().expect("stake weight overflow") as u128
}

async fn audit(rpc: &RpcClient) -> Result<WeightAudit> {
    let treasury = get_treasury(rpc).await?;
    let stakes: Vec<(Pubkey, StakeV2)> =
        get_program_accounts::<StakeV2>(rpc, godl_api::ID, vec![]).await?;
    let legacy: Vec<(Pubkey, Stake)> =
        get_program_accounts::<Stake>(rpc, godl_api::ID, vec![]).await?;

    // Legacy v1 stakes have weight = balance (already the sqrt fixpoint).
    let legacy_balance: u128 = legacy.iter().map(|(_, s)| s.balance as u128).sum();

    let mut true_sum = legacy_balance;
    let mut all_linear = legacy_balance;
    let mut all_sqrt = legacy_balance;
    let mut n_v0 = 0usize;
    let mut n_v1 = 0usize;
    for (_, s) in &stakes {
        true_sum += s.weighted_units().expect("stake weight overflow") as u128;
        all_linear += weight_as(s, 0);
        all_sqrt += weight_as(s, 1);
        if s.weight_version == 0 {
            n_v0 += 1;
        } else {
            n_v1 += 1;
        }
    }

    Ok(WeightAudit {
        total_staked: treasury.total_staked,
        true_sum,
        all_linear,
        all_sqrt,
        n_v0,
        n_v1,
        n_legacy: legacy.len(),
    })
}

fn ui(units: u128) -> f64 {
    units as f64 / 10f64.powi(godl_api::consts::TOKEN_DECIMALS as i32)
}

fn print_audit(a: &WeightAudit) {
    let drift = a.total_staked as i128 - a.true_sum as i128;
    println!("treasury.total_staked      : {:>26} ({:.2} GODL-weight)", a.total_staked, ui(a.total_staked as u128));
    println!("Σ true weight (by version) : {:>26} ({:.2} GODL-weight)", a.true_sum, ui(a.true_sum));
    println!("drift (treasury - true)    : {:>26}", drift);
    println!("Σ if all v2 linear         : {:>26}", a.all_linear);
    println!("Σ if all v2 sqrt           : {:>26}  <- rebase target once fully migrated", a.all_sqrt);
    println!("v2 weight versions         : v0={} v1={}  (+{} legacy v1 stakes)", a.n_v0, a.n_v1, a.n_legacy);
}

/// Read-only audit: recompute the per-account weight sum and compare it to
/// treasury.total_staked. Used pre/post rollout and by smoke tests.
pub async fn verify_stake_weights(rpc: &RpcClient) -> Result<()> {
    let a = audit(rpc).await?;
    print_audit(&a);
    let drift = a.total_staked as i128 - a.true_sum as i128;
    if drift == 0 {
        println!("OK: invariant holds (drift 0)");
        Ok(())
    } else {
        bail!("invariant violated: drift {drift}");
    }
}

/// Permissionless sweep: migrate every weight-version-0 StakeV2 account to the
/// sqrt curve. Every instruction writes the treasury, so transactions serialize
/// on its write lock — expect a sequential sweep. Failed batches are simply
/// retried on the next pass (migration is idempotent).
pub async fn migrate_stakes(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    dry_run: bool,
) -> Result<()> {
    const CHUNK: usize = 10;
    const MAX_PASSES: usize = 10;

    let mut pass = 0usize;
    loop {
        let stakes: Vec<(Pubkey, StakeV2)> =
            get_program_accounts::<StakeV2>(rpc, godl_api::ID, vec![]).await?;
        let v0: Vec<Pubkey> = stakes
            .iter()
            .filter(|(_, s)| s.weight_version == 0)
            .map(|(addr, _)| *addr)
            .collect();
        println!(
            "Pass {}: {} StakeV2 accounts, {} still at weight version 0",
            pass + 1,
            stakes.len(),
            v0.len()
        );
        if v0.is_empty() {
            break;
        }
        if dry_run {
            println!("(dry run — no transactions sent)");
            return Ok(());
        }
        pass += 1;
        if pass > MAX_PASSES {
            bail!("{} accounts still unmigrated after {MAX_PASSES} passes", v0.len());
        }

        let batches: Vec<Vec<solana_sdk::instruction::Instruction>> = v0
            .chunks(CHUNK)
            .map(|chunk| {
                chunk
                    .iter()
                    .map(|addr| godl_api::sdk::migrate_stake_weight(payer.pubkey(), *addr))
                    .collect()
            })
            .collect();
        // A transport-level error mid-sweep must not abort the crank: whatever
        // landed stays migrated (idempotent), the rest retries next pass.
        match crate::transaction::send_and_confirm_transactions_in_parallel_blocking_v2(
            rpc, payer, batches,
        )
        .await
        {
            Ok(results) => {
                let failed = results.iter().filter(|r| r.is_some()).count();
                println!(
                    "  submitted {} tx(s), {} failed (failures retry next pass)",
                    results.len(),
                    failed
                );
            }
            Err(err) => println!("  batch submission error ({err}); retrying next pass"),
        }
    }

    println!("Sweep complete — final invariant check:");
    verify_stake_weights(rpc).await
}

/// Admin verification backstop: compare-and-swap treasury.total_staked to the
/// recomputed true sum. Expected to be a no-op write (drift 0) after a full
/// migrate sweep; a mismatched CAS fails on-chain and is recomputed + retried
/// here.
pub async fn rebase_total_staked(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    dry_run: bool,
) -> Result<()> {
    const MAX_ATTEMPTS: usize = 5;

    for attempt in 1..=MAX_ATTEMPTS {
        // Snapshot hygiene: total_staked must be identical before and after
        // the getProgramAccounts sweep, else stake traffic landed mid-snapshot
        // and the recomputed sum may mix states.
        let a = audit(rpc).await?;
        let after = get_treasury(rpc).await?.total_staked;
        if a.total_staked != after {
            println!(
                "total_staked moved during snapshot ({} -> {}), retrying",
                a.total_staked, after
            );
            continue;
        }

        print_audit(&a);
        let expected = a.total_staked;
        let new_value = u64::try_from(a.true_sum).context("true weight sum exceeds u64")?;

        if dry_run {
            println!("(dry run) would CAS total_staked: expected {expected} -> {new_value}");
            return Ok(());
        }
        if a.n_v0 > 0 {
            bail!(
                "{} v2 accounts still at weight version 0 — run migrate-stakes to completion first",
                a.n_v0
            );
        }

        let ix = godl_api::sdk::rebase_total_staked(payer.pubkey(), expected, new_value);
        match submit_transaction(rpc, payer, &[ix]).await {
            Ok(_) => {
                println!(
                    "Rebase confirmed: total_staked {} -> {} (drift was {})",
                    expected,
                    new_value,
                    expected as i128 - new_value as i128
                );
                return Ok(());
            }
            Err(err) => {
                println!("Attempt {attempt}/{MAX_ATTEMPTS} failed ({err}); recomputing");
            }
        }
    }
    bail!("rebase failed after {MAX_ATTEMPTS} attempts (CAS kept losing races?)");
}
