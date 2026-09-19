//! Post-deploy mainnet dust smoke test for the stake lifecycle surface.
//!
//! Exercises the full new-instruction loop with ~0.1 GODL and lock 0 so every
//! token returns to the wallet: DepositV2 -> TopUpStakeV2 -> DepositV2 ->
//! MergeStakeV2 -> WithdrawV2 -> ClaimYieldV2 (readonly treasury) ->
//! CloseStakeV2.
//!
//! Per-stake state is asserted strictly. Global `total_staked` deltas are
//! reported but only warn on mismatch — concurrent mainnet stake traffic can
//! legitimately interleave; run `cli verify-stake-weights` afterwards for the
//! authoritative global check.
//!
//! Usage: cargo run -p cli --example dust_smoke -- <RPC_URL> <KEYPAIR_PATH>

use godl_api::prelude::*;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{read_keypair_file, Keypair, Signer},
    system_program,
    transaction::Transaction,
};
use spl_associated_token_account::get_associated_token_address;
use steel::AccountDeserialize;

const DUST_A: u64 = ONE_GODL / 20; // 0.05 GODL
const DUST_TOPUP: u64 = ONE_GODL / 50; // 0.02 GODL
const DUST_B: u64 = ONE_GODL * 3 / 100; // 0.03 GODL

fn ix_deposit_v2(signer: Pubkey, id: u64, amount: u64, lock_duration: i64) -> Instruction {
    let stake = stake_v2_pda(signer, id).0;
    Instruction {
        program_id: godl_api::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new_readonly(MINT_ADDRESS, false),
            AccountMeta::new(get_associated_token_address(&signer, &MINT_ADDRESS), false),
            AccountMeta::new(stake, false),
            AccountMeta::new(get_associated_token_address(&stake, &MINT_ADDRESS), false),
            AccountMeta::new(treasury_pda().0, false),
            AccountMeta::new_readonly(signer, false), // executor
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(spl_token::ID, false),
            AccountMeta::new_readonly(spl_associated_token_account::ID, false),
        ],
        data: godl_api::instruction::DepositV2 {
            id: id.to_le_bytes(),
            amount: amount.to_le_bytes(),
            lock_duration: lock_duration.to_le_bytes(),
        }
        .to_bytes(),
    }
}

fn ix_withdraw_v2(signer: Pubkey, id: u64, amount: u64) -> Instruction {
    let stake = stake_v2_pda(signer, id).0;
    Instruction {
        program_id: godl_api::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new_readonly(MINT_ADDRESS, false),
            AccountMeta::new(get_associated_token_address(&signer, &MINT_ADDRESS), false),
            AccountMeta::new(stake, false),
            AccountMeta::new(get_associated_token_address(&stake, &MINT_ADDRESS), false),
            AccountMeta::new(treasury_pda().0, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(spl_token::ID, false),
            AccountMeta::new_readonly(spl_associated_token_account::ID, false),
        ],
        data: godl_api::instruction::WithdrawV2 {
            id: id.to_le_bytes(),
            amount: amount.to_le_bytes(),
        }
        .to_bytes(),
    }
}

fn ix_claim_yield_v2(signer: Pubkey, id: u64, amount: u64) -> Instruction {
    let stake = stake_v2_pda(signer, id).0;
    let treasury = treasury_pda().0;
    Instruction {
        program_id: godl_api::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new_readonly(MINT_ADDRESS, false),
            AccountMeta::new(get_associated_token_address(&signer, &MINT_ADDRESS), false),
            AccountMeta::new(stake, false),
            // Deliberately readonly: the claim path must stay treasury-write-free.
            AccountMeta::new_readonly(treasury, false),
            AccountMeta::new(
                get_associated_token_address(&treasury, &MINT_ADDRESS),
                false,
            ),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(spl_token::ID, false),
            AccountMeta::new_readonly(spl_associated_token_account::ID, false),
        ],
        data: godl_api::instruction::ClaimYieldV2 {
            id: id.to_le_bytes(),
            amount: amount.to_le_bytes(),
        }
        .to_bytes(),
    }
}

async fn send(rpc: &RpcClient, payer: &Keypair, ixs: &[Instruction]) -> anyhow::Result<()> {
    let blockhash = rpc.get_latest_blockhash().await?;
    let tx = Transaction::new_signed_with_payer(ixs, Some(&payer.pubkey()), &[payer], blockhash);
    rpc.send_and_confirm_transaction(&tx).await?;
    Ok(())
}

async fn total_staked(rpc: &RpcClient) -> anyhow::Result<u64> {
    let data = rpc.get_account_data(&treasury_pda().0).await?;
    Ok(Treasury::try_from_bytes(&data)?.total_staked)
}

async fn stake(rpc: &RpcClient, addr: Pubkey) -> anyhow::Result<StakeV2> {
    let data = rpc.get_account_data(&addr).await?;
    Ok(*StakeV2::try_from_bytes(&data)?)
}

async fn account_gone(rpc: &RpcClient, addr: Pubkey) -> anyhow::Result<bool> {
    Ok(rpc
        .get_account_with_commitment(&addr, CommitmentConfig::confirmed())
        .await?
        .value
        .is_none())
}

async fn godl_balance(rpc: &RpcClient, owner: &Pubkey) -> anyhow::Result<u64> {
    let ata = get_associated_token_address(owner, &MINT_ADDRESS);
    let bal = rpc.get_token_account_balance(&ata).await?;
    Ok(bal.amount.parse()?)
}

/// Global-counter delta check: exact on a quiet chain, but concurrent stake
/// traffic can interleave — warn, never fail. Per-stake asserts + the final
/// `verify-stake-weights` run are the authoritative checks.
fn soft_delta(label: &str, before: u64, after: u64, expected: i128) {
    let actual = after as i128 - before as i128;
    if actual == expected {
        println!("{label}: total_staked delta exact ({expected}) ✓");
    } else {
        println!(
            "{label}: WARN total_staked delta {actual} != expected {expected} \
             (concurrent traffic? verify-stake-weights will arbitrate)"
        );
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let rpc_url = args.next().expect("usage: dust_smoke <RPC_URL> <KEYPAIR_PATH>");
    let keypair_path = args.next().expect("usage: dust_smoke <RPC_URL> <KEYPAIR_PATH>");
    let payer = read_keypair_file(&keypair_path).expect("failed to read keypair");
    let rpc = RpcClient::new_with_commitment(rpc_url, CommitmentConfig::confirmed());
    let me = payer.pubkey();

    // Collision-free ids from wall time; verify both PDAs are actually unused.
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let (id_a, id_b) = (base, base + 1);
    let stake_a = stake_v2_pda(me, id_a).0;
    let stake_b = stake_v2_pda(me, id_b).0;
    assert!(account_gone(&rpc, stake_a).await?, "id {id_a} already in use");
    assert!(account_gone(&rpc, stake_b).await?, "id {id_b} already in use");
    println!("staker {me}, ids {id_a}/{id_b}, dust total 0.10 GODL, lock 0");

    let wallet_before = godl_balance(&rpc, &me).await?;

    // 1. Deposit A (0.05 GODL, lock 0) — born v1 at the 1x fixpoint.
    let t = total_staked(&rpc).await?;
    send(&rpc, &payer, &[ix_deposit_v2(me, id_a, DUST_A, 0)]).await?;
    let sa = stake(&rpc, stake_a).await?;
    assert_eq!(sa.weight_version, 1, "deposit must be born v1");
    assert_eq!(sa.multiplier, STAKE_MULTIPLIER_SCALE, "lock 0 => 1x");
    assert_eq!(sa.balance, DUST_A);
    soft_delta("deposit A", t, total_staked(&rpc).await?, DUST_A as i128);

    // 2. Top-up A (+0.02) — balance grows, created_at restarts (no-op at lock 0).
    let created_before = sa.created_at;
    let t = total_staked(&rpc).await?;
    send(
        &rpc,
        &payer,
        &[godl_api::sdk::top_up_stake_v2(me, id_a, DUST_TOPUP)],
    )
    .await?;
    let sa = stake(&rpc, stake_a).await?;
    assert_eq!(sa.balance, DUST_A + DUST_TOPUP);
    assert!(sa.created_at >= created_before, "created_at must restart");
    soft_delta("top-up A", t, total_staked(&rpc).await?, DUST_TOPUP as i128);

    // 3. Deposit B (0.03 GODL, lock 0).
    let t = total_staked(&rpc).await?;
    send(&rpc, &payer, &[ix_deposit_v2(me, id_b, DUST_B, 0)]).await?;
    assert_eq!(stake(&rpc, stake_b).await?.balance, DUST_B);
    soft_delta("deposit B", t, total_staked(&rpc).await?, DUST_B as i128);

    // 4. Merge B into A — source closed, balances fold, 1x terms preserved.
    let t = total_staked(&rpc).await?;
    send(
        &rpc,
        &payer,
        &[godl_api::sdk::merge_stake_v2(me, id_a, id_b)],
    )
    .await?;
    let sa = stake(&rpc, stake_a).await?;
    assert_eq!(sa.balance, DUST_A + DUST_TOPUP + DUST_B);
    assert_eq!(sa.lock_duration, 0);
    assert_eq!(sa.multiplier, STAKE_MULTIPLIER_SCALE, "merged 1x stays 1x");
    assert!(
        account_gone(&rpc, stake_b).await?,
        "source stake must be closed"
    );
    assert!(
        account_gone(&rpc, get_associated_token_address(&stake_b, &MINT_ADDRESS)).await?,
        "source vault must be closed"
    );
    soft_delta("merge B->A", t, total_staked(&rpc).await?, 0);

    // 5. Withdraw everything from A.
    let t = total_staked(&rpc).await?;
    let full = sa.balance;
    send(&rpc, &payer, &[ix_withdraw_v2(me, id_a, full)]).await?;
    assert_eq!(stake(&rpc, stake_a).await?.balance, 0);
    soft_delta("withdraw A", t, total_staked(&rpc).await?, -(full as i128));

    // 6. Claim with READONLY treasury meta (dust rewards or zero — both fine).
    let t = total_staked(&rpc).await?;
    send(&rpc, &payer, &[ix_claim_yield_v2(me, id_a, u64::MAX)]).await?;
    soft_delta("claim A", t, total_staked(&rpc).await?, 0);
    println!("claim with readonly treasury ✓");

    // 7. Close A — stake and vault gone, rent back to wallet.
    send(&rpc, &payer, &[godl_api::sdk::close_stake_v2(me, id_a)]).await?;
    assert!(account_gone(&rpc, stake_a).await?, "stake must be closed");
    assert!(
        account_gone(&rpc, get_associated_token_address(&stake_a, &MINT_ADDRESS)).await?,
        "vault must be closed"
    );
    println!("close A: stake + vault gone, rent recovered ✓");

    // Round trip: every dust gram is back (>= covers claimed reward dust).
    let wallet_after = godl_balance(&rpc, &me).await?;
    assert!(
        wallet_after >= wallet_before,
        "wallet GODL must round-trip: {wallet_before} -> {wallet_after}"
    );
    println!(
        "wallet GODL round trip: {} -> {} (+{} reward dust)",
        wallet_before,
        wallet_after,
        wallet_after - wallet_before
    );

    println!("dust_smoke: ALL CHECKS PASSED");
    Ok(())
}
