use anyhow::Result;
use entropy_api::prelude::*;
use godl_api::prelude::*;
use solana_account_decoder::UiAccountEncoding;
use solana_client::{
    client_error::{reqwest::StatusCode, ClientErrorKind},
    nonblocking::rpc_client::RpcClient,
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
    rpc_filter::{Memcmp, RpcFilterType},
};
use solana_sdk::pubkey::Pubkey;
use steel::{AccountDeserialize, Clock, Discriminator};

/// Fetch all program accounts for a specific type with optional filters
pub async fn get_program_accounts<T>(
    client: &RpcClient,
    program_id: Pubkey,
    filters: Vec<RpcFilterType>,
) -> Result<Vec<(Pubkey, T)>>
where
    T: AccountDeserialize + Discriminator + Clone,
{
    let mut all_filters = vec![RpcFilterType::Memcmp(Memcmp::new_base58_encoded(
        0,
        &T::discriminator().to_le_bytes(),
    ))];
    all_filters.extend(filters);
    let result = client
        .get_program_accounts_with_config(
            &program_id,
            RpcProgramAccountsConfig {
                filters: Some(all_filters),
                account_config: RpcAccountInfoConfig {
                    encoding: Some(UiAccountEncoding::Base64),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await;

    match result {
        Ok(accounts) => {
            let accounts = accounts
                .into_iter()
                .filter_map(|(pubkey, account)| {
                    if let Ok(account) = T::try_from_bytes(&account.data) {
                        Some((pubkey, account.clone()))
                    } else {
                        None
                    }
                })
                .collect();
            Ok(accounts)
        }
        Err(err) => match err.kind {
            ClientErrorKind::Reqwest(err) => {
                if let Some(status_code) = err.status() {
                    if status_code == StatusCode::GONE {
                        panic!(
                                "\n{} Your RPC provider does not support the getProgramAccounts endpoint, needed to execute this command. Please use a different RPC provider.\n",
                                "ERROR"
                            );
                    }
                }
                Err(anyhow::anyhow!("Failed to get program accounts: {}", err))
            }
            _ => Err(anyhow::anyhow!("Failed to get program accounts: {}", err)),
        },
    }
}

/// Get the Board account
pub async fn get_board(rpc: &RpcClient) -> Result<Board> {
    let board_pda = godl_api::state::board_pda();
    let account = rpc.get_account(&board_pda.0).await?;
    let board = Board::try_from_bytes(&account.data)?;
    Ok(*board)
}

/// Get a VAR account
pub async fn get_var(rpc: &RpcClient, address: Pubkey) -> Result<Var> {
    let account = rpc.get_account(&address).await?;
    let var = Var::try_from_bytes(&account.data)?;
    Ok(*var)
}

/// Get a Round account
pub async fn get_round(rpc: &RpcClient, id: u64) -> Result<Round> {
    let round_pda = godl_api::state::round_pda(id);
    let account = rpc.get_account(&round_pda.0).await?;
    let round = Round::try_from_bytes(&account.data)?;
    Ok(*round)
}

/// Get all Round accounts
pub async fn get_rounds(rpc: &RpcClient) -> Result<Vec<(Pubkey, Round)>> {
    get_program_accounts::<Round>(rpc, godl_api::ID, vec![]).await
}

/// Get the Treasury account
pub async fn get_treasury(rpc: &RpcClient) -> Result<Treasury> {
    let treasury_pda = godl_api::state::treasury_pda();
    let account = rpc.get_account(&treasury_pda.0).await?;
    let treasury = Treasury::try_from_bytes(&account.data)?;
    Ok(*treasury)
}

/// Get the Config account
pub async fn get_config(rpc: &RpcClient) -> Result<Config> {
    let config_pda = godl_api::state::config_pda();
    let account = rpc.get_account(&config_pda.0).await?;
    let config = Config::try_from_bytes(&account.data)?;
    Ok(*config)
}

/// Get a Miner account by authority
pub async fn get_miner(rpc: &RpcClient, authority: Pubkey) -> Result<Miner> {
    let miner_pda = godl_api::state::miner_pda(authority);
    let account = rpc.get_account(&miner_pda.0).await?;
    let miner = Miner::try_from_bytes(&account.data)?;
    Ok(*miner)
}

/// Get all Miner accounts
pub async fn get_miners(rpc: &RpcClient) -> Result<Vec<(Pubkey, Miner)>> {
    get_program_accounts::<Miner>(rpc, godl_api::ID, vec![]).await
}

/// Get miners participating in a specific round
pub async fn get_miners_participating(
    rpc: &RpcClient,
    round_id: u64,
) -> Result<Vec<(Pubkey, Miner)>> {
    // Filter by `round_id` field inside Miner; account layout is:
    // [8-byte discriminator][Miner struct], with `round_id` starting at byte 536
    // of the struct, so offset = 8 + 536 = 544.
    let filter = RpcFilterType::Memcmp(Memcmp::new_base58_encoded(544, &round_id.to_le_bytes()));
    get_program_accounts::<Miner>(rpc, godl_api::ID, vec![filter]).await
}

/// Get a Referrer account by address
pub async fn get_referrer_by_address(rpc: &RpcClient, address: Pubkey) -> Result<Referrer> {
    let account = rpc.get_account(&address).await?;
    let referrer = Referrer::try_from_bytes(&account.data)?;
    Ok(*referrer)
}

/// Get a Referrer account by authority
#[allow(dead_code)]
pub async fn get_referrer(rpc: &RpcClient, authority: Pubkey) -> Result<Referrer> {
    let (address, _) = godl_api::state::referrer_pda(authority);
    get_referrer_by_address(rpc, address).await
}

/// Get the Clock sysvar
pub async fn get_clock(rpc: &RpcClient) -> Result<Clock> {
    let data = rpc.get_account_data(&solana_sdk::sysvar::clock::ID).await?;
    let clock = bincode::deserialize::<Clock>(&data)?;
    Ok(clock)
}

/// Get a Stake account by authority
pub async fn get_stake(rpc: &RpcClient, authority: Pubkey) -> Result<Stake> {
    let stake_pda = godl_api::state::stake_pda(authority);
    let account = rpc.get_account(&stake_pda.0).await?;
    let stake = Stake::try_from_bytes(&account.data)?;
    Ok(*stake)
}

/// Get all Automation accounts for the zenx executor
pub async fn get_automations(rpc: &RpcClient) -> Result<Vec<(Pubkey, Automation)>> {
    use solana_sdk::pubkey;
    const EXECUTOR: Pubkey = pubkey!("botHfLbBG8oSrohhfCF63xj3LhpBjJrYQkyE27gA4rN");
    let filter = RpcFilterType::Memcmp(Memcmp::new_base58_encoded(56, &EXECUTOR.to_bytes()));
    get_program_accounts::<Automation>(rpc, godl_api::ID, vec![filter]).await
}
