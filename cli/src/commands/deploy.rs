use anyhow::Result;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::signature::Signer;

use crate::{
    rpc::{get_board, get_config, get_miner, get_miner_extended},
    transaction::submit_transaction,
};

/// Deploy to a single square
pub async fn deploy(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    amount: u64,
    square_id: u64,
    pooled: bool,
) -> Result<()> {
    if square_id >= 25 {
        return Err(anyhow::anyhow!(
            "Square index must be between 0 and 24 inclusive"
        ));
    }
    let board = get_board(rpc).await?;
    let config = get_config(rpc).await?;
    let mut squares = [false; 25];
    squares[square_id as usize] = true;

    // Check if miner exists - if not, skip checkpoint. The gold-aware checkpoint needs the
    // miner's extended account, so create it first when it is missing.
    let mut ixs = Vec::new();
    if let Ok(miner) = get_miner(rpc, payer.pubkey()).await {
        if get_miner_extended(rpc, payer.pubkey()).await?.is_none() {
            ixs.push(godl_api::sdk::create_miner_extended(
                payer.pubkey(),
                payer.pubkey(),
            ));
        }
        ixs.push(godl_api::sdk::checkpoint_with_miner_extended(
            payer.pubkey(),
            payer.pubkey(),
            miner.round_id,
        ));
    }

    let deploy_ix = godl_api::sdk::deploy_with_miner_extended(
        payer.pubkey(),
        payer.pubkey(),
        config.var_address,
        amount,
        board.round_id,
        squares,
        pooled,
    );
    ixs.push(deploy_ix);

    submit_transaction(rpc, payer, &ixs).await?;
    Ok(())
}

/// Deploy to all squares
pub async fn deploy_all(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    amount: u64,
    pooled: bool,
) -> Result<()> {
    println!("Deploying to all squares with pooled: {}", pooled);
    let board = get_board(rpc).await?;
    let config = get_config(rpc).await?;
    let squares = [true; 25];

    // Check if miner exists - if not, skip checkpoint. The gold-aware checkpoint needs the
    // miner's extended account, so create it first when it is missing.
    let mut ixs = Vec::new();
    if let Ok(miner) = get_miner(rpc, payer.pubkey()).await {
        if get_miner_extended(rpc, payer.pubkey()).await?.is_none() {
            ixs.push(godl_api::sdk::create_miner_extended(
                payer.pubkey(),
                payer.pubkey(),
            ));
        }
        ixs.push(godl_api::sdk::checkpoint_with_miner_extended(
            payer.pubkey(),
            payer.pubkey(),
            miner.round_id,
        ));
    }

    let deploy_ix = godl_api::sdk::deploy_with_miner_extended(
        payer.pubkey(),
        payer.pubkey(),
        config.var_address,
        amount,
        board.round_id,
        squares,
        pooled,
    );
    ixs.push(deploy_ix);

    submit_transaction(rpc, payer, &ixs).await?;
    Ok(())
}
