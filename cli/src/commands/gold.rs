//! Gold rewards commands: SOL accumulated in the sol motherlode is swapped
//! into XAUt0 and distributed to unrefined GODL holders.
//!
//! Rollout order matters: `initialize-gold-vault`, then
//! `create-miner-extended-all`, then `audit-unclaimed` (and
//! `rebase-total-unclaimed` if it drifted), and only then the first `buy-gold`.

use std::collections::HashSet;

use anyhow::{bail, Context, Result};
use godl_api::prelude::*;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    instruction::Instruction, native_token::LAMPORTS_PER_SOL, pubkey::Pubkey, signature::Keypair,
    signature::Signer,
};
use spl_associated_token_account::get_associated_token_address;
use spl_token::amount_to_ui_amount;
use tokio::time::{sleep, Duration};

use super::swap::GODL_LUT;
use crate::display::{print_gold_vault, print_miner_extended};
use crate::jupiter::JupiterClient;
use crate::rpc::{
    get_board, get_clock, get_gold_vault, get_miner, get_miner_extended, get_miner_extendeds,
    get_miners, get_sol_motherlode, get_treasury,
};
use crate::transaction::{
    get_address_lookup_table_accounts, submit_transaction,
    submit_transaction_with_address_lookup_tables,
};

/// How often `buy-gold-listen` polls the sol motherlode.
const POLL_INTERVAL: Duration = Duration::from_secs(60);

/// `CreateMinerExtended` instructions per transaction.
///
/// Each instruction adds two keys the batch cannot share (the miner PDA and its extended PDA,
/// 64 bytes) plus ~9 bytes of instruction metadata; with the shared signer / gold vault /
/// system program / program id keys, signature and header (~230 bytes) a legacy transaction
/// fits about 13 before the 1232-byte packet limit. Ten matches `checkpoint_all` and leaves
/// headroom.
const CREATE_BATCH: usize = 10;

fn xaut_units(amount: f64) -> u64 {
    (amount * 10f64.powi(XAUT_DECIMALS as i32)).round() as u64
}

/// Deployer-only: create the gold vault PDA and its token accounts.
pub async fn initialize_gold_vault(rpc: &RpcClient, payer: &Keypair) -> Result<()> {
    let ix = godl_api::sdk::initialize_gold_vault(payer.pubkey());
    submit_transaction(rpc, payer, &[ix]).await?;
    println!("Gold vault initialized at {}", gold_vault_pda().0);
    Ok(())
}

/// Off-chain mirror of `MinerExtended::update_rewards` for display.
fn claimable_xaut(extended: &MinerExtended, gold_vault: &GoldVault, unrefined: u64) -> u64 {
    let mut e = *extended;
    e.update_rewards(gold_vault, unrefined);
    e.xaut_rewards
}

pub async fn gold_info(rpc: &RpcClient, authority: Option<Pubkey>) -> Result<()> {
    let gold_vault_address = gold_vault_pda().0;
    let gold_vault = get_gold_vault(rpc).await?;
    let xaut_balance = rpc
        .get_token_account_balance(&gold_vault_xaut_address())
        .await
        .map(|b| b.amount.parse::<u64>().unwrap_or(0))
        .unwrap_or(0);
    let sol_motherlode = get_sol_motherlode(rpc).await?;
    let sol_motherlode_lamports = rpc.get_balance(&sol_motherlode_pda().0).await?;
    print_gold_vault(
        &gold_vault,
        gold_vault_address,
        xaut_balance,
        &sol_motherlode,
        sol_motherlode_lamports,
    );
    let owed = gold_vault
        .total_xaut_distributed
        .saturating_sub(gold_vault.total_xaut_claimed);
    println!(
        "  outstanding (distributed - claimed): {} XAUt0, surplus in vault: {} XAUt0",
        amount_to_ui_amount(owed, XAUT_DECIMALS),
        amount_to_ui_amount(
            xaut_balance
                .saturating_sub(owed)
                .saturating_sub(gold_vault.pending_xaut),
            XAUT_DECIMALS
        )
    );

    if let Some(authority) = authority {
        let miner = get_miner(rpc, authority).await?;
        match get_miner_extended(rpc, authority).await? {
            Some(extended) => {
                let claimable = claimable_xaut(&extended, &gold_vault, miner.rewards_godl);
                print_miner_extended(
                    &extended,
                    miner_extended_pda(authority).0,
                    miner.rewards_godl,
                    claimable,
                );
            }
            None => println!(
                "Miner extended: not created yet for {} ({} GODL unrefined)",
                authority,
                amount_to_ui_amount(miner.rewards_godl, TOKEN_DECIMALS)
            ),
        }
    }
    Ok(())
}

/// One-shot creation of `MinerExtended` accounts. By default targets every
/// miner with unrefined GODL plus miners with a pending (un-checkpointed)
/// round; `--all` covers every miner account. Idempotent: existing accounts
/// are skipped.
pub async fn create_miner_extended_all(
    rpc: &RpcClient,
    payer: &Keypair,
    dry_run: bool,
    all: bool,
) -> Result<()> {
    let miners = get_miners(rpc).await?;
    let existing: HashSet<Pubkey> = get_miner_extendeds(rpc)
        .await?
        .into_iter()
        .map(|(_, e)| e.authority)
        .collect();
    let targets: Vec<Pubkey> = miners
        .iter()
        .filter(|(_, m)| all || m.rewards_godl > 0 || m.checkpoint_id < m.round_id)
        .filter(|(_, m)| !existing.contains(&m.authority))
        .map(|(_, m)| m.authority)
        .collect();
    let rent = rpc
        .get_minimum_balance_for_rent_exemption(8 + std::mem::size_of::<MinerExtended>())
        .await?;
    println!(
        "{} miner accounts, {} already extended, {} to create (~{} SOL rent, {} transactions)",
        miners.len(),
        existing.len(),
        targets.len(),
        (rent * targets.len() as u64) as f64 / LAMPORTS_PER_SOL as f64,
        (targets.len() + CREATE_BATCH - 1) / CREATE_BATCH,
    );
    if targets.is_empty() {
        return Ok(());
    }
    if dry_run {
        for authority in &targets {
            println!("  would create for {}", authority);
        }
        println!("(dry run — no transactions sent)");
        return Ok(());
    }

    let mut created = 0usize;
    let mut failed = 0usize;
    for (i, batch) in targets.chunks(CREATE_BATCH).enumerate() {
        let ixs: Vec<Instruction> = batch
            .iter()
            .map(|authority| godl_api::sdk::create_miner_extended(payer.pubkey(), *authority))
            .collect();
        match submit_transaction(rpc, payer, &ixs).await {
            Ok(_) => {
                created += batch.len();
                println!(
                    "[{}/{}] created {} extended accounts",
                    i + 1,
                    (targets.len() + CREATE_BATCH - 1) / CREATE_BATCH,
                    batch.len()
                );
            }
            Err(err) => {
                failed += batch.len();
                println!("[{}] batch failed: {err:#?}", i + 1);
            }
        }
    }
    println!("Done: {created} created, {failed} failed (re-run to retry; creation is idempotent)");
    if failed > 0 {
        bail!("{failed} extended accounts were not created");
    }
    Ok(())
}

struct UnclaimedAudit {
    total_unclaimed: u64,
    true_sum: u128,
    n_miners: usize,
    n_unrefined: usize,
    n_unrefined_without_extended: usize,
}

async fn audit(rpc: &RpcClient) -> Result<UnclaimedAudit> {
    let treasury = get_treasury(rpc).await?;
    let miners = get_miners(rpc).await?;
    let existing: HashSet<Pubkey> = get_miner_extendeds(rpc)
        .await?
        .into_iter()
        .map(|(_, e)| e.authority)
        .collect();
    let unrefined: Vec<&Miner> = miners
        .iter()
        .map(|(_, m)| m)
        .filter(|m| m.rewards_godl > 0)
        .collect();
    Ok(UnclaimedAudit {
        total_unclaimed: treasury.total_unclaimed,
        true_sum: unrefined.iter().map(|m| m.rewards_godl as u128).sum(),
        n_miners: miners.len(),
        n_unrefined: unrefined.len(),
        n_unrefined_without_extended: unrefined
            .iter()
            .filter(|m| !existing.contains(&m.authority))
            .count(),
    })
}

fn print_audit(a: &UnclaimedAudit) {
    println!("Miner accounts: {}", a.n_miners);
    println!(
        "Unrefined holders: {} ({} without a MinerExtended account)",
        a.n_unrefined, a.n_unrefined_without_extended
    );
    println!(
        "treasury.total_unclaimed: {} GODL ({} grams)",
        amount_to_ui_amount(a.total_unclaimed, TOKEN_DECIMALS),
        a.total_unclaimed
    );
    println!(
        "sum(miner.rewards_godl):  {} GODL ({} grams)",
        a.true_sum as f64 / ONE_GODL as f64,
        a.true_sum
    );
    println!(
        "drift: {} grams",
        a.total_unclaimed as i128 - a.true_sum as i128
    );
}

/// The gold factor is denominated by `treasury.total_unclaimed`, so it must
/// equal the sum of unrefined GODL over all miners before the first buy.
pub async fn audit_unclaimed(rpc: &RpcClient) -> Result<()> {
    let a = audit(rpc).await?;
    print_audit(&a);
    if a.total_unclaimed as i128 == a.true_sum as i128 {
        println!("OK: invariant holds (drift 0)");
    } else {
        bail!("invariant violated");
    }
    if a.n_unrefined_without_extended > 0 {
        bail!(
            "{} unrefined holders have no MinerExtended account — run create-miner-extended-all before buy-gold",
            a.n_unrefined_without_extended
        );
    }
    Ok(())
}

pub async fn rebase_total_unclaimed(rpc: &RpcClient, payer: &Keypair, dry_run: bool) -> Result<()> {
    const MAX_ATTEMPTS: usize = 5;
    for attempt in 1..=MAX_ATTEMPTS {
        let a = audit(rpc).await?;
        let after = get_treasury(rpc).await?.total_unclaimed;
        if a.total_unclaimed != after {
            println!(
                "total_unclaimed moved during snapshot ({} -> {}), retrying",
                a.total_unclaimed, after
            );
            continue;
        }
        print_audit(&a);
        let expected = a.total_unclaimed;
        let new_value = u64::try_from(a.true_sum).context("unrefined sum exceeds u64")?;
        if expected == new_value {
            println!("Nothing to rebase");
            return Ok(());
        }
        if dry_run {
            println!("(dry run) would CAS total_unclaimed: expected {expected} -> {new_value}");
            return Ok(());
        }
        let ix = godl_api::sdk::rebase_total_unclaimed(payer.pubkey(), expected, new_value);
        match submit_transaction(rpc, payer, &[ix]).await {
            Ok(_) => {
                println!(
                    "Rebase confirmed: total_unclaimed {} -> {} (drift was {})",
                    expected,
                    new_value,
                    expected as i128 - new_value as i128
                );
                return Ok(());
            }
            Err(err) => println!("attempt {attempt}: rebase failed: {err:#?}"),
        }
    }
    bail!("gave up after {MAX_ATTEMPTS} attempts")
}

/// Manual-input distribution: XAUt0 from the signer's ATA into the vault.
pub async fn deposit_gold(rpc: &RpcClient, payer: &Keypair, amount_xaut: f64) -> Result<()> {
    let amount = xaut_units(amount_xaut);
    if amount == 0 {
        bail!("amount too small");
    }
    let ix = godl_api::sdk::deposit_gold(payer.pubkey(), amount);
    submit_transaction(rpc, payer, &[ix]).await?;
    println!("Deposited {} XAUt0 ({} base units)", amount_xaut, amount);
    Ok(())
}

pub async fn claim_gold(rpc: &RpcClient, payer: &Keypair) -> Result<()> {
    let ix = godl_api::sdk::claim_gold(payer.pubkey());
    submit_transaction(rpc, payer, &[ix]).await?;
    let balance = rpc
        .get_token_account_balance(&get_associated_token_address(&payer.pubkey(), &XAUT_MINT))
        .await
        .map(|b| b.ui_amount_string)
        .unwrap_or_else(|_| "0".to_string());
    println!("Claimed. XAUt0 balance: {}", balance);
    Ok(())
}

/// One-shot buy: quote a SOL→XAUt0 swap via Jupiter with the gold vault as
/// taker and execute `buy_gold`.
pub async fn buy_gold(
    rpc: &RpcClient,
    payer: &Keypair,
    amount_sol: f64,
    min_out_xaut: f64,
    dexes: Option<&str>,
    jup: &JupiterClient,
) -> Result<()> {
    let amount = (amount_sol * LAMPORTS_PER_SOL as f64) as u64;
    execute_buy_gold(rpc, payer, amount, xaut_units(min_out_xaut), dexes, jup).await
}

/// Polls the sol motherlode ledger and buys gold whenever it holds at least
/// `amount_sol`.
pub async fn buy_gold_listen(
    rpc: &RpcClient,
    payer: &Keypair,
    amount_sol: f64,
    min_out_xaut: f64,
    dexes: Option<&str>,
    jup: &JupiterClient,
) -> Result<()> {
    let amount = (amount_sol * LAMPORTS_PER_SOL as f64) as u64;
    let min_out = xaut_units(min_out_xaut);
    println!("Starting buy-gold-listen...");
    println!("Sol motherlode: {}", sol_motherlode_pda().0);
    println!("Amount per buy: {} SOL", amount_sol);
    println!("Checking every {} seconds...\n", POLL_INTERVAL.as_secs());

    loop {
        match get_sol_motherlode(rpc).await {
            Ok(motherlode) => {
                println!(
                    "[{}] Sol motherlode: {} SOL (threshold: {} SOL)",
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                    motherlode.amount as f64 / LAMPORTS_PER_SOL as f64,
                    amount_sol,
                );
                if motherlode.amount >= amount {
                    println!("Threshold reached! Sending buy-gold transaction...");
                    match execute_buy_gold(rpc, payer, amount, min_out, dexes, jup).await {
                        Ok(()) => println!("✓ Buy-gold transaction successful!\n"),
                        Err(e) => println!("✗ Buy-gold transaction failed: {:#?}\n", e),
                    }
                }
            }
            Err(e) => println!("Failed to get sol motherlode: {:#?}", e),
        }
        sleep(POLL_INTERVAL).await;
    }
}

async fn execute_buy_gold(
    rpc: &RpcClient,
    payer: &Keypair,
    amount: u64,
    min_out: u64,
    dexes: Option<&str>,
    jup: &JupiterClient,
) -> Result<()> {
    if amount == 0 {
        bail!("amount too small");
    }
    let motherlode = get_sol_motherlode(rpc).await?;
    if motherlode.amount < amount {
        bail!(
            "sol motherlode holds {} SOL, requested {}",
            motherlode.amount as f64 / LAMPORTS_PER_SOL as f64,
            amount as f64 / LAMPORTS_PER_SOL as f64
        );
    }
    // The program rejects distributions until the legacy checkpoint has expired; fail early.
    let clock = get_clock(rpc).await?;
    if clock.unix_timestamp < LEGACY_CHECKPOINT_EXPIRY_TS {
        bail!(
            "gold distribution is locked until unix time {} (legacy checkpoint expiry); chain time is {}",
            LEGACY_CHECKPOINT_EXPIRY_TS,
            clock.unix_timestamp
        );
    }
    // Refuse to distribute against a broken denominator or before the bulk creation ran.
    let a = audit(rpc).await?;
    if a.total_unclaimed as i128 != a.true_sum as i128 {
        print_audit(&a);
        bail!("total_unclaimed drifted from the sum of unrefined GODL; run rebase-total-unclaimed first");
    }
    if a.n_unrefined_without_extended > 0 {
        bail!(
            "{} unrefined holders have no MinerExtended account; run create-miner-extended-all first",
            a.n_unrefined_without_extended
        );
    }
    let _ = get_board(rpc).await?;

    let gold_vault_address = gold_vault_pda().0;
    let swap = jup
        .build_sol_swap(gold_vault_address, payer.pubkey(), amount, XAUT_MINT, dexes)
        .await?;

    let mut lut_accounts = get_address_lookup_table_accounts(rpc, vec![GODL_LUT]).await?;
    lut_accounts.extend(swap.lut_accounts);

    let buy_ix = godl_api::sdk::buy_gold(
        payer.pubkey(),
        amount,
        min_out,
        &swap.swap_accounts,
        &swap.swap_data,
    );

    let mut ixs: Vec<Instruction> = Vec::with_capacity(swap.setup_ixs.len() + 2);
    ixs.extend(swap.setup_ixs);
    ixs.push(buy_ix);
    if let Some(cleanup) = swap.cleanup_ix {
        ixs.push(cleanup);
    }

    submit_transaction_with_address_lookup_tables(rpc, payer, &ixs, lut_accounts).await?;
    println!(
        "Bought gold with {} SOL from the sol motherlode",
        amount as f64 / LAMPORTS_PER_SOL as f64
    );
    Ok(())
}
