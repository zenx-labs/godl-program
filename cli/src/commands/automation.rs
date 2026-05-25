use anyhow::Result;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, signature::Signer};

use crate::{
    rpc::{get_board, get_config, get_var},
    transaction::submit_transaction,
};

/// Reset the round using entropy
pub async fn reset(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    top_miner: Option<Pubkey>,
) -> Result<()> {
    let board = get_board(rpc).await?;
    let config = get_config(rpc).await?;
    let var_address = config.var_address;
    let var = get_var(rpc, var_address).await?;

    println!("Var: {:?}", var);

    let client = reqwest::Client::new();
    let url = format!("https://www.godl.supply/api/entropy/var/{var_address}/seed");
    let response = client
        .get(url)
        .send()
        .await?
        .json::<entropy_types::response::GetSeedResponse>()
        .await?;
    println!("Entropy seed: {:?}", response);

    let sample_ix = entropy_api::sdk::sample(payer.pubkey(), var_address);
    let reveal_ix = entropy_api::sdk::reveal(payer.pubkey(), var_address, response.seed);
    let reset_ix = if let Some(top_miner) = top_miner {
        godl_api::sdk::reset(
            payer.pubkey(),
            config.fee_collector,
            board.round_id,
            top_miner,
            var_address,
        )
    } else {
        godl_api::sdk::reset(
            payer.pubkey(),
            config.fee_collector,
            board.round_id,
            Pubkey::default(),
            var_address,
        )
    };
    let sig = submit_transaction(rpc, payer, &[sample_ix, reveal_ix, reset_ix]).await?;
    println!("Reset: {}", sig);

    Ok(())
}

/// Reset the entropy var
pub async fn crank(rpc: &RpcClient, payer: &solana_sdk::signer::keypair::Keypair) -> Result<()> {
    let config = get_config(rpc).await?;
    let var_address = config.var_address;
    let var = get_var(rpc, var_address).await?;

    println!("Var: {:#?}", var);

    let client = reqwest::Client::new();
    let url = format!("https://www.godl.supply/api/entropy/var/{var_address}/seed");
    let response = client
        .get(url)
        .send()
        .await?
        .json::<entropy_types::response::GetSeedResponse>()
        .await?;
    println!("Entropy seed: {:#?}", response);

    let sample_ix = entropy_api::sdk::sample(payer.pubkey(), var_address);
    let reveal_ix = entropy_api::sdk::reveal(payer.pubkey(), var_address, response.seed);
    let sig = submit_transaction(rpc, payer, &[sample_ix, reveal_ix]).await?;
    println!("Crank: {:#?}", sig);

    Ok(())
}
