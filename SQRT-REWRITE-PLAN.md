# Stake Reward Reweight: sqrt Effective-Weight Curve (+ total_staked verification backstop)

> **IMPLEMENTATION STATUS (2026-08-05, same day):** the program-side plan (§2,
> §2.4, §5.1, §5.2, §5.5, and the §5.3 fork rehearsal) is **implemented in the
> working tree** — uncommitted, per the no-commits rule. See the checked boxes
> in §6 and the "implemented:" notes inline. §3 remains delegated to the
> interface/indexer agent; §5.4 smoke tests and the Rollout sequence run at
> deploy time.

Implementation plan, written 2026-08-05. **Revised later the same day** after
source-level verification against the godl-program code and a live mainnet
re-measurement (program `mineWsRs2Rmw2jPMkVbgAbDjV1E23yQ8TEodaX3iza4`). Key
revisions: (a) `ExecuteOtcTrade` also creates StakeV2 accounts and must set the
weight version (§2.2); (b) `claim_yield_v2` is dropped from the auto-migrate list —
claim-path treasury writes could break live clients (§2.3); (c) the total_staked
deficit has already been repaired on-chain — the rebase instruction is demoted to a
verification backstop (§1, §2.3); (d) environment notes rewritten for the actual
workspace (below); (e) the program repo has no integration-test harness today — §5.2
must scaffold it from scratch. Re-measure all numbers at execution time with the
script in Appendix A; they drift hourly.

**Repos touched:** the godl-program workspace at `/workspace` (primary; contains
`api/`, `program/`, `cli/`). `godl-interface` and `godl-indexer` are **not present
in this environment** — their location/owner is TBD; §3 is blocked until they are
provided. Deliver as one PR per repo, sequenced per the Rollout section.

**Hard rules for the implementing agent:**

- Environment: local devcontainer, no sudo. The hardened-sandbox runner referenced
  by the original plan does not exist here; builds/tests/surfpool run as the local
  user in `/workspace`. Tooling **installed 2026-08-05 (user-approved)**: Agave
  2.3.13 (`solana`, `cargo-build-sbf`, `cargo-test-sbf`; PATH via `~/.profile`),
  surfpool 1.5.0 (`~/.local/bin`), cargo-fuzz 0.13.2 (+ rust nightly to run it),
  cargo-llvm-cov 0.8.7. Keypairs/RPC creds stay in `.env`/`wallets/` (gitignored),
  never committed.
- **NO COMMITS in this workspace (user directive).** Make all changes in the
  working tree only — never `git commit`, `git push`, branch, or tag. The user
  reviews and commits themselves. This supersedes any commit/PR step elsewhere in
  this plan as far as the implementing agent is concerned.
- Instruction discriminants are append-only. Never renumber or reuse (`api/src/instruction.rs`).
- Do not modify the stored `StakeV2.multiplier` field, its deposit-time computation
  (`stake_multiplier()` in `program/src/stake_v2/deposit_v2.rs`), or any UI copy that
  shows the 1x–20x multiplier. Only the _internal weight function_ changes.
- Every APR figure the interface displays MUST be computed from the new effective
  weight. Keeping the "20x" badge is fine; showing an APR derived from the raw
  multiplier after this change would overstate locked yield ~4x. Non-negotiable.

---

## 1. Context and goal

Staking rewards (3% of each bury, `STAKERS_BPS`) are split pro-rata by
`weighted_units = balance × multiplier (× 1.1 NFT boost)`, where `multiplier` is
linear 1x→20x over 0→730 days of lock. Measured 2026-08-05:

- 6,214 StakeV2 accounts (6,224 by the same-day re-measurement — grows steadily),
  896 legacy v1 Stake accounts (v1 weight = balance, 1x). The v1 staking
  instructions (`Deposit`/`Withdraw`/`ClaimYield` = 10/11/12) are still live in the
  dispatcher; v1 weight is identically balance, which is already the sqrt fixpoint
  (`w(1)=1`), so v1 needs no migration — but v1 balances must be included in every
  true-sum computation.
- 86% of staked balance (112,851 of ~131k GODL) sits at exactly 20x, capturing 94.2%
  of rewards. Unlocked APR 2.95%, max-lock 58.93% (20:1 spread) at 208.03 GODL/day
  shared.

**Decision:** flatten the _effective_ reward weight to `w = balance × √multiplier`
(× NFT boost unchanged). Displayed multiplier stays exactly as stored. Expected
outcome at the measured distribution (fixed 208 GODL/day pot):

| lock        | displayed mult | effective weight | APR before → after  |
| ----------- | -------------- | ---------------- | ------------------- |
| unlocked    | 1.00x          | 1.000            | 2.95% → **12.79%**  |
| 1mo (30d)   | 1.78x          | 1.334            | 5.25% → 17.07%      |
| 3mo (90d)   | 3.34x          | 1.828            | 9.85% → 23.39%      |
| 6mo (180d)  | 5.68x          | 2.384            | 16.75% → 30.51%     |
| 12mo (365d) | 10.50x         | 3.240            | 30.94% → 41.46%     |
| 24mo (730d) | 20.00x         | 4.472            | 58.93% → **57.22%** |

Aggregate payout to stakers is unchanged (the pot is fixed); only the split moves.
Use this table (recomputed against live data) as the smoke-test oracle.

**Accounting drift — RESOLVED before this plan executes.** The morning measurement
found treasury.total_staked = 257,717,394,199,646,609 against a true weight sum of
259,251,240,615,198,832 — a ~1.53e15-unit deficit causing each bury to over-allocate
~0.6% (residual from the PDA-substitution exploit). The `ReconcileStakeV2` sweep
(commit 9f088a5 plus the pending CLI batching changes) has since repaired it.
Re-measured live later on 2026-08-05:

```
treasury.total_staked                        261,997,086,775,897,332
Σ true weight, all linear (v2 + v1)          261,997,086,775,897,332   drift = 0 (exact)
Σ true weight, all sqrt  (v2 + v1)            61,339,522,509,434,353   <- rebase target once all v1
stake versions                                v0 = 6,224   v1 = 0
```

The exact-to-the-unit match also validates Appendix A's replication of the on-chain
weight math (offsets, floor division, NFT-boost ordering). Consequence: the
`RebaseTotalStaked` instruction (§2.3) is no longer a solvency fix — the per-account
migration deltas alone should land total_staked on the true sqrt sum. Keep the
instruction as a post-sweep verification backstop; expect it to confirm drift 0 (or
correct dust at most).

---

## 2. Design

> **Implemented.** §2.1 in `api/src/state/stake_v2.rs` (`isqrt_u128`,
> `effective_multiplier`, plus a `StakeV2::migrate_weight` method holding the
> settle→flip→adjust sequence so handlers and tests share one implementation);
> §2.2 ditto (`weight_version` field, buffer 31→30, compile-time size assert,
> both born-v1 creation sites); §2.3 in
> `program/src/stake_v2/migrate_stake_weight.rs` and
> `program/src/admin/rebase_total_staked.rs` (+ `GodlError::RebaseMismatch = 15`,
> sdk builders, dispatcher arms); §2.4 in `cli/src/commands/stake_weight.rs`
> (`migrate-stakes`, `verify-stake-weights`, `rebase-total-staked`, each with
> `--dry-run` where applicable).

### 2.1 Effective weight function

```rust
// api/src/consts.rs — no new consts needed; curve is fixed sqrt.

// api/src/state/stake_v2.rs (or a shared helper module in api):
pub fn isqrt_u128(n: u128) -> u128 {
    if n == 0 { return 0; }
    let mut x = 1u128 << ((128 - n.leading_zeros()).div_ceil(2));
    loop {
        let y = (x + n / x) / 2;
        if y >= x { return x; }
        x = y;
    }
}

/// multiplier is scaled by STAKE_MULTIPLIER_SCALE (1e9).
/// Returns floor(sqrt(m)) in the same 1e9 fixed-point scale:
/// isqrt(m_scaled * 1e9) == 1e9 * sqrt(m).
pub fn effective_multiplier(multiplier: u64) -> u64 {
    isqrt_u128(multiplier as u128 * STAKE_MULTIPLIER_SCALE as u128) as u64
}
```

Anchor points (exact, must be asserted in tests): `effective_multiplier(1_000_000_000)
== 1_000_000_000`, `effective_multiplier(20_000_000_000) == 4_472_135_954`,
`effective_multiplier(0) == 0`. Monotone non-decreasing over the full u64 domain.
No floats — BPF-safe, few hundred CUs. Algorithm reviewed in-session: the initial
guess `1 << ceil(bit_length/2)` always upper-bounds the root so Newton descends
monotonically; the max input here is `u64::MAX × 1e9 < 2^94`, far from u128
overflow, and the u64 cast of the result is safe (`isqrt(2^94) < 2^47`). The tests
below remain mandatory.

### 2.2 Per-account weight versioning

The formula cannot flip globally in one deploy: `treasury.total_staked` must equal
Σ weighted_units at all times or the rewards-factor math misallocates. Use the
reserved buffer:

- `StakeV2.buffer[0]` becomes `weight_version: u8`. Existing accounts are
  zero-initialized → version 0 = linear (current behavior), version 1 = sqrt.
  Represent it properly: shrink `buffer` from today's `[u8; 31]` to `[u8; 30]` and
  add a `weight_version: u8` field before it. Layout-identical — verified against
  the live struct: `weight_version` lands at struct offset 161 (raw account offset
  169, after `is_nft_staked` at struct 160 / raw 168). The struct stays 192 bytes
  and the account 200 bytes (8-byte steel discriminant + struct); assert with
  `const _: () = assert!(core::mem::size_of::<StakeV2>() == 192);`.
- `weighted_units()` branches:

```rust
let m_eff = match self.weight_version {
    0 => self.multiplier,
    _ => effective_multiplier(self.multiplier),
};
```

NFT boost (×11/10) applies after, exactly as today. A mixed v0/v1 population is
always globally consistent because each account's contribution to `total_staked`
was recorded under its own version.

- **Both** StakeV2 creation sites set `weight_version = 1` so new stakes are sqrt
  from the moment the program ships:
  - `program/src/stake_v2/deposit_v2.rs` (~line 88, the `buffer = [0; 31]` init) —
    top-ups don't exist (`StakeAlreadyExists` guard), creation-only is sufficient;
  - `program/src/otc/execute_otc_trade.rs` (~line 194) — **the original plan missed
    this site.** OTC trades mint locked StakeV2 accounts; left at v0 they would keep
    creating linear-weight accounts after deploy and the sweep would never reach a
    stable zero-v0 end state. The compiler forces both files to change when `buffer`
    shrinks, but both born-v1 paths must be explicitly tested (§5.2).
- Legacy v1 `Stake` accounts are untouched (weight = balance = 1x anchor; `w(1)=1`).

### 2.3 New instructions (append-only; current max discriminant is 53)

**`MigrateStakeWeight = 54`** — permissionless, idempotent, one stake per call.
Follow the standard 4-part pattern (enum + Pod args struct + `instruction!` macro +
handler + dispatcher arm + `sdk.rs` builder; see CLAUDE.md in godl-program).

Handler (`program/src/stake_v2/migrate_stake_weight.rs`), order is load-bearing:

```
1. load stake (as_account_mut::<StakeV2>) + treasury (has_seeds [TREASURY])
2. if stake.weight_version != 0 → Ok(())            // idempotent no-op
3. stake.update_rewards(treasury)?                  // settle accrual at OLD weight
4. prev = stake.weighted_units()?                   // still version 0 → linear
5. stake.weight_version = 1
6. new = stake.weighted_units()?                    // sqrt
7. treasury.total_staked = total_staked − prev + new   (checked_sub/checked_add)
```

Deliberately do NOT enforce canonical PDA seeds (same reasoning as
`reconcile_stake_v2.rs`: the exploit-era on-curve accounts must be migratable too).
`as_account_mut` still enforces owner + discriminant, and the instruction moves no
tokens, so permissionless + seedless is safe. No signer requirement beyond fee payer.

Also add a `migrate_if_needed(stake, treasury)` helper (steps 2–7) and call it at
the top of the handlers that **already mutate `treasury.total_staked` in
production**: `withdraw_v2`, `compound_yield_v2`, `stake_nft`, `unstake_nft`.
Existing clients necessarily mark treasury writable for these (the ops would fail
today otherwise), so adding the migration write is meta-compatible. Accounts then
self-migrate on touch; the crank just accelerates the tail.

**Deliberately do NOT auto-migrate in `claim_yield_v2`** (revision from the original
plan). Claim is the only stake-settling handler that never writes treasury data
today (it only reads the factor and signs the payout transfer), and this repo's SDK
has **no** `claim_yield_v2` builder — the interface constructs that instruction
itself from the IDL, so we cannot verify from here that live claim transactions mark
treasury writable. If any client passes it readonly, an auto-migrate write would
make every claim from an unmigrated account fail at runtime. Claiming doesn't change
weight, so a v0 claimer settling at linear weight is fully consistent; the crank
migrates the account moments later anyway. Constraint to preserve and test: the
claim path must remain treasury-write-free (§5.2 readonly-meta regression test).
If the interface repo becomes available and its metas provably pass treasury
writable, this decision may be revisited — until then it stands.

**`RebaseTotalStaked = 55`** — admin-gated (config.admin, same gate as
`reconcile_stake_v2`), compare-and-swap semantics so it is race-safe against
concurrent staking traffic:

```
args: { expected: u64, new_value: u64 }
if treasury.total_staked != expected → GodlError (add variant, e.g. RebaseMismatch)
treasury.total_staked = new_value
sol_log both values
```

Run only after the sweep reports 100% of v2 accounts at version 1. `new_value` =
Σ weighted_units recomputed off-chain (Appendix A script, sqrt mode) + Σ v1 balances.
If a competing tx lands between compute and submit, the CAS fails; recompute and
retry.

Role revision: the pre-existing deficit this was meant to erase is already repaired
(drift measured exactly 0 — §1), and the per-account migration deltas keep the total
exact throughout the sweep. So rebase is now a **verification backstop**: expect
`expected == new_value` (submit anyway to assert on-chain, or skip the write when
they match — implementer's choice, but always log the comparison). Two hygiene
rules: (a) fetch the treasury (`expected`) and the getProgramAccounts snapshot
(`new_value`) at the **same slot** — plain value-equality CAS has a theoretical ABA
hole if interleaved deposits/withdraws happen to restore the old value between two
reads taken at different times; (b) the mixed-state true sum is per-account by
version (v0 linear, v1 sqrt) — Appendix A's all-or-nothing modes are only valid at
the endpoints.

### 2.4 CLI (godl-program/cli)

- `migrate-stakes` — getProgramAccounts (dataSize 200, discriminant 115), filter
  `weight_version == 0`, submit `MigrateStakeWeight` batched several per tx (every ix
  writes treasury, so txs serialize on the write lock — expect a sequential sweep of
  ~6.2k accounts; batch ~10 ix/tx, retry failures, loop until zero v0 accounts
  remain; print progress + final Σ check). Reuse the parallel batch-submission
  plumbing already added to `cli/src/transaction.rs` for the reconcile sweep
  (`send_and_confirm_transactions_in_parallel_blocking_v2` + the batching pattern in
  `cli/src/commands/admin.rs::reconcile_phantom_stakes`) — currently uncommitted;
  commit it before starting (Rollout step 0).
- `rebase-total-staked` — recomputes the true sum from getProgramAccounts (v2 by
  version + v1 balances), prints expected/new, submits the CAS instruction, retries
  on mismatch.
- `verify-stake-weights` — read-only audit: recompute Σ per version vs
  treasury.total_staked, print drift. Used pre/post rollout and by smoke tests.

> Robustness additions from the fork rehearsal: the shared parallel sender now
> uses `resign_txs_count: Some(10)` (same-account sweeps serialize on the
> treasury write lock, so queued txs can outlive their blockhash), and
> `migrate-stakes` treats a transport-level batch error as "retry next pass"
> instead of aborting — the sweep is idempotent and crash-resumable (proven
> mid-sweep on the fork). Also added `cli/examples/stake_smoke.rs`, the
> end-to-end deposit/migrate/withdraw/claim smoke used in §5.3 step 7 and
> reusable for §5.4's live check.

---

## 3. Off-chain surfaces (must ship with, or before, the crank)

> **DELEGATED (user decision, 2026-08-05):** this section is owned by a separate
> agent working in the `godl-interface` / `godl-indexer` repos. Its self-contained
> briefing — including every on-chain fact it needs (offsets, anchor values, exact
> weight math) and a report-back request on how the interface's `ClaimYieldV2` metas
> mark the treasury (which could let us revisit the §2.3 claim decision) — is in
> `INTERFACE-INDEXER-AGENT-PROMPT.md` at the repo root. The program-side work (§2)
> can be built and fully rehearsed (§5.3) in the meantime, but **must not be
> deployed to mainnet before this section ships** — the APR-display rule in the
> header is non-negotiable.

### 3.1 godl-interface

- `src/features/stake/ui/stake-utils.ts`:
  - add `effectiveMultiplier(multiplier: bigint, weightVersion: number): bigint`
    (BigInt isqrt — same floor semantics as on-chain; port the anchor-point tests).
  - `weightedUnits(...)` and `calculateStakeApr(...)` take the weight version and use
    the effective multiplier. `stakingPower` in `StakeDisplay` likewise.
  - Displayed multiplier badge/copy: unchanged.
- `src/features/stake/data-access/use-stake-items.ts` + generated client: expose
  `weightVersion` from account data. Preferred path: update `api/idl.json`
  (buffer → weightVersion u8 + buffer[30]) and regenerate the Codama client
  (`src/lib/program/client/generated`), same for the deposit lock-APR preview in
  `summary.tsx` / `stake-item.tsx`. Coordination detail (added in revision): the
  deposit preview is forward-looking — new deposits are born v1 only after the
  program upgrade (Rollout step 2), so the preview formula sits behind a
  config/env flag: linear at interface-deploy time, flipped to sqrt when the
  program upgrade lands. Per-account displays need no flag (they read each
  account's stored version).
- `src/lib/data/analytics.ts` `getStakeStats`: the SQL power aggregation must branch
  per row:
  `CASE WHEN weight_version = 0 THEN div(balance::numeric * multiplier, 1e9)
      ELSE div(balance::numeric * floor(sqrt(multiplier::numeric * 1e9)), 1e9) END`
  (floor(sqrt(numeric)) matches isqrt; add a test comparing SQL vs TS on the live
  multiplier set). While in there, fix the known pre-existing omission: this SQL
  ignores `is_nft_staked` (×1.1) — include the boost in the power sum.
- `/api/stake/stats` output shape unchanged; numbers will move on their own.

### 3.2 godl-indexer

- Index `weight_version` (byte at offset 169 of the raw account, i.e. first buffer
  byte after `is_nft_staked` at 168) into the `stake_v2` table + DB migration +
  drizzle schema bump in the interface. Backfill: existing rows default 0; the
  account-update stream corrects them as the crank touches each account.

---

## 4. Rollout sequence (mainnet)

0. **Housekeeping**: the working tree carries the pending CLI batching changes
   (`cli/src/commands/admin.rs`, `cli/src/transaction.rs`) plus this plan's
   implementation, all **uncommitted — the user reviews and commits everything
   themselves** (agent makes no commits, per the hard rule). Before rollout:
   re-run Appendix A and refresh every number in §1.
1. **Indexer + interface deploy** (version-aware, all accounts still v0 → zero
   visible change; verify `/api/stake/stats` unchanged within noise). Blocked on
   repo access — see §3.
2. **Program upgrade.** From this moment new deposits are sqrt-weighted (v1); old
   accounts keep linear until migrated. Mixed state is consistent by design.
3. **Crank sweep**: `migrate-stakes` until zero v0 accounts. APRs shown per-account
   correct themselves progressively.
4. **Rebase**: `verify-stake-weights` → `rebase-total-staked` (CAS). Post-condition:
   drift == 0.
5. **Smoke tests** (§5.4). Announce/monitor.

**Rollback story (understand before deploying):** version flips are one-way and the
old binary cannot read versioned accounts correctly — after any account is migrated,
reverting to the previous program binary would corrupt total_staked accounting. To
revert _economics_, ship a new binary whose v1 branch is linear again and re-crank.
Never `solana program deploy` the old binary after step 3 has started.

---

## 5. Testing (local devcontainer — the original plan's sandbox does not exist here)

### 5.1 Unit + property tests (host-target `cargo test`)

> **Implemented** as `#[cfg(test)] mod tests` in `api/src/state/stake_v2.rs`
> (15 tests, all green): anchors, isqrt floor-correctness/monotonicity over
> 100k seeded-xorshift samples plus power-of-two edges up to u128::MAX,
> layout-offset freeze (balance 40 / multiplier 104 / nft 160 / version 161,
> struct offsets), version branching, boost-after-curve (incl. a case where the
> wrong order provably differs), max-supply headroom, migrate settle-at-old-
> weight/idempotency/never-increases. Gap-closure additions (2026-08-06):
> StakeOverflow error path actually triggered (base and NFT-boost guards, both
> versions — Err, never panic); sub-1x multiplier migration through the
> weight-INCREASE branch (0.25x → 0.5x exact, settle still at old weight) plus
> zero-multiplier no-op; degenerate flag bytes (`weight_version`/`is_nft_staked`
> ∉ {0,1}) behave identically to canonical values and migrate leaves the byte
> untouched. Caveat: the pre-existing
> `round::tests::test_rent` is a debug leftover ending in `assert!(false)` — it
> fails on a pristine tree too; run with `-- --skip test_rent` (left untouched
> for the user to delete).

- `isqrt_u128` / `effective_multiplier`: exact anchor points (§2.1); proptest-style
  properties over random u64 multipliers — floor-correctness
  (`r*r <= n < (r+1)*(r+1)`), monotonicity, scale fixpoint at 1e9.
- `weighted_units` version branching, incl. NFT boost ordering (boost after curve)
  and overflow headroom (max supply 2.1M GODL × 1e11 grams × 20x fits u64; sqrt
  weights are strictly smaller).

### 5.2 Integration tests (`cargo test-sbf`, solana-program-test)

**Verified reality check: no harness exists.** Despite CLAUDE.md's description,
`program/` has no `tests/` directory and no `solana-program-test` dev-dependency —
the only test in the workspace is one `#[cfg(test)]` block in
`api/src/state/round.rs`. Scaffold from scratch: add `solana-program-test`,
`solana-sdk`, and `tokio` as dev-dependencies (2.3.x, matching Cargo.lock), create
`program/tests/`, and build fixtures that initialize config/treasury and mint GODL.
Budget real time for this — it is the largest single work item in §5.

> **Implemented** as `program/tests/sqrt_weight.rs` (13 tests, green under both
> the native processor and `cargo test-sbf` against the real BPF binary).
> Fixtures are crafted with `ProgramTest::add_account` (no deployer key needed).
> Notes from the build-out:
>
> - **Lockfile pins**: adding `solana-program-test = "2.3"` unified the
>   workspace's solana crates at 2.3.13 and pulled two edition-2024 crates the
>   platform-tools cargo (1.84) cannot parse. Pinned `blake3 = 1.8.2` and
>   `indexmap = 2.9.0` in Cargo.lock (`cargo update -p <crate> --precise ...`);
>   re-pin if a future `cargo update` bumps them while Agave 2.3.x is the
>   toolchain.
> - **OTC born-v1**: `ExecuteOtcTrade` requires the hardcoded OTC oracle's
>   signature, which no test environment can forge. The positive born-v1 test
>   exists but is env-gated on `OTC_ORACLE_KEYPAIR` (run it once with the real
>   oracle key); the oracle gate itself is tested negatively, and the born-v1
>   initializer is the same compiler-forced line as the DepositV2 site (which
>   is fully tested).
> - **NFT-op auto-migrate**: exercising `stake_nft`/`unstake_nft` end-to-end
>   needs an mpl-core fixture + real Core asset; not built. Covered instead by
>   the unit-level migrate tests with `is_nft_staked = 1`, crafted NFT-flagged
>   stakes in the integration storm, and the fact that the call site is the
>   same one-line pattern as withdraw/compound (which are tested end-to-end).

Cover at minimum:

- migrate: settles rewards at old weight (fund the factor, migrate, assert pending
  rewards equal linear-weight expectation, not sqrt), flips version, adjusts
  total_staked by exactly `new − prev`.
- idempotency: second migrate is a no-op (rewards, total_staked, version unchanged).
- auto-migrate on touch: withdraw/compound/stake_nft/unstake_nft on a v0 account
  migrates first; resulting totals match crank-then-op. (Claim intentionally
  excluded — see §2.3.)
- born-v1 at both creation sites: a fresh `DepositV2` stake and a fresh
  `ExecuteOtcTrade` stake both come out `weight_version == 1` with sqrt weight
  reflected in total_staked from birth.
- claim-path meta compatibility: `ClaimYieldV2` on a **v0** account with the
  treasury passed **readonly** in the metas succeeds and leaves the account v0 —
  the regression guard for the §2.3 decision (a treasury write sneaking into the
  claim path must fail this test).
- mixed-population invariant: interleave deposits (born v1), migrations, withdraws,
  claims, NFT ops across many accounts; after every step assert
  `treasury.total_staked == Σ weighted_units(all accounts)`.
- rebase: CAS success path; mismatch path errors and mutates nothing; admin gate
  (non-admin signer rejected).
- reward conservation: run several distribution cycles (simulate bury share via the
  same path prod uses), then claim everything from every account; assert Σ claimed
  ≤ Σ funded and residual dust is bounded (Numeric floor rounding only).

### 5.3 Surfpool hard simulation (mainnet fork with the REAL account set)

> **EXECUTED 2026-08-05 — all steps passed.** Fork at slot ~437,420,939
> (surfpool 1.5.0, upstream `api.mainnet-beta.solana.com`, headless
> `--no-tui --no-deploy --db :memory:`). Results:
>
> 1. Program overridden via the `surfnet_writeProgram(program_id, hex, offset)`
>    cheatcode (hex-encoded chunks); ELF sha256-verified inside the
>    programdata account (`scratchpad/override_program.py`).
> 2. Pre-check: `verify-stake-weights` on the fork — drift 0, v0=6224 (+896
>    legacy v1), total_staked 262,019,108,609,237,364.
> 3. **Full crank** (`migrate-stakes`, real CLI, 10 ix/tx): all 6,224 accounts
>    migrated across two runs — the first run died mid-sweep at 2,254 accounts
>    ("Blockhash not found": queued txs serializing on the treasury write lock
>    outlived their blockhash), which **proved crash-resume**: the rerun picked
>    up the remaining 3,970 with 0 failures. Fix folded into the CLI:
>    `resign_txs_count: Some(10)` in the parallel sender + pass-level tolerance
>    of transport errors (see §2.4 note below).
> 4. Post-sweep: drift **exactly 0**; total_staked landed on the all-sqrt sum
>    61,344,446,741,067,001 to the unit. Stored multipliers byte-identical to
>    mainnet for all 6,224 common accounts (fork-vs-upstream comparison).
> 5. `rebase-total-staked` (config.admin patched to a throwaway key via
>    `surfnet_setAccount`): CAS confirmed on-chain, expected == new, drift 0.
> 6. Solvency: vault 441,031 GODL vs staker claims 4,509 GODL (848 pending +
>    3,662 stored); headroom +162,924 GODL after miners' total_unclaimed
>    (273,389) and motherlode (209). No shortfall
>    (`scratchpad/solvency_audit.py`).
> 7. Behavior storm via `cli/examples/stake_smoke.rs` (reusable for the §5.4
>    live smoke test): deposits born v1 at 1x and 20x with exact weight deltas,
>    migrate idempotency on a fresh stake, withdraw delta exact, claim with
>    READONLY treasury meta succeeds. All checks passed; invariant held after.
> 8. Oracle: unlocked APR 12.38% / 24mo 55.36% vs the §1 table's 12.79% /
>    57.22% — both off by the same ~3.3%, exactly the growth of total sqrt
>    weight since the morning snapshot the table was computed from; curve shape
>    exact. Within drift tolerance.
>
> Caveat for re-runs: surfpool's `getProgramAccounts` proxies upstream AND
> overlays local writes (verified — the sweep loop terminates), and
> `surfnet_setTokenAccount(owner, mint, {amount})` funds ATAs for the storm.

Use surfpool (installed: 1.5.0 at `~/.local/bin`) to fork mainnet so the
migration is rehearsed against all ~6.2k live StakeV2 accounts, not synthetic
fixtures. Note: public mainnet RPC `getProgramAccounts` works from this
environment (verified — Appendix A ran successfully against
`api.mainnet-beta.solana.com`), so the fork's upstream can be the public
endpoint if no private RPC is provided. The scenario is:

1. `surfpool start` with an upstream mainnet RPC (lazy account forking). Pin a slot;
   record it in the test report.
2. Override the program: deploy the newly built `.so` at
   `mineWsRs2Rmw2jPMkVbgAbDjV1E23yQ8TEodaX3iza4` via surfpool's account/program
   cheatcodes (or set upgrade authority to a local key and upgrade normally).
3. Pre-snapshot: dump every StakeV2 + v1 Stake + treasury (Appendix A fetcher works
   against the surfnet RPC).
4. Run the real `migrate-stakes` crank end-to-end. Assert: zero v0 remaining; per
   account `rewards_after >= rewards_before`; version==1; multiplier bytes unchanged.
5. Run `rebase-total-staked`. Assert drift == 0 exactly.
6. Solvency audit: settle + claim max rewards for EVERY stake account (cheatcode the
   authorities' signatures or iterate with surfpool's signature-bypass if available;
   otherwise assert vault balance ≥ Σ(pending + stored rewards) computed off-chain).
   Report the vault headroom (the historical deficit is already repaired — expect
   no shortfall; finding one is a stop-ship signal).
7. Behavior storm on the fork: random deposits/withdraws/claims across migrated
   accounts + fresh ones; re-assert the Σ invariant after each batch.
8. Oracle check: recompute the §1 APR table from post-migration fork state; unlocked
   and 24mo tiers must match the sqrt predictions within data-drift tolerance.

### 5.4 Smoke tests (post-mainnet-deploy, read-only, host-allowed curl)

- `verify-stake-weights` (or Appendix A) against mainnet: drift == 0 after rebase.
- `https://godl.supply/api/stake/stats`: `totalStakePower` ≈ rebased value; recompute
  unlocked APR = shared/power×365 and compare to the UI.
- Sample 5–10 accounts across versions/multipliers: on-chain weighted_units vs
  interface `stakingPower` vs DB row agree.
- One live end-to-end: small unlocked deposit → confirm born v1, APR display sane,
  withdraw works.

### 5.5 Fuzzing

- `cargo-fuzz` (or test-sbf-driven randomized harness if fuzz tooling fights the BPF
  toolchain) on: (a) `MigrateStakeWeight` with adversarial account inputs — wrong
  owner, wrong discriminant, treasury substituted, exploit-style on-curve stake
  addresses, truncated data; only the owner+discriminant-valid path may mutate
  state. (b) instruction-data parsing for both new instructions (arbitrary bytes must
  error cleanly, never panic). (c) op-sequence fuzzer: random valid sequences of
  {deposit, withdraw, claim, compound, migrate, nft ops, distribute} on a small
  account set with the Σ-invariant and reward-conservation checks as the oracle —
  this is the highest-value target; run long (≥1e5 sequences) in CI or overnight.

> **Implemented** in `fuzz/` (standalone cargo-fuzz crate, excluded from the
> workspace since it needs nightly; `overflow-checks = true` to match the
> program profile). Targets: `instruction_parsing` (b) and `op_sequence` (c) —
> the latter drives the REAL api-crate methods with tx-rollback semantics
> (failed ops applied to a scratch copy and discarded) and both oracles checked
> after every op. First bounded run 2026-08-05: 2,000,000 parsing runs and
> 5,632,801 op-sequence runs (120s), zero panics / zero invariant violations.
> Run longer with `cargo +nightly fuzz run op_sequence -- -max_total_time=3600`.
> Target (a) is covered deterministically by the §5.2
> `migrate_rejects_invalid_accounts` test (wrong discriminant, foreign owner,
> substituted treasury) plus the seedless/on-curve test.

---

## 6. Acceptance criteria

- [x] All §5.1/5.2 tests green locally (`cargo test`, `cargo test-sbf`): 12 unit
      + 13 integration tests pass (native and BPF). Readonly-treasury
      `ClaimYieldV2` regression included. OTC born-v1: negative oracle-gate test
      green; the positive test is env-gated on `OTC_ORACLE_KEYPAIR` (§5.2 note)
      — **user: run it once with the real oracle key before deploy.**
- [x] Surfpool fork run 2026-08-05: full crank (6,224/6,224) + rebase CAS
      completed; drift 0 at every checkpoint; §1 oracle table holds within
      data-drift tolerance; solvency audit shows +162,924 GODL headroom (see
      the §5.3 results block — attach to the PR).
- [x] Fuzz targets run with zero panics/invariant violations (2.0M parsing runs;
      5.63M op-sequence runs — first bounded session; run longer before deploy).
- [x] Stored multipliers byte-identical pre/post migration: fork-vs-mainnet
      comparison over all 6,224 accounts — 0 mismatches.
- [x] No instruction renumbering (54/55 appended; enum cursor now 55); StakeV2
      stays 192-byte struct / 200-byte account (compile-time assert + layout
      test); treasury layout untouched.
- [ ] Interface: every APR figure uses effective weight; multiplier badge unchanged;
      SQL/TS/on-chain weight parity test green; NFT boost included in stats SQL.
      (Delegated — §3.)
- [ ] Indexer captures weight_version; DB migration applied. (Delegated — §3.)
- [ ] Post-deploy smoke tests documented with actual numbers. (Deploy-time; use
      `verify-stake-weights` + `cli/examples/stake_smoke.rs`.)
- [x] Post-sweep `RebaseTotalStaked` comparison logged on the fork: expected ==
      recomputed true sqrt sum == 61,344,446,741,067,001 (drift 0). (Repeat on
      mainnet at rollout step 4.)
- [x] No commits made by the agent in this workspace (user directive) — all
      changes delivered in the working tree; the user commits and opens the PR
      under their own identity. (The original plan's "sign as bootapollo" item is
      thereby the user's concern, not the agent's.)

---

## Appendix A — measurement script (rerun at execution time)

Pure-stdlib Python; run against mainnet RPC or a surfnet fork. Prints treasury vs
true weight sums in both linear and sqrt modes; use it for the pre-check, the rebase
`new_value`, and smoke tests.

Validation status (2026-08-05, this environment): ran successfully against live
mainnet via the public RPC; every hardcoded offset was independently cross-checked
against the source structs (StakeV2: balance 48, multiplier 112, nft 168, version
169; Treasury: total_staked 56; v1 Stake: balance 40, dataSize 112); the treasury
PDA `5epGzdW6veQwLQiQs1L45uUQ8jdSLQHWL8RbC7uTWVY3` was re-derived from seeds
(bump 254); and the all-linear sum matched on-chain total_staked to the exact unit.

```python
#!/usr/bin/env python3
import base64, json, math, urllib.request

RPC = "https://api.mainnet-beta.solana.com"  # or surfnet URL
PROGRAM = "mineWsRs2Rmw2jPMkVbgAbDjV1E23yQ8TEodaX3iza4"
TREASURY_PDA = "5epGzdW6veQwLQiQs1L45uUQ8jdSLQHWL8RbC7uTWVY3"
SCALE = 1_000_000_000

def rpc(method, params):
    req = urllib.request.Request(RPC, json.dumps(
        {"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode(),
        {"Content-Type": "application/json"})
    return json.load(urllib.request.urlopen(req))["result"]

def isqrt_scaled(mult):  # matches on-chain effective_multiplier
    return math.isqrt(mult * SCALE)

tre = base64.b64decode(rpc("getAccountInfo", [TREASURY_PDA, {"encoding": "base64"}])["value"]["data"][0])
total_staked = int.from_bytes(tre[56:64], "little")

def accounts(size):
    return rpc("getProgramAccounts", [PROGRAM, {"encoding": "base64", "filters": [{"dataSize": size}]}])

lin = sq = 0
n_v0 = n_v1 = 0
for a in accounts(200):                      # StakeV2
    d = base64.b64decode(a["account"]["data"][0])
    if d[0] != 115: continue
    bal = int.from_bytes(d[48:56], "little")
    mult = int.from_bytes(d[112:120], "little")
    nft = d[168]; ver = d[169]               # weight_version = first buffer byte
    for mode, m_eff in (("lin", mult), ("sq", isqrt_scaled(mult))):
        w = bal * m_eff // SCALE
        if nft: w = w * 11 // 10
        if mode == "lin": lin += w
        else: sq += w
    n_v0 += ver == 0; n_v1 += ver != 0

v1_bal = 0
for a in accounts(112):                      # legacy v1 Stake
    d = base64.b64decode(a["account"]["data"][0])
    if d[0] != 108: continue
    v1_bal += int.from_bytes(d[40:48], "little")

print(f"treasury.total_staked : {total_staked:,}")
print(f"Σ true (all linear)   : {lin + v1_bal:,}   drift {total_staked - lin - v1_bal:,}")
print(f"Σ true (all sqrt)     : {sq + v1_bal:,}   <- rebase target once all v1")
print(f"stake versions        : v0={n_v0}  v1={n_v1}")
```

For the mixed mid-sweep state, the true sum is per-account by version (adapt the
loop accordingly — `verify-stake-weights` in the CLI should implement exactly that).
