mod commands;
mod display;
mod jupiter;
mod rpc;
mod transaction;

use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use commands::*;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::signature::Signer;
use solana_sdk::{keccak, pubkey::Pubkey, signature::read_keypair_file};

fn parse_keccak_hash(s: &str) -> Result<keccak::Hash, String> {
    // Remove 0x prefix if present
    let s = s.strip_prefix("0x").unwrap_or(s);

    // Decode hex string to bytes
    let bytes = hex::decode(s).map_err(|e| format!("Failed to decode hex string: {}", e))?;

    // Ensure we have exactly 32 bytes
    if bytes.len() != 32 {
        return Err(format!("Hash must be 32 bytes, got {}", bytes.len()));
    }

    // Convert to array and create Hash
    let mut hash_bytes = [0u8; 32];
    hash_bytes.copy_from_slice(&bytes);
    Ok(keccak::Hash::new(&hash_bytes))
}

#[derive(Parser)]
#[command(
    name = "godl",
    about = "Improved CLI for interacting with the GODL program",
    version
)]
struct Cli {
    #[arg(long, env = "RPC", help = "RPC endpoint URL")]
    rpc: String,
    #[arg(
        long,
        env = "KEYPAIR",
        value_name = "PATH",
        help = "Path to the payer keypair file"
    )]
    keypair: PathBuf,
    #[arg(
        long,
        env = "JUP_API_KEY",
        help = "Jupiter Swap V2 API key (required for the bury commands)"
    )]
    jup_api_key: Option<String>,
    #[arg(
        long,
        env = "JUP_API_BASE_URL",
        default_value = jupiter::DEFAULT_BASE_URL,
        help = "Jupiter Swap API base URL"
    )]
    jup_api_base_url: String,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
#[command(rename_all = "kebab-case")]
enum Commands {
    Automations,
    Clock,
    Claim,
    InitializeReferrer,
    SetReferrer {
        #[arg(long, help = "Referrer authority address", value_name = "PUBKEY")]
        referrer_authority: Option<Pubkey>,
    },
    ClaimReferral,
    Board,
    Config,
    Bury {
        #[arg(long, help = "Amount in SOL to swap")]
        amount: f64,
        #[arg(long, help = "Send GODL to warchest instead of burning")]
        no_burn: bool,
    },
    BuryListen {
        #[arg(long, help = "Amount in SOL to swap")]
        amount: f64,
        #[arg(long, help = "Send GODL to warchest instead of burning")]
        no_burn: bool,
    },
    ManualBury {
        #[arg(long, help = "Amount of GODL to bury (decimal)")]
        amount: f64,
        #[arg(long, help = "Send GODL to warchest instead of burning")]
        no_burn: bool,
    },
    ManualBuryListen {
        #[arg(long, help = "Amount of GODL threshold (decimal)")]
        amount: f64,
        #[arg(long, help = "Send GODL to warchest instead of burning")]
        no_burn: bool,
    },
    Reset {
        #[arg(long, help = "Authority of the top miner for pooled payout")]
        top_miner: Option<Pubkey>,
    },
    Crank,
    Treasury,
    Miner {
        #[arg(long, help = "Miner authority address")]
        authority: Option<Pubkey>,
    },
    Deploy {
        #[arg(long, help = "Deployment amount in lamports")]
        amount: u64,
        #[arg(long, help = "Square index to deploy to (0-24)")]
        square: u64,
        #[arg(long, help = "Deploy using mining pool" )]
        pooled: bool,
    },
    Stake {
        #[arg(long, help = "Stake authority address")]
        authority: Option<Pubkey>,
    },
    Explore,
    DeployAll {
        #[arg(long, help = "Deployment amount in lamports")]
        amount: u64,
        #[arg(long, help = "Deploy using mining pool" )]
        pooled: bool,
    },
    Round {
        #[arg(long, help = "Round identifier")]
        id: u64,
    },
    SetAdmin,
    SetFeeCollector {
        #[arg(long, help = "New fee collector address")]
        fee_collector: Pubkey,
    },
    Ata {
        #[arg(long, help = "User address")]
        user: Pubkey,
    },
    Checkpoint {
        #[arg(long, help = "Miner authority address")]
        authority: Option<Pubkey>,
    },
    CheckpointAll,
    CheckpointRounds,
    CloseAll,
    CloseRounds,
    ParticipatingMiners {
        #[arg(long, help = "Round identifier")]
        id: u64,
    },
    NewVar {
        #[arg(long, help = "Provider authority address")]
        provider: Pubkey,
        #[arg(long, help = "Commit hash (hex string, with or without 0x prefix)", value_parser = parse_keccak_hash)]
        commit: keccak::Hash,
        #[arg(long, help = "Number of samples")]
        samples: u64,
    },
    SetMotherlodeDenominator {
        #[arg(long, help = "New motherlode denominator value")]
        motherlode_denominator: u64,
    },
    SetGodlPerRound {
        #[arg(long, help = "New godl per round value")]
        godl_per_round: u64,
    },
    SetSwapProgram {
        #[arg(long, help = "Swap program address")]
        swap_program: Pubkey,
    },
    SetVarAddress {
        #[arg(long, help = "New VAR account address")]
        var: Pubkey,
    },
    WithdrawVault {
        #[arg(long, help = "Amount in lamports to withdraw from treasury vault")]
        amount: u64,
    },
    SimulateWithdrawVault {
        #[arg(long, help = "Amount in lamports to withdraw from treasury vault")]
        amount: u64,
    },
    InitializeSolMotherlode,
    InjectGodlMotherlode {
        #[arg(long, help = "Amount of GODL to inject (decimal, e.g. 1.5)")]
        amount: f64,
    },
    InjectUnrefinedRewards {
        #[arg(long, help = "Miner authority address")]
        miner: Pubkey,
        #[arg(long, help = "Amount of GODL to inject (e.g. 100, 500)")]
        amount: f64,
    },
    Keys {
        #[arg(long, help = "Authority address")]
        authority: Option<Pubkey>,
    },
    Lut,
}

fn build_jupiter_client(cli: &Cli) -> anyhow::Result<jupiter::JupiterClient> {
    let api_key = cli
        .jup_api_key
        .as_deref()
        .context("JUP_API_KEY is required for bury commands (set in .env or pass --jup-api-key)")?;
    Ok(jupiter::JupiterClient::new(&cli.jup_api_base_url, api_key))
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Load .env file if it exists (silently ignore if not found)
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();
    let payer = read_keypair_file(&cli.keypair).map_err(|err| {
        anyhow::anyhow!(
            "Failed to read keypair from path {}: {err}",
            cli.keypair.to_string_lossy()
        )
    })?;
    let rpc = RpcClient::new_with_commitment(cli.rpc.clone(), CommitmentConfig::processed());

    match cli.command {
        Commands::Automations => {
            log_automations(&rpc).await?;
        }
        Commands::Clock => {
            log_clock(&rpc).await?;
        }
        Commands::Claim => {
            claim(&rpc, &payer).await?;
        }
        Commands::InitializeReferrer => {
            initialize_referrer_cmd(&rpc, &payer).await?;
        }
        Commands::SetReferrer { referrer_authority } => {
            set_referrer_cmd(&rpc, &payer, referrer_authority).await?;
        }
        Commands::ClaimReferral => {
            claim_referral_cmd(&rpc, &payer).await?;
        }
        Commands::Board => {
            log_board(&rpc).await?;
        }
        Commands::Config => {
            log_config(&rpc).await?;
        }
        Commands::Bury { amount, no_burn } => {
            let jup = build_jupiter_client(&cli)?;
            bury(&rpc, &payer, amount, &jup, no_burn).await?;
        }
        Commands::BuryListen { amount, no_burn } => {
            let jup = build_jupiter_client(&cli)?;
            bury_listen(&rpc, &payer, amount, &jup, no_burn).await?;
        }
        Commands::ManualBury { amount, no_burn } => {
            manual_bury(&rpc, &payer, amount, no_burn).await?;
        }
        Commands::ManualBuryListen { amount, no_burn } => {
            manual_bury_listen(&rpc, &payer, amount, no_burn).await?;
        }
        Commands::Reset { top_miner } => {
            reset(&rpc, &payer, top_miner).await?;
        }
        Commands::Crank => {
            crank(&rpc, &payer).await?;
        }
        Commands::Treasury => {
            log_treasury(&rpc).await?;
        }
        Commands::Miner { authority } => {
            log_miner(&rpc, &payer, authority).await?;
        }
        Commands::Deploy { amount, square, pooled } => {
            deploy(&rpc, &payer, amount, square, pooled).await?;
        }
        Commands::Stake { authority } => {
            log_stake(&rpc, &payer, authority).await?;
        }
        Commands::Explore => {
            explore(&rpc).await?;
        }
        Commands::DeployAll { amount, pooled } => {
            deploy_all(&rpc, &payer, amount, pooled).await?;
        }
        Commands::Round { id } => {
            log_round(&rpc, id).await?;
        }
        Commands::SetAdmin => {
            set_admin(&rpc, &payer).await?;
        }
        Commands::SetFeeCollector { fee_collector } => {
            set_fee_collector(&rpc, &payer, fee_collector).await?;
        }
        Commands::Ata { user } => {
            ata(&rpc, &payer, user).await?;
        }
        Commands::Checkpoint { authority } => {
            checkpoint(&rpc, &payer, authority).await?;
        }
        Commands::CheckpointAll => {
            checkpoint_all(&rpc, &payer).await?;
        }
        Commands::CheckpointRounds => {
            checkpoint_rounds(&rpc, &payer).await?;
        }
        Commands::CloseAll => {
            close_all(&rpc, &payer).await?;
        }
        Commands::CloseRounds => {
            close_rounds(&rpc, &payer).await?;
        }
        Commands::ParticipatingMiners { id } => {
            participating_miners(&rpc, id).await?;
        }
        Commands::NewVar {
            provider,
            commit,
            samples,
        } => {
            new_var(&rpc, &payer, provider, commit, samples).await?;
        }
        Commands::SetMotherlodeDenominator {
            motherlode_denominator,
        } => {
            set_motherlode_denominator(&rpc, &payer, motherlode_denominator).await?;
        }
        Commands::SetGodlPerRound { godl_per_round } => {
            set_godl_per_round(&rpc, &payer, godl_per_round).await?;
        }
        Commands::SetSwapProgram { swap_program } => {
            set_swap_program(&rpc, &payer, swap_program).await?;
        }
        Commands::SetVarAddress { var } => {
            set_var_address(&rpc, &payer, var).await?;
        }
        Commands::WithdrawVault { amount } => {
            withdraw_vault(&rpc, &payer, amount).await?;
        }
        Commands::SimulateWithdrawVault { amount } => {
            simulate_withdraw_vault(&rpc, &payer, amount).await?;
        }
        Commands::InitializeSolMotherlode => {
            initialize_sol_motherlode(&rpc, &payer).await?;
        }
        Commands::InjectGodlMotherlode { amount } => {
            inject_godl_motherlode(&rpc, &payer, amount).await?;
        }
        Commands::InjectUnrefinedRewards { miner, amount } => {
            inject_unrefined_rewards(&rpc, &payer, miner, amount).await?;
        }
        Commands::Keys { authority } => {
            keys(
                &rpc,
                authority.unwrap_or_else(|| payer.try_pubkey().unwrap()),
            )
            .await?;
        }
        Commands::Lut => {
            lut(&rpc, &payer).await?;
        }
    }

    Ok(())
}
