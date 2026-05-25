use anyhow::{anyhow, Result};
use godl_api::{consts::REFERRER_LOCKED_SENTINEL, prelude::*};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, signature::Signer};
use steel::Instruction;

use crate::{
    rpc::{get_board, get_clock, get_miner, get_miners, get_round, get_rounds},
    transaction::{send_and_confirm_transactions_in_parallel_blocking_v2, submit_transaction},
};

/// Claim rewards (SOL and GODL)
pub async fn claim(rpc: &RpcClient, payer: &solana_sdk::signer::keypair::Keypair) -> Result<()> {
    let referrer = match get_miner(rpc, payer.pubkey()).await {
        Ok(miner)
            if miner.referrer != Pubkey::default()
                && miner.referrer != REFERRER_LOCKED_SENTINEL =>
        {
            Some(miner.referrer)
        }
        Ok(_) => None,
        Err(err) => {
            println!("Warning: unable to load miner account ({err})");
            None
        }
    };
    let ix_sol = godl_api::sdk::claim_sol_with_referrer(payer.pubkey(), referrer);
    let ix_godl = godl_api::sdk::claim_godl_with_referrer(payer.pubkey(), referrer);
    submit_transaction(rpc, payer, &[ix_sol, ix_godl]).await?;
    Ok(())
}

/// Checkpoint a single miner
pub async fn checkpoint(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    authority: Option<Pubkey>,
) -> Result<()> {
    let authority = authority.unwrap_or_else(|| payer.pubkey());
    let miner = get_miner(rpc, authority).await?;
    let ix = godl_api::sdk::checkpoint(payer.pubkey(), authority, miner.round_id);
    submit_transaction(rpc, payer, &[ix]).await?;
    Ok(())
}

/// Checkpoint all eligible miners
pub async fn checkpoint_all(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
) -> Result<()> {
    use std::collections::HashMap;

    let clock = get_clock(rpc).await?;
    let miners = get_miners(rpc).await?;
    let mut expiry_slots = HashMap::new();
    let mut ixs = vec![];

    for (i, (_address, miner)) in miners.iter().enumerate() {
        if miner.checkpoint_id < miner.round_id {
            // Log the expiry slot for the round.
            if !expiry_slots.contains_key(&miner.round_id) {
                if let Ok(round) = get_round(rpc, miner.round_id).await {
                    expiry_slots.insert(miner.round_id, round.expires_at);
                }
            }

            // Get the expiry slot for the round.
            let Some(expires_at) = expiry_slots.get(&miner.round_id) else {
                continue;
            };

            // If we are in fee collection period, checkpoint the miner.
            let fee_start_slot = expires_at.saturating_sub(TWELVE_HOURS_SLOTS);
            if clock.slot >= fee_start_slot {
                let seconds_remaining = expires_at.saturating_sub(clock.slot);
                println!(
                    "[{}/{}] Checkpoint miner: {} ({} s)",
                    i + 1,
                    miners.len(),
                    miner.authority,
                    seconds_remaining as f64 * 0.4
                );
                ixs.push(godl_api::sdk::checkpoint(
                    payer.pubkey(),
                    miner.authority,
                    miner.round_id,
                ));
            }
        }
    }

    // Batch and submit the instructions.
    while !ixs.is_empty() {
        let batch = ixs
            .drain(..std::cmp::min(10, ixs.len()))
            .collect::<Vec<Instruction>>();
        submit_transaction(rpc, payer, &batch).await?;
    }

    Ok(())
}

/// Checkpoint all eligible miners using checkpoint
pub async fn checkpoint_rounds(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
) -> Result<()> {
    use std::collections::HashMap;

    let clock = get_clock(rpc).await?;
    let miners = get_miners(rpc).await?;
    let rounds = get_rounds(rpc).await?;
    let mut expiry_slots = HashMap::new();
    let mut ixs = vec![];

    for (_address, round) in rounds.iter() {
        expiry_slots.insert(round.id, round.expires_at);
    }

    println!("Fetched {} miners", miners.len());

    for (i, (_address, miner)) in miners.iter().enumerate() {
        if miner.checkpoint_id >= miner.round_id {
            continue;
        }

        let expires_at = if let Some(slot) = expiry_slots.get(&miner.round_id) {
            *slot
        } else {
            continue;
        };

        let fee_start_slot = expires_at.saturating_sub(TWELVE_HOURS_SLOTS);
        if clock.slot >= fee_start_slot {
            let seconds_remaining = expires_at.saturating_sub(clock.slot);
            println!(
                "[{}/{}] Checkpoint miner: {} ({} s)",
                i + 1,
                miners.len(),
                miner.authority,
                seconds_remaining as f64 * 0.4
            );
            ixs.push(godl_api::sdk::checkpoint(
                payer.pubkey(),
                miner.authority,
                miner.round_id,
            ));
        }
    }

    println!("Found {} miners to checkpoint", ixs.len());

    // Build all transactions up-front (as instruction batches).
    let mut batches: Vec<Vec<Instruction>> = Vec::new();
    while !ixs.is_empty() {
        batches.push(
            ixs.drain(..std::cmp::min(6, ixs.len()))
                .collect::<Vec<Instruction>>(),
        );
    }

    // Send + confirm everything in parallel.
    let results = send_and_confirm_transactions_in_parallel_blocking_v2(rpc, payer, batches).await?;
    let failed = results.iter().filter(|e| e.is_some()).count();
    if failed > 0 {
        return Err(anyhow!(
            "Failed to checkpoint {failed}/{} transactions",
            results.len()
        ));
    }

    // for batch in batches {
    //     let results = submit_transaction_sender(rpc, payer, &batch, None, None, false).await?;
    //     println!("Transaction submitted: {:?}", results);
    // }

    Ok(())
}

/// Close all expired rounds
pub async fn close_all(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
) -> Result<()> {
    let rounds = get_rounds(rpc).await?;
    let mut ixs = vec![];
    let clock = get_clock(rpc).await?;

    for (_address, round) in rounds.iter() {
        if clock.slot >= round.expires_at {
            ixs.push(godl_api::sdk::close(
                payer.pubkey(),
                round.id,
                round.rent_payer,
            ));
        }
    }

    // Batch and submit the instructions.
    while !ixs.is_empty() {
        let batch = ixs
            .drain(..std::cmp::min(12, ixs.len()))
            .collect::<Vec<Instruction>>();
        submit_transaction(rpc, payer, &batch).await?;
    }

    Ok(())
}

/// Close expired rounds where the payer is the rent payer
pub async fn close_rounds(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
) -> Result<()> {
    let clock = get_clock(rpc).await?;
    let payer_pubkey = payer.pubkey();
    let rounds = get_rounds(rpc).await?;
    let board = get_board(rpc).await?;
    let mut ixs = vec![];

    println!("Fetched {} rounds", rounds.len());

    for (_address, round) in rounds.iter() {
        if round.rent_payer == payer_pubkey
            && round.expires_at < clock.slot
            && round.id < board.round_id
        {
            ixs.push(godl_api::sdk::close(payer_pubkey, round.id, payer_pubkey));
        }
    }

    println!("Found {} rounds to close", ixs.len());

    // Build all transactions up-front (as instruction batches).
    let mut batches: Vec<Vec<Instruction>> = Vec::new();
    while !ixs.is_empty() {
        batches.push(
            ixs.drain(..std::cmp::min(6, ixs.len()))
                .collect::<Vec<Instruction>>(),
        );
    }

    // Send + confirm everything in parallel.
    let results = send_and_confirm_transactions_in_parallel_blocking_v2(rpc, payer, batches).await?;
    let failed = results.iter().filter(|e| e.is_some()).count();
    if failed > 0 {
        return Err(anyhow!(
            "Failed to close {failed}/{} round transactions",
            results.len()
        ));
    }

    Ok(())
}
