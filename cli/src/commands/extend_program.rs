// Idempotently ensure the program's ProgramData account is large enough for the
// freshly built binary, extending it (permissionless ExtendProgram) if needed.
//
// On Agave the `solana program extend` CLI builds the *checked* variant, which
// requires the upgrade authority to sign. Our authority is a Squads multisig, so
// that path fails client-side. The permissionless `ExtendProgram` instruction
// (accounts: ProgramData, Program, SystemProgram, Payer — no authority) can be
// paid by any wallet, which is what this command uses.

// The `solana_sdk::bpf_loader_upgradeable` module is deprecated in favour of the
// `solana-loader-v3-interface` crate, but the helpers here (extend_program /
// get_program_data_address) still work and match the loader-v3 signatures.
#![allow(deprecated)]

use anyhow::{bail, Result};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    bpf_loader_upgradeable, pubkey::Pubkey, signature::Signer, transaction::Transaction,
};

// 45-byte UpgradeableLoaderState::ProgramData header (4 enum + 8 slot + 33 Option<Pubkey>).
const PROGRAMDATA_HEADER_LEN: usize = 45;

pub struct ExtendArgs {
    /// Program whose ProgramData account is sized. Defaults to `godl_api::ID`.
    pub program_id: Pubkey,
    /// Path to the freshly built binary, e.g. "target/deploy/godl.so".
    pub so_path: String,
    /// Extra bytes to add beyond the binary size.
    pub headroom: u64,
    /// Hard spend cap on rent, in SOL.
    pub max_sol: f64,
    /// Skip the confirmation prompt and execute.
    pub yes: bool,
}

/// Size the program's ProgramData account to fit the freshly built `.so`,
/// extending it via the permissionless `ExtendProgram` instruction if needed.
///
/// This does **not** require the upgrade authority (our multisig) to sign — the
/// additional rent is paid by `payer`. The command is idempotent: if the account
/// is already large enough it prints "No extension needed" and returns.
#[allow(deprecated)]
pub async fn extend_program(
    rpc: &RpcClient,
    payer: &solana_sdk::signer::keypair::Keypair,
    args: ExtendArgs,
) -> Result<()> {
    // Required capacity = size of the built .so plus headroom.
    let so_len = std::fs::metadata(&args.so_path)
        .map_err(|e| anyhow::anyhow!("cannot stat {}: {e}", args.so_path))?
        .len();
    let required = so_len + args.headroom;

    // Current allocated capacity = account data length minus the loader header.
    let pd = bpf_loader_upgradeable::get_program_data_address(&args.program_id);
    let acct = rpc.get_account(&pd).await?;
    let capacity = (acct.data.len() as u64).saturating_sub(PROGRAMDATA_HEADER_LEN as u64);

    println!("Program         : {}", args.program_id);
    println!("ProgramData     : {pd}");
    println!("Current capacity: {capacity} bytes");
    println!(
        "Built binary    : {so_len} bytes (+{} headroom = {required})",
        args.headroom
    );

    if capacity >= required {
        println!("✓ No extension needed (capacity {capacity} >= required {required}).");
        return Ok(());
    }

    let additional = required - capacity;
    let new_total_len = acct.data.len() as u64 + additional;

    // Budget check via on-chain rent, not a hardcoded lamports/byte rate.
    let new_rent = rpc
        .get_minimum_balance_for_rent_exemption(new_total_len as usize)
        .await?;
    let cost_lamports = new_rent.saturating_sub(acct.lamports);
    let cost_sol = cost_lamports as f64 / 1e9;
    println!("Extend by       : {additional} bytes");
    println!("Rent cost       : {cost_sol:.6} SOL");

    if cost_sol > args.max_sol {
        bail!(
            "rent cost {cost_sol:.6} SOL exceeds --max-sol {:.6}; aborting",
            args.max_sol
        );
    }

    if !args.yes {
        println!("Proceed? Re-run with --yes to execute.");
        return Ok(());
    }

    // Permissionless variant: `payer` funds the rent, no upgrade authority needed.
    // Do NOT use `extend_program_checked` — it requires the multisig authority.
    let ix = bpf_loader_upgradeable::extend_program(
        &args.program_id,
        Some(&payer.pubkey()),
        additional as u32,
    );

    let blockhash = rpc.get_latest_blockhash().await?;
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&payer.pubkey()), &[payer], blockhash);

    // Simulate first; abort on failure. This guard is our canary for mainnet ever
    // disabling permissionless extend.
    let sim = rpc.simulate_transaction(&tx).await?;
    if let Some(err) = sim.value.err {
        if let Some(logs) = sim.value.logs {
            for l in logs {
                eprintln!("  {l}");
            }
        }
        bail!("simulation failed: {err:?}");
    }

    let sig = rpc.send_and_confirm_transaction_with_spinner(&tx).await?;
    println!("✓ Extended. Signature: {sig}");
    Ok(())
}
