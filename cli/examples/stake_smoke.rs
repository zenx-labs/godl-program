//! End-to-end StakeV2 smoke test for the sqrt-weight upgrade.
//!
//! Runs a small op storm with the provided keypair as the staker — deposit
//! (must be born weight version 1), migrate idempotency, withdraw, claim —
//! asserting the exact expected `treasury.total_staked` delta after each op.
//!
//! Built for the §5.3 surfpool rehearsal (fund the keypair's GODL ATA with
//! `surfnet_setTokenAccount` first) and reusable as the §5.4 post-deploy
//! mainnet smoke test with a real funded wallet.
//!
//! Usage: cargo run -p cli --example stake_smoke -- <RPC_URL> <KEYPAIR_PATH>

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

const GODL: u64 = ONE_GODL;
const SQRT20: u128 = 4_472_135_954;

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
            // Deliberately readonly: the claim path must stay treasury-write-free (§2.3).
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let rpc_url = args.next().expect("usage: stake_smoke <RPC_URL> <KEYPAIR_PATH>");
    let keypair_path = args.next().expect("usage: stake_smoke <RPC_URL> <KEYPAIR_PATH>");
    let payer = read_keypair_file(&keypair_path).expect("failed to read keypair");
    let rpc = RpcClient::new_with_commitment(rpc_url, CommitmentConfig::confirmed());
    let me = payer.pubkey();

    // Two fresh stakes: unlocked (1x) and max-lock (exactly 20x).
    let (id_a, id_b) = (9001u64, 9002u64);
    let stake_a = stake_v2_pda(me, id_a).0;
    let stake_b = stake_v2_pda(me, id_b).0;

    let t0 = total_staked(&rpc).await?;
    send(&rpc, &payer, &[ix_deposit_v2(me, id_a, 100 * GODL, 0)]).await?;
    let sa = stake(&rpc, stake_a).await?;
    assert_eq!(sa.weight_version, 1, "deposit must be born v1");
    let t1 = total_staked(&rpc).await?;
    assert_eq!(t1 - t0, 100 * GODL, "1x weight == balance (sqrt fixpoint)");
    println!("deposit unlocked: born v1, weight delta exact ✓");

    send(&rpc, &payer, &[ix_deposit_v2(me, id_b, 200 * GODL, MAX_LOCK_DURATION)]).await?;
    let sb = stake(&rpc, stake_b).await?;
    assert_eq!(sb.weight_version, 1);
    assert_eq!(sb.multiplier, 20 * STAKE_MULTIPLIER_SCALE, "stored multiplier stays 20x");
    let t2 = total_staked(&rpc).await?;
    assert_eq!(
        (t2 - t1) as u128,
        (200 * GODL) as u128 * SQRT20 / STAKE_MULTIPLIER_SCALE as u128,
        "20x deposit enters at sqrt weight"
    );
    println!("deposit max-lock: born v1, sqrt weight delta exact ✓");

    // Migrating a born-v1 stake is a no-op.
    send(
        &rpc,
        &payer,
        &[godl_api::sdk::migrate_stake_weight(me, stake_b)],
    )
    .await?;
    let t3 = total_staked(&rpc).await?;
    assert_eq!(t2, t3, "migrate on v1 stake must not move total_staked");
    println!("migrate idempotency on fresh stake ✓");

    send(&rpc, &payer, &[ix_withdraw_v2(me, id_a, 50 * GODL)]).await?;
    let t4 = total_staked(&rpc).await?;
    assert_eq!(t3 - t4, 50 * GODL);
    println!("withdraw: weight delta exact ✓");

    // Claim with READONLY treasury meta — guards the §2.3 claim-path decision.
    send(&rpc, &payer, &[ix_claim_yield_v2(me, id_a, u64::MAX)]).await?;
    let t5 = total_staked(&rpc).await?;
    assert_eq!(t4, t5, "claim must not move total_staked");
    println!("claim with readonly treasury ✓");

    println!("stake_smoke: ALL CHECKS PASSED");
    Ok(())
}
