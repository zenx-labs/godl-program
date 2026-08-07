use anyhow::Result;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::signer::Signer;

use crate::transaction::submit_transaction;

/// Close an empty stake account: claims pending rewards, sweeps vault dust,
/// and returns the vault + account rent to the signer.
pub async fn close_stake(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    id: u64,
) -> Result<()> {
    let ix = godl_api::sdk::close_stake_v2(payer.pubkey(), id);
    submit_transaction(rpc, payer, &[ix]).await?;
    println!("Closed stake account id {id}");
    Ok(())
}

/// Top up an existing stake account with additional GODL. Restarts the lock:
/// the whole balance is locked for the full lock_duration from now.
pub async fn top_up_stake(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    id: u64,
    amount: f64,
) -> Result<()> {
    let amount_raw = spl_token::ui_amount_to_amount(amount, godl_api::consts::TOKEN_DECIMALS);
    let ix = godl_api::sdk::top_up_stake_v2(payer.pubkey(), id, amount_raw);
    submit_transaction(rpc, payer, &[ix]).await?;
    println!("Topped up stake id {id} with {amount} GODL (lock restarted from now)");
    Ok(())
}

/// Merge the source stake account into the target and close the source. The
/// merged account takes the longer lock_duration of the two, restarted from
/// now, with the multiplier recomputed from that duration.
pub async fn merge_stake(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    target_id: u64,
    source_id: u64,
) -> Result<()> {
    let ix = godl_api::sdk::merge_stake_v2(payer.pubkey(), target_id, source_id);
    submit_transaction(rpc, payer, &[ix]).await?;
    println!("Merged stake id {source_id} into id {target_id} (lock restarted from now)");
    Ok(())
}
