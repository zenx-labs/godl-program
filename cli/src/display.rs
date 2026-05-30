use godl_api::{consts::REFERRER_LOCKED_SENTINEL, prelude::*};
use solana_sdk::{native_token::lamports_to_sol, pubkey::Pubkey};
use spl_token::amount_to_ui_amount;
use steel::Clock;

use crate::rpc::get_referrer_by_address;
use solana_client::nonblocking::rpc_client::RpcClient;

/// Print Board information
pub fn print_board(board: Board, clock: &Clock) {
    let current_slot = clock.slot;
    println!("Board");
    println!("  Id: {:?}", board.round_id);
    println!("  Start slot: {}", board.start_slot);
    println!("  End slot: {}", board.end_slot);
    println!(
        "  Time remaining: {} sec",
        (board.end_slot.saturating_sub(current_slot) as f64) * 0.4
    );
}

/// Print Clock information
pub fn print_clock(clock: &Clock) {
    println!("Clock");
    println!("  slot: {}", clock.slot);
    println!("  epoch_start_timestamp: {}", clock.epoch_start_timestamp);
    println!("  epoch: {}", clock.epoch);
    println!("  leader_schedule_epoch: {}", clock.leader_schedule_epoch);
    println!("  unix_timestamp: {}", clock.unix_timestamp);
}

/// Print Config information
pub fn print_config(config: &Config) {
    println!("Config");
    println!("  admin: {}", config.admin);
    println!("  godl_per_round: {}", config.godl_per_round);
    println!("  bury_authority: {}", config.bury_authority);
    println!("  fee_collector: {}", config.fee_collector);
    println!("  swap_program: {}", config.swap_program);
    println!("  var_address: {}", config.var_address);
    println!(
        "  motherlode_denominator: {}",
        config.motherlode_denominator
    );
}

/// Print Treasury information
pub fn print_treasury(treasury: &Treasury, treasury_address: Pubkey) {
    println!("Treasury");
    println!("  address: {}", treasury_address);
    println!("  balance: {} SOL", lamports_to_sol(treasury.balance));
    println!(
        "  motherlode: {} GODL",
        amount_to_ui_amount(treasury.motherlode, TOKEN_DECIMALS)
    );
    println!(
        "  miner_rewards_factor: {}",
        treasury.miner_rewards_factor.to_i80f48().to_string()
    );
    println!(
        "  stake_rewards_factor: {}",
        treasury.stake_rewards_factor.to_i80f48().to_string()
    );
    println!(
        "  total_staked: {} GODL",
        amount_to_ui_amount(treasury.total_staked, TOKEN_DECIMALS)
    );
    println!(
        "  total_unclaimed: {} GODL",
        amount_to_ui_amount(treasury.total_unclaimed, TOKEN_DECIMALS)
    );
    println!(
        "  total_refined: {} GODL",
        amount_to_ui_amount(treasury.total_refined, TOKEN_DECIMALS)
    );
}

/// Print Round information
pub fn print_round(round: &Round, round_address: Pubkey) {
    let rng = round.rng();
    println!("Round");
    println!("  Address: {}", round_address);
    println!("  Count: {:?}", round.count);
    println!("  Deployed: {:?}", round.deployed);
    println!("  Expires at: {}", round.expires_at);
    println!("  Id: {:?}", round.id);
    println!("  Motherlode: {}", round.motherlode);
    println!("  Rent payer: {}", round.rent_payer);
    println!("  Slot hash: {:?}", round.slot_hash);
    println!("  Top miner: {:?}", round.top_miner);
    println!("  Top miner reward: {}", round.top_miner_reward);
    println!("  Total deployed: {}", round.total_deployed);
    println!("  Total vaulted: {}", round.total_vaulted);
    println!("  Total winnings: {}", round.total_winnings);
    if let Some(rng) = rng {
        println!("  Winning square: {}", round.winning_square(rng));
    }
}

/// Print Miner information
pub async fn print_miner(rpc: &RpcClient, miner: &Miner, authority: Pubkey, miner_address: Pubkey) {
    println!("Miner");
    println!("  address: {}", miner_address);
    println!("  authority: {}", authority);
    if miner.referrer == Pubkey::default() {
        println!("  referrer: unset");
    } else if miner.referrer == REFERRER_LOCKED_SENTINEL {
        println!("  referrer: locked without referrer");
    } else {
        println!("  referrer: {}", miner.referrer);
    }
    println!("  referrer_locked: {}", miner.referrer != Pubkey::default());
    println!("  deployed: {:?}", miner.deployed);
    println!("  cumulative: {:?}", miner.cumulative);
    println!("  rewards_sol: {} SOL", lamports_to_sol(miner.rewards_sol));
    println!(
        "  rewards_godl: {} GODL",
        amount_to_ui_amount(miner.rewards_godl, TOKEN_DECIMALS)
    );
    println!(
        "  refined_godl: {} GODL",
        amount_to_ui_amount(miner.refined_godl, TOKEN_DECIMALS)
    );
    println!("  round_id: {}", miner.round_id);
    println!("  checkpoint_id: {}", miner.checkpoint_id);
    println!(
        "  lifetime_deployed: {} SOL",
        lamports_to_sol(miner.lifetime_deployed)
    );
    println!(
        "  lifetime_rewards_sol: {} SOL",
        lamports_to_sol(miner.lifetime_rewards_sol)
    );
    println!(
        "  lifetime_rewards_godl: {} GODL",
        amount_to_ui_amount(miner.lifetime_rewards_godl, TOKEN_DECIMALS)
    );
    if miner.referrer != Pubkey::default() && miner.referrer != REFERRER_LOCKED_SENTINEL {
        if let Ok(referrer) = get_referrer_by_address(rpc, miner.referrer).await {
            println!("  referrer_authority: {}", referrer.authority);
            println!(
                "  referrer_pending_sol: {} SOL",
                lamports_to_sol(referrer.rewards_sol)
            );
            println!(
                "  referrer_pending_godl: {} GODL",
                amount_to_ui_amount(referrer.rewards_godl, TOKEN_DECIMALS)
            );
            println!(
                "  referrer_cumulative_sol: {} SOL",
                lamports_to_sol(referrer.cumulative_rewards_sol)
            );
            println!(
                "  referrer_cumulative_godl: {} GODL",
                amount_to_ui_amount(referrer.cumulative_rewards_godl, TOKEN_DECIMALS)
            );
            println!("  referrer_count: {}", referrer.referrer_count);
        }
    }
}

/// Print Stake information
pub fn print_stake(stake: &Stake, authority: Pubkey, staker_address: Pubkey) {
    println!("Stake");
    println!("  address: {}", staker_address);
    println!("  authority: {}", authority);
    println!(
        "  balance: {} GODL",
        amount_to_ui_amount(stake.balance, TOKEN_DECIMALS)
    );
    println!("  last_claim_at: {}", stake.last_claim_at);
    println!("  last_deposit_at: {}", stake.last_deposit_at);
    println!("  last_withdraw_at: {}", stake.last_withdraw_at);
    println!(
        "  rewards_factor: {}",
        stake.rewards_factor.to_i80f48().to_string()
    );
    println!(
        "  rewards: {} GODL",
        amount_to_ui_amount(stake.rewards, TOKEN_DECIMALS)
    );
    println!(
        "  lifetime_rewards: {} GODL",
        amount_to_ui_amount(stake.lifetime_rewards, TOKEN_DECIMALS)
    );
}

/// Print Automation information
pub fn print_automations(automations: &[(Pubkey, Automation)]) {
    for (i, (address, automation)) in automations.iter().enumerate() {
        println!("[{}/{}] {}", i + 1, automations.len(), address);
        println!("  authority: {}", automation.authority);
        println!("  balance: {}", automation.balance);
        println!("  executor: {}", automation.executor);
        println!("  fee: {}", automation.fee);
        println!("  mask: {}", automation.mask);
        println!("  strategy: {}", automation.strategy);
        println!();
    }
}

/// Print participating miners
pub fn print_participating_miners(miners: &[(Pubkey, Miner)]) {
    for (i, (_address, miner)) in miners.iter().enumerate() {
        println!("{}: {}", i, miner.authority);
    }
}
