//! `bury` and `manual-bury` commands.
//!
//! `bury` swaps SOL from the GODL treasury into GODL via Jupiter and then
//! burns (or warchests) the proceeds. The Jupiter HTTP plumbing lives in
//! [`crate::jupiter`]; this module owns only the GODL-specific transaction
//! assembly.

use anyhow::Result;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    instruction::Instruction, native_token::LAMPORTS_PER_SOL, pubkey, pubkey::Pubkey,
    signature::Keypair, signature::Signer,
};
use spl_associated_token_account::get_associated_token_address;
use tokio::time::{sleep, Duration};

use crate::jupiter::JupiterClient;
use crate::transaction::{
    get_address_lookup_table_accounts, submit_transaction,
    submit_transaction_with_address_lookup_tables,
};

/// Share of each bury that goes to the warchest reserve (25%).
const CHEST_AMOUNT_BPS: u64 = 2500;
/// Share of each bury that goes to the admin fee account (5%).
const ADMIN_AMOUNT_BPS: u64 = 500;

/// GODL-managed lookup table; Jupiter doesn't include this in its response so
/// we fetch it separately and prepend to the route's LUT set.
const GODL_LUT: Pubkey = pubkey!("CWD8mcpi4QFPZfhgG46cmcytShfEMXWF2gHDjVKaYFce");

/// How often `bury-listen` polls the treasury balance.
const POLL_INTERVAL: Duration = Duration::from_secs(30);
/// How often `manual-bury-listen` polls the treasury GODL balance.
const MANUAL_POLL_INTERVAL: Duration = Duration::from_secs(60);

/// Manually bury (burn or warchest) a fixed amount of GODL from the treasury.
pub async fn manual_bury(
    rpc: &RpcClient,
    payer: &Keypair,
    amount_godl: f64,
    no_burn: bool,
) -> Result<()> {
    let amount_units = (amount_godl * godl_api::consts::ONE_GODL as f64) as u64;
    if amount_units == 0 {
        println!("Amount too small after conversion; nothing to bury.");
        return Ok(());
    }

    let ix = godl_api::sdk::bury_tokens(payer.pubkey(), amount_units, no_burn);
    submit_transaction(rpc, payer, &[ix]).await?;

    println!(
        "Submitted manual-bury for {} GODL ({} base units)",
        amount_godl, amount_units
    );
    Ok(())
}

/// One-shot bury: quote a SOL→GODL swap via Jupiter and execute the GODL
/// `pre_bury` + `bury` flow.
pub async fn bury(
    rpc: &RpcClient,
    payer: &Keypair,
    amount_sol: f64,
    jup: &JupiterClient,
    no_burn: bool,
) -> Result<()> {
    let amount = (amount_sol * LAMPORTS_PER_SOL as f64) as u64;
    execute_bury(rpc, payer, amount, jup, no_burn).await
}

/// Polls the treasury SOL balance and triggers a `bury` whenever it crosses
/// `amount + 0.1 SOL`.
pub async fn bury_listen(
    rpc: &RpcClient,
    payer: &Keypair,
    amount_sol: f64,
    jup: &JupiterClient,
    no_burn: bool,
) -> Result<()> {
    let amount = (amount_sol * LAMPORTS_PER_SOL as f64) as u64;
    let threshold = amount + (LAMPORTS_PER_SOL / 10);
    let treasury_address = godl_api::state::treasury_pda().0;

    println!("Starting bury-listen...");
    println!("Treasury address: {}", treasury_address);
    println!("Amount to bury: {} SOL", amount_sol);
    println!(
        "Threshold: {} SOL",
        threshold as f64 / LAMPORTS_PER_SOL as f64
    );
    println!(
        "Checking treasury balance every {} seconds...\n",
        POLL_INTERVAL.as_secs()
    );

    loop {
        match rpc.get_balance(&treasury_address).await {
            Ok(balance) => {
                println!(
                    "[{}] Treasury balance: {} SOL (threshold: {} SOL)",
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                    balance as f64 / LAMPORTS_PER_SOL as f64,
                    threshold as f64 / LAMPORTS_PER_SOL as f64,
                );

                if balance >= threshold {
                    println!("Balance exceeds threshold! Sending bury transaction...");
                    match execute_bury(rpc, payer, amount, jup, no_burn).await {
                        Ok(()) => println!("✓ Bury transaction successful!\n"),
                        Err(e) => println!("✗ Bury transaction failed: {:#?}\n", e),
                    }
                }
            }
            Err(e) => println!("Failed to get treasury balance: {:#?}", e),
        }

        sleep(POLL_INTERVAL).await;
    }
}

/// Polls the treasury GODL balance and triggers a `manual_bury` whenever it
/// crosses the configured threshold.
pub async fn manual_bury_listen(
    rpc: &RpcClient,
    payer: &Keypair,
    amount_godl: f64,
    no_burn: bool,
) -> Result<()> {
    let threshold_units = (amount_godl * godl_api::consts::ONE_GODL as f64) as u64;
    if threshold_units == 0 {
        println!("Threshold too small after conversion; nothing to do.");
        return Ok(());
    }

    let treasury_address = godl_api::state::treasury_pda().0;
    let treasury_godl_address =
        get_associated_token_address(&treasury_address, &godl_api::consts::MINT_ADDRESS);

    println!("Starting manual-bury-listen...");
    println!("Treasury address: {}", treasury_address);
    println!("Treasury GODL ATA: {}", treasury_godl_address);
    println!("Threshold: {} GODL", amount_godl);
    println!(
        "Checking treasury GODL balance every {} seconds...\n",
        MANUAL_POLL_INTERVAL.as_secs()
    );

    loop {
        match rpc.get_token_account_balance(&treasury_godl_address).await {
            Ok(balance) => {
                let balance_units: u64 = balance.amount.parse().unwrap_or(0);
                println!(
                    "[{}] Treasury GODL balance: {} GODL ({} units, threshold: {} units)",
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                    balance_units as f64 / godl_api::consts::ONE_GODL as f64,
                    balance_units,
                    threshold_units,
                );

                if balance_units >= threshold_units {
                    println!("Balance exceeds threshold! Sending manual-bury transaction...");
                    match execute_manual_bury(rpc, payer, threshold_units, no_burn).await {
                        Ok(()) => println!("✓ Manual-bury transaction successful!\n"),
                        Err(e) => println!("✗ Manual-bury transaction failed: {:#?}\n", e),
                    }
                }
            }
            Err(e) => println!("Failed to get treasury GODL balance: {:#?}", e),
        }

        sleep(MANUAL_POLL_INTERVAL).await;
    }
}

// --- shared bury execution ---------------------------------------------------

async fn execute_bury(
    rpc: &RpcClient,
    payer: &Keypair,
    amount_lamports: u64,
    jup: &JupiterClient,
    no_burn: bool,
) -> Result<()> {
    let chest_amount = amount_lamports * CHEST_AMOUNT_BPS / 10_000;
    let admin_amount = amount_lamports * ADMIN_AMOUNT_BPS / 10_000;
    let bury_amount = amount_lamports - chest_amount - admin_amount;

    let treasury_address = godl_api::state::treasury_pda().0;
    let swap = jup
        .build_sol_to_godl(treasury_address, payer.pubkey(), bury_amount)
        .await?;

    // GODL_LUT isn't returned by /build, so still fetch it via RPC.
    let mut lut_accounts = get_address_lookup_table_accounts(rpc, vec![GODL_LUT]).await?;
    lut_accounts.extend(swap.lut_accounts);

    let pre_bury_ix =
        godl_api::sdk::pre_bury(payer.pubkey(), bury_amount, chest_amount, admin_amount);
    let bury_ix = godl_api::sdk::bury(
        payer.pubkey(),
        &swap.swap_accounts,
        &swap.swap_data,
        no_burn,
    );

    let mut ixs: Vec<Instruction> = Vec::with_capacity(swap.setup_ixs.len() + 3);
    ixs.extend(swap.setup_ixs);
    ixs.push(pre_bury_ix);
    ixs.push(bury_ix);
    if let Some(cleanup) = swap.cleanup_ix {
        ixs.push(cleanup);
    }

    submit_transaction_with_address_lookup_tables(rpc, payer, &ixs, lut_accounts).await?;
    Ok(())
}

async fn execute_manual_bury(
    rpc: &RpcClient,
    payer: &Keypair,
    amount_units: u64,
    no_burn: bool,
) -> Result<()> {
    if amount_units == 0 {
        return Ok(());
    }
    let ix = godl_api::sdk::bury_tokens(payer.pubkey(), amount_units, no_burn);
    submit_transaction(rpc, payer, &[ix]).await?;
    Ok(())
}
