use anyhow::{anyhow, Result};
use base64::{engine::general_purpose, Engine as _};
use bincode;
use reqwest::Client;
use serde_json::json;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::rpc_client::RpcClient as BlockingRpcClient;
use solana_client::rpc_config::RpcSendTransactionConfig;
use solana_client::send_and_confirm_transactions_in_parallel::{
    send_and_confirm_transactions_in_parallel_blocking_v2 as parallel_blocking_v2,
    SendAndConfirmConfigV2,
};
use solana_sdk::{
    address_lookup_table::{state::AddressLookupTable, AddressLookupTableAccount},
    compute_budget::ComputeBudgetInstruction,
    instruction::Instruction,
    message::{v0::Message, VersionedMessage},
    pubkey,
    pubkey::Pubkey,
    signature::{Signature, Signer},
    system_instruction,
    transaction::{Transaction, TransactionError, VersionedTransaction},
};
use std::{str::FromStr, sync::Arc};

const LAMPORTS_PER_SOL: u64 = 1_000_000_000;
const JITO_TIP_LAMPORTS: u64 = 1_000_000; // 0.001 SOL
const SWQOS_TIP_LAMPORTS: u64 = 5_000; // 0.000005 SOL

const SENDER_ENDPOINT_EWR_FAST: &str = "http://ewr-sender.helius-rpc.com/fast";
const SENDER_ENDPOINT_EWR_PING: &str = "http://ewr-sender.helius-rpc.com/ping";

const TIP_ACCOUNTS: [Pubkey; 10] = [
    pubkey!("4ACfpUFoaSD9bfPdeu6DBt89gB6ENTeHBXCAi87NhDEE"),
    pubkey!("D2L6yPZ2FmmmTKPgzaMKdhu6EWZcTpLy1Vhx8uvZe7NZ"),
    pubkey!("9bnz4RShgq1hAnLnZbP8kbgBg1kEmcJBYQq3gQbmnSta"),
    pubkey!("5VY91ws6B2hMmBFRsXkoAAdsPHBJwRfBht4DXox3xkwn"),
    pubkey!("2nyhqdwKcJZR2vcqCyrYsaPVdAnFoJjiksCXJ7hfEYgD"),
    pubkey!("2q5pghRs6arqVjRvT5gfgWfWcHWmw1ZuCzphgd5KfWGJ"),
    pubkey!("wyvPkWjVZz1M8fHQnMMCDTQDbkManefNNhweYk5WkcF"),
    pubkey!("3KCKozbAaF75qEU33jtzozcJ29yJuaLJTy2jFdzUY8bT"),
    pubkey!("4vieeGHPYPG2MmyPRcYjdiDmmhN3ww7hsFNap8pVN3Ey"),
    pubkey!("4TQLFNWK8AovT1gFvda5jfw2oJeRMKEmw7aH6MGBJ3or"),
];

/// Submit a transaction via Helius Sender with required tip and priority fee
pub async fn submit_transaction_sender(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    instructions: &[Instruction],
    compute_unit_limit: Option<u32>,
    compute_unit_price: Option<u64>,
    swqos_only: bool,
) -> Result<Signature> {
    let blockhash = rpc.get_latest_blockhash().await?;

    // Add compute budget instructions (priority fee requirement)
    let mut all_instructions = vec![];
    if let Some(compute_unit_limit) = compute_unit_limit {
        all_instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(
            compute_unit_limit,
        ));
    }
    if let Some(compute_unit_price) = compute_unit_price {
        all_instructions.push(ComputeBudgetInstruction::set_compute_unit_price(
            compute_unit_price,
        ));
    }
    all_instructions.extend_from_slice(instructions);

    // Add mandatory Jito/SWQOS tip
    let tip_lamports = if swqos_only {
        SWQOS_TIP_LAMPORTS
    } else {
        JITO_TIP_LAMPORTS
    };

    let tip_account = TIP_ACCOUNTS[4];

    all_instructions.push(system_instruction::transfer(
        &payer.pubkey(),
        &tip_account,
        tip_lamports,
    ));

    let transaction = Transaction::new_signed_with_payer(
        &all_instructions,
        Some(&payer.pubkey()),
        &[payer],
        blockhash,
    );

    // Serialize and base64-encode transaction
    let tx_bytes = bincode::serialize(&transaction)?;
    let tx_base64 = general_purpose::STANDARD.encode(tx_bytes);

    // Choose endpoint based on routing mode
    let endpoint = if swqos_only {
        format!("{}?swqos_only=true", SENDER_ENDPOINT_EWR_FAST)
    } else {
        SENDER_ENDPOINT_EWR_FAST.to_string()
    };

    let client = Client::new();
    let body = json!({
        "jsonrpc": "2.0",
        "id": "godl-cli-sender",
        "method": "sendTransaction",
        "params": [
            tx_base64,
            {
                "encoding": "base64",
                "skipPreflight": true,
                "maxRetries": 0,
            }
        ],
    });

    let resp = client.post(&endpoint).json(&body).send().await?;
    let value = resp.json::<serde_json::Value>().await?;

    if let Some(error) = value.get("error") {
        return Err(anyhow!("Sender error: {}", error));
    }

    let sig_str = value
        .get("result")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing result in Sender response"))?;

    let signature = Signature::from_str(sig_str)?;
    println!(
        "Transaction submitted via Helius Sender (swqos_only={}): {}",
        swqos_only, signature
    );

    Ok(signature)
}

/// Warm up the Helius Sender connection in the ewr region
pub async fn ping_sender() -> Result<()> {
    let client = Client::new();
    let resp = client.get(SENDER_ENDPOINT_EWR_PING).send().await?;
    println!("Sender ping (ewr) status: {}", resp.status());
    Ok(())
}

/// Submit a transaction with compute budget instructions
pub async fn submit_transaction_core(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    instructions: &[Instruction],
    compute_unit_limit: Option<u32>,
    compute_unit_price: Option<u64>,
    wait_for_confirmation: bool,
) -> Result<Signature> {
    let blockhash = rpc.get_latest_blockhash().await?;
    let mut all_instructions = vec![];
    if let Some(compute_unit_limit) = compute_unit_limit {
        all_instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(
            compute_unit_limit,
        ));
    }
    if let Some(compute_unit_price) = compute_unit_price {
        all_instructions.push(ComputeBudgetInstruction::set_compute_unit_price(
            compute_unit_price,
        ));
    }

    all_instructions.extend_from_slice(instructions);
    let transaction = Transaction::new_signed_with_payer(
        &all_instructions,
        Some(&payer.pubkey()),
        &[payer],
        blockhash,
    );

    let result = if wait_for_confirmation {
        rpc.send_and_confirm_transaction(&transaction).await
    } else {
        rpc.send_transaction(&transaction).await
    };

    match result {
        Ok(signature) => {
            println!("Transaction submitted: {:?}", signature);
            Ok(signature)
        }
        Err(e) => {
            println!("Error submitting transaction: {:?}", e);
            Err(e.into())
        }
    }
}

/// Simulate a transaction built the same way as [`submit_transaction_core`].
pub async fn simulate_transaction_core(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    instructions: &[Instruction],
    compute_unit_limit: Option<u32>,
    compute_unit_price: Option<u64>,
) -> Result<()> {
    let blockhash = rpc.get_latest_blockhash().await?;
    let mut all_instructions = vec![];
    if let Some(compute_unit_limit) = compute_unit_limit {
        all_instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(
            compute_unit_limit,
        ));
    }
    if let Some(compute_unit_price) = compute_unit_price {
        all_instructions.push(ComputeBudgetInstruction::set_compute_unit_price(
            compute_unit_price,
        ));
    }

    all_instructions.extend_from_slice(instructions);
    let transaction = Transaction::new_signed_with_payer(
        &all_instructions,
        Some(&payer.pubkey()),
        &[payer],
        blockhash,
    );

    let result = rpc.simulate_transaction(&transaction).await?;

    println!("Simulation result:");
    println!("  Success: {}", result.value.err.is_none());

    if let Some(err) = &result.value.err {
        println!("  Error: {:?}", err);
    }

    if let Some(logs) = &result.value.logs {
        println!("  Logs:");
        for log in logs {
            println!("    {}", log);
        }
    }

    if let Some(units_consumed) = result.value.units_consumed {
        println!("  Compute units consumed: {}", units_consumed);
    }

    if let Some(err) = result.value.err {
        return Err(anyhow!("Simulation failed: {:?}", err));
    }

    Ok(())
}

/// Submit a transaction with compute budget instructions
pub async fn submit_transaction(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    instructions: &[Instruction],
) -> Result<Signature> {
    submit_transaction_core(rpc, payer, instructions, None, None, true).await
}

/// Submit a transaction without confirmation
pub async fn submit_transaction_no_confirm(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    instructions: &[Instruction],
) -> Result<Signature> {
    submit_transaction_core(rpc, payer, instructions, None, None, false).await
}

/// Submit a transaction with address lookup tables
pub async fn submit_transaction_with_address_lookup_tables(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    instructions: &[Instruction],
    address_lookup_table_accounts: Vec<AddressLookupTableAccount>,
) -> Result<Signature> {
    let blockhash = rpc.get_latest_blockhash().await?;

    // Build a v0 message with ALTs
    let message_v0 = Message::try_compile(
        &payer.pubkey(),
        instructions,
        &address_lookup_table_accounts,
        blockhash,
    )?;

    let versioned_message = VersionedMessage::V0(message_v0);

    let tx = VersionedTransaction::try_new(versioned_message, &[payer])?;

    // Send + confirm
    let sig = rpc.send_and_confirm_transaction(&tx).await?;
    println!("Transaction submitted: {:?}", sig);

    Ok(sig)
}

/// Get address lookup table accounts from pubkeys
#[allow(dead_code)]
pub async fn get_address_lookup_table_accounts(
    rpc_client: &RpcClient,
    addresses: Vec<Pubkey>,
) -> Result<Vec<AddressLookupTableAccount>> {
    let mut accounts = Vec::new();
    for key in addresses {
        if let Ok(account) = rpc_client.get_account(&key).await {
            if let Ok(address_lookup_table_account) = AddressLookupTable::deserialize(&account.data)
            {
                accounts.push(AddressLookupTableAccount {
                    key,
                    addresses: address_lookup_table_account.addresses.to_vec(),
                });
            }
        }
    }
    Ok(accounts)
}

/// Simulate a transaction built the same way as [`submit_transaction_sender`].
pub async fn simulate_transaction_sender(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    instructions: &[Instruction],
    compute_unit_limit: Option<u32>,
    compute_unit_price: Option<u64>,
    swqos_only: bool,
) -> Result<()> {
    let blockhash = rpc.get_latest_blockhash().await?;

    let mut all_instructions = vec![];
    if let Some(compute_unit_limit) = compute_unit_limit {
        all_instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(
            compute_unit_limit,
        ));
    }
    if let Some(compute_unit_price) = compute_unit_price {
        all_instructions.push(ComputeBudgetInstruction::set_compute_unit_price(
            compute_unit_price,
        ));
    }
    all_instructions.extend_from_slice(instructions);

    let tip_lamports = if swqos_only {
        SWQOS_TIP_LAMPORTS
    } else {
        JITO_TIP_LAMPORTS
    };
    let tip_account = TIP_ACCOUNTS[4];
    all_instructions.push(system_instruction::transfer(
        &payer.pubkey(),
        &tip_account,
        tip_lamports,
    ));

    let transaction = Transaction::new_signed_with_payer(
        &all_instructions,
        Some(&payer.pubkey()),
        &[payer],
        blockhash,
    );

    let result = rpc.simulate_transaction(&transaction).await?;

    println!("Simulation result:");
    println!("  Success: {}", result.value.err.is_none());

    if let Some(err) = &result.value.err {
        println!("  Error: {:?}", err);
    }

    if let Some(logs) = &result.value.logs {
        println!("  Logs:");
        for log in logs {
            println!("    {}", log);
        }
    }

    if let Some(units_consumed) = result.value.units_consumed {
        println!("  Compute units consumed: {}", units_consumed);
    }

    if let Some(err) = result.value.err {
        return Err(anyhow!("Simulation failed: {:?}", err));
    }

    Ok(())
}

/// Simulate a transaction and return the result
pub async fn simulate_transaction(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    instructions: &[Instruction],
) -> Result<()> {
    let blockhash = rpc.get_latest_blockhash().await?;
    let transaction = Transaction::new_signed_with_payer(
        instructions,
        Some(&payer.pubkey()),
        &[payer],
        blockhash,
    );

    let result = rpc.simulate_transaction(&transaction).await?;

    println!("Simulation result:");
    println!("  Success: {}", result.value.err.is_none());

    if let Some(err) = result.value.err {
        println!("  Error: {:?}", err);
    }

    if let Some(logs) = result.value.logs {
        println!("  Logs:");
        for log in logs {
            println!("    {}", log);
        }
    }

    if let Some(units_consumed) = result.value.units_consumed {
        println!("  Compute units consumed: {}", units_consumed);
    }

    Ok(())
}

/// Build, send, and confirm many transactions in parallel (v2).
///
/// This is a convenience wrapper around Solana's parallel sender that:
/// - accepts instruction batches (each batch becomes one legacy `Message`)
/// - uses the same RPC URL + commitment as the provided client
/// - sends/confirm in parallel using the provided payer as the only signer
pub async fn send_and_confirm_transactions_in_parallel_blocking_v2(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    instruction_batches: Vec<Vec<Instruction>>,
) -> Result<Vec<Option<TransactionError>>> {
    let blocking_rpc = Arc::new(BlockingRpcClient::new_with_commitment(
        rpc.url(),
        rpc.commitment(),
    ));

    let messages = instruction_batches
        .iter()
        .map(|ixs| solana_sdk::message::Message::new(ixs, Some(&payer.pubkey())))
        .collect::<Vec<_>>();

    let signers: [&solana_sdk::signer::keypair::Keypair; 1] = [payer];
    let config = SendAndConfirmConfigV2 {
        with_spinner: true,
        // Long same-account sweeps (reconcile, migrate) serialize on the
        // treasury write lock; queued transactions can outlive their
        // blockhash, so allow re-signing instead of failing the batch.
        resign_txs_count: Some(10),
        rpc_send_transaction_config: RpcSendTransactionConfig::default(),
    };

    let results = parallel_blocking_v2(blocking_rpc, None, &messages, &signers, config)?;

    Ok(results)
}
