# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

GODL is a Solana on-chain crypto mining protocol built with the [steel](https://crates.io/crates/steel) framework. Program ID: `mineWsRs2Rmw2jPMkVbgAbDjV1E23yQ8TEodaX3iza4`. GODL token mint: `GodL6KZ9uuUoQwELggtVzQkKmU1LfqmDokPibPeDKkhF` (11 decimals; 100 billion "grams" per GODL; max supply 2.1M).

## Workspace layout

Cargo workspace pinned to Rust `1.85.0` (see `rust-toolchain.toml`). Members:

- `api/` — `godl-api` crate. The on-chain program's public surface: `consts`, `error`, `event`, `instruction` (the `GodlInstruction` enum + zero-copy `Pod` arg structs), `state` (account layouts and PDA seed helpers), and `sdk` (instruction builders for off-chain use). `prelude` re-exports everything; both `program/` and `cli/` import via `godl_api::prelude::*`.
- `program/` — `godl-program` crate, the on-chain BPF program (`crate-type = ["cdylib", "lib"]`, lib name `godl`). Each instruction handler lives in its own file under `program/src/{admin,automation,miner,referral,stake,stake_v2}/`; `lib.rs::process_instruction` dispatches on `GodlInstruction`.
- `cli/` — `godl` CLI binary that talks to the program via RPC. Subcommand definitions are in `cli/src/main.rs`; implementations under `cli/src/commands/`. Reads `RPC` and `KEYPAIR` from CLI args or env (auto-loads `.env`).
- `vendor/jupiter-swap-api-client/` — vendored Jupiter swap client used by the `bury` instruction.

External dep: `entropy-api` is pulled from `https://github.com/oxapo11o/entropy-program.git` (Git, not crates.io). It supplies the on-chain entropy `Var` used to seed round RNG.

## Common commands

```bash
# Build everything (host targets)
cargo build --release

# Build just the on-chain program (BPF/SBF)
cargo build-sbf --manifest-path program/Cargo.toml

# Run the test suite (uses solana-program-test under sbf)
cargo test-sbf

# Coverage
cargo llvm-cov

# Type-check fast without producing artifacts
cargo check --workspace

# Run the CLI (RPC + KEYPAIR can come from .env)
cargo run -p cli -- <subcommand> [...]
# e.g.
cargo run -p cli -- board
cargo run -p cli -- deploy --amount 100000 --square 7 --pooled
```

There are essentially no native Rust unit tests in this repo (only one `#[cfg(test)]` block in `api/src/state/round.rs`). Almost all coverage comes from `cargo test-sbf`, which runs program-level integration tests via the Solana test framework.

## Architecture

### Instruction model

Every instruction follows the same shape:

1. A discriminant in `GodlInstruction` (enum `u8`, see `api/src/instruction.rs`). Numbers are stable across versions — **do not reuse or renumber** them; deprecated variants stay in the enum and return `InvalidInstructionData` from the program. New instructions append to the enum.
2. A zero-copy POD args struct (`#[repr(C)] #[derive(Pod, Zeroable)]`) for the instruction data, with all multi-byte integers stored as little-endian byte arrays (`[u8; 8]`, etc.) so the layout is alignment-agnostic on the BPF target. Wired up with the `instruction!` macro at the bottom of `instruction.rs`.
3. A handler `process_<name>` in the corresponding `program/src/.../` module, dispatched from `program/src/lib.rs::process_instruction`.
4. A builder in `api/src/sdk.rs` that off-chain callers (the CLI and any external clients) use to construct the `Instruction` with the right account ordering.

When adding an instruction: update all four (enum + POD struct + `instruction!` macro + handler + dispatcher arm + sdk builder). The CLI typically grows a corresponding subcommand in `cli/src/main.rs` and a function in `cli/src/commands/`.

### Account model & PDAs

All program state lives in PDAs whose seeds are defined as `&[u8]` constants in `api/src/consts.rs` (e.g. `BOARD`, `MINER`, `ROUND`, `POOL_ROUND`, `STAKE_V2`, `OTC_TREASURY`). Helper functions in `api/src/state/mod.rs` (`board_pda()`, `miner_pda(authority)`, `round_pda(id)`, `stake_v2_pda(authority, id)`, etc.) are the canonical way to derive addresses — never re-derive seeds inline.

Account discriminants are in the `GodlAccount` enum (`api/src/state/mod.rs`). Each account struct registers itself via the `account!(GodlAccount, MyState)` macro at the bottom of its module. State structs are `#[repr(C)] Pod + Zeroable` and are typically read/written with steel's `as_account::<T>` / `as_account_mut::<T>` helpers, plus `has_seeds(...)` and `has_address(...)` for verification. A `buffer: [u8; N]` field at the end of a struct (e.g. `OTC_Treasury`, `StakeV2`) reserves space for forward-compatible field additions — preserve and respect those.

### Versioned instruction families

The protocol has gone through several rounds of versioning. The mining/round flow is currently **V3**:

- `DeployV3` — places a deployment on a square (0–24); `is_pooled` flag opts the miner into the shared `PoolMember`/`PoolRound` accounts for that round.
- `CheckpointV3` — settles round results for a miner; for pooled rounds it splits the top-miner share proportionally across pool members.
- `ResetV3` — closes out a round, verifies the supplied top miner, and (for pool wins) writes `round.top_miner = POOL_ADDRESS` so checkpointing knows to do proportional payouts.
- `CloseV2` — reclaims rent from expired round + pool-round accounts.
- `AutomateV3` — configures the executor strategy with optional pool participation.

Earlier variants (`DeployV2`, `Checkpoint`, `ResetV2`, `Close`) are kept in the enum/dispatcher but their handlers return `InvalidInstructionData`. Staking is split between V1 (`Deposit`/`Withdraw`/`ClaimYield`) and V2 (lockable, multiplier-weighted, optional NFT boost via the `NFT_BOOST_COLLECTION` Metaplex Core collection — `StakeNft`/`UnstakeNft` toggle a 1.10× boost on `weighted_units`). `DeployV4`/`CheckpointV4`/`ResetV4`/`AutomateV4`/`ClaimSpl` POD structs exist in `instruction.rs` but are **not yet wired into `GodlInstruction` or the dispatcher** — they are scaffolding for a future SPL-collateral mining track that also includes new state (`TreasuryExtended`, `MinerExtended`, `OTC_Treasury`).

### Reward accounting pattern

Rewards use a "rewards factor" pattern (cumulative rewards per unit, fixed-point via steel's `Numeric`). See `Treasury::miner_rewards_factor` / `stake_rewards_factor`, and the `update_rewards` methods on `Miner` and `StakeV2`: each account stores the factor it last observed, and pulls its share lazily by multiplying the delta by its weight (deployed / staked / weighted units). Anything that mutates `total_unclaimed`, `total_staked`, or those factors must keep them consistent — the math relies on always calling `update_rewards` *before* changing the weight.

### Bury / treasury flow

`PreBury` → `Bury` swaps SOL from the treasury vault to GODL through the configured swap program (Jupiter, set via `SetSwapProgram`), shares a slice with stakers and admin (`STAKERS_BPS`, `ADMIN_BPS` of `DENOMINATOR_BPS = 10_000`), and burns the rest. `BuryTokens` is the manual-input variant that skips the swap. The CLI's `bury-listen` / `manual-bury-listen` commands watch balances and trigger automatically when a threshold is reached.

### Constants worth knowing

- Slot timing assumes ~150 slots/min (`ONE_MINUTE_SLOTS`); rounds run for `~1 minute` of mining (`board.end_slot = start_slot + 150`) with a 35-slot intermission and a `ONE_DAY_SLOTS` claim window.
- Sentinel pubkeys: `SPLIT_ADDRESS` (rewards split across all miners on a square), `POOL_ADDRESS` (pool win), `REFERRER_LOCKED_SENTINEL` (miner explicitly opted out of having a referrer).
- `DEPLOYER_ADDRESS` is the only authority allowed to call `Initialize`.

## Working with this codebase

- Both `[profile.dev]` and `[profile.release]` enable `overflow-checks`. Use `checked_*` arithmetic on user-influenced values; the existing code uses `GodlError::StakeOverflow` and similar variants for graceful handling.
- POD args use little-endian byte arrays so callers must pass `value.to_le_bytes()` and handlers must call `u64::from_le_bytes(args.field)`. Don't try to make these `u64` directly — alignment on the BPF target will bite you.
- The `entropy-api` dep is a Git dependency; `cargo update -p entropy-api` is how you bump it. `Cargo.lock` is committed.
- `target/`, `wallets/`, and `.env` are `.gitignore`d, so a fresh clone has no keypairs and no RPC/database/NATS credentials — the CLI will refuse to run until you provide them.
