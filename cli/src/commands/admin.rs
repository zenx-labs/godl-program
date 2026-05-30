use anyhow::Result;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{keccak, pubkey::Pubkey, signature::Signer};

use crate::transaction::submit_transaction;

/// Set admin authority
pub async fn set_admin(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
) -> Result<()> {
    let ix = godl_api::sdk::set_admin(payer.pubkey(), payer.pubkey());
    submit_transaction(rpc, payer, &[ix]).await?;
    Ok(())
}

/// Set fee collector
pub async fn set_fee_collector(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    fee_collector: Pubkey,
) -> Result<()> {
    let ix = godl_api::sdk::set_fee_collector(payer.pubkey(), fee_collector);
    submit_transaction(rpc, payer, &[ix]).await?;
    Ok(())
}

/// Set swap program
pub async fn set_swap_program(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    swap_program: Pubkey,
) -> Result<()> {
    let ix = godl_api::sdk::set_swap_program(payer.pubkey(), swap_program);
    submit_transaction(rpc, payer, &[ix]).await?;
    Ok(())
}

/// Set motherlode denominator
pub async fn set_motherlode_denominator(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    motherlode_denominator: u64,
) -> Result<()> {
    let ix = godl_api::sdk::set_motherlode_denominator(payer.pubkey(), motherlode_denominator);
    submit_transaction(rpc, payer, &[ix]).await?;
    Ok(())
}

/// Set GODL per round
pub async fn set_godl_per_round(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    godl_per_round: u64,
) -> Result<()> {
    let ix = godl_api::sdk::set_godl_per_round(payer.pubkey(), godl_per_round);
    submit_transaction(rpc, payer, &[ix]).await?;
    Ok(())
}

/// Set VAR address
pub async fn set_var_address(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    new_var_address: Pubkey,
) -> Result<()> {
    let ix = godl_api::sdk::set_var_address(payer.pubkey(), new_var_address);
    submit_transaction(rpc, payer, &[ix]).await?;
    Ok(())
}

/// Create a new VAR account
pub async fn new_var(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    provider: Pubkey,
    commit: keccak::Hash,
    samples: u64,
) -> Result<()> {
    use godl_api::state::board_pda;

    let board_address = board_pda().0;
    let var_address = entropy_api::state::var_pda(board_address, 0).0;
    println!("Var address: {}", var_address);
    let ix = godl_api::sdk::new_var(payer.pubkey(), provider, 0, commit.to_bytes(), samples);
    submit_transaction(rpc, payer, &[ix]).await?;
    Ok(())
}

/// Withdraw SOL from the treasury vault
pub async fn withdraw_vault(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    amount: u64,
) -> Result<()> {
    let ix = godl_api::sdk::withdraw_vault(payer.pubkey(), amount);
    submit_transaction(rpc, payer, &[ix]).await?;
    println!("Withdrew {} lamports from treasury vault", amount);
    Ok(())
}

/// Simulate withdraw SOL from the treasury vault
pub async fn simulate_withdraw_vault(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    amount: u64,
) -> Result<()> {
    use crate::transaction::simulate_transaction;

    let ix = godl_api::sdk::withdraw_vault(payer.pubkey(), amount);
    println!(
        "Simulating withdrawal of {} lamports from treasury vault",
        amount
    );
    simulate_transaction(rpc, payer, &[ix]).await?;
    Ok(())
}

/// Inject GODL into the motherlode rewards pool
pub async fn inject_godl_motherlode(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    amount: f64,
) -> Result<()> {
    let amount_raw = spl_token::ui_amount_to_amount(amount, godl_api::consts::TOKEN_DECIMALS);
    let ix = godl_api::sdk::inject_godl_motherlode(payer.pubkey(), amount_raw);
    submit_transaction(rpc, payer, &[ix]).await?;
    println!("Injected {} GODL into motherlode", amount);
    Ok(())
}

/// Inject unrefined GODL rewards into a miner account (bury authority only).
pub async fn inject_unrefined_rewards(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    miner: Pubkey,
    amount: f64,
) -> Result<()> {
    let amount_raw = spl_token::ui_amount_to_amount(amount, godl_api::consts::TOKEN_DECIMALS);
    let ix = godl_api::sdk::inject_unrefined_rewards(payer.pubkey(), miner, amount_raw);
    submit_transaction(rpc, payer, &[ix]).await?;
    println!("Injected {} unrefined GODL into miner {}", amount, miner);
    Ok(())
}

/// Initialize the SolMotherlode account
pub async fn initialize_sol_motherlode(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
) -> Result<()> {
    let ix = godl_api::sdk::initialize_sol_motherlode(payer.pubkey());
    submit_transaction(rpc, payer, &[ix]).await?;
    println!("SolMotherlode account initialized successfully");
    Ok(())
}

/// Transfer GODL mint authority from the treasury PDA to the godl-mint
/// program's authority PDA.
pub async fn transfer_mint_authority(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
) -> Result<()> {
    let ix = godl_api::sdk::transfer_mint_authority(payer.pubkey());
    submit_transaction(rpc, payer, &[ix]).await?;
    Ok(())
}

/// Simulate the GODL mint authority transfer and print the PDAs involved.
pub async fn simulate_transfer_mint_authority(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
) -> Result<()> {
    use crate::transaction::simulate_transaction;

    let treasury_address = godl_api::state::treasury_pda().0;
    let godl_mint_authority_address = godl_mint_api::state::authority_pda().0;
    println!(
        "GODL mint:                  {}",
        godl_api::consts::MINT_ADDRESS
    );
    println!("Current authority (treasury PDA):  {treasury_address}");
    println!("New authority (godl-mint PDA):     {godl_mint_authority_address}");
    println!("godl-mint program:                 {}", godl_mint_api::ID);

    let ix = godl_api::sdk::transfer_mint_authority(payer.pubkey());
    simulate_transaction(rpc, payer, &[ix]).await?;
    Ok(())
}
