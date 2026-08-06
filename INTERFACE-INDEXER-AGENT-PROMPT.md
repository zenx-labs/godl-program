# Task: godl-interface + godl-indexer changes for the stake sqrt-weight reweight

You are working ONLY on the `godl-interface` and `godl-indexer` repos. The on-chain
program and CLI changes are owned by a different agent in a different workspace — do
not touch program code, and do not deploy anything. Deliverable: one PR per repo
(leave both unmerged/undeployed; the human coordinates rollout), plus the findings
report described at the end.

## Context (all facts verified on-chain 2026-08-05)

GODL (Solana program `mineWsRs2Rmw2jPMkVbgAbDjV1E23yQ8TEodaX3iza4`) is changing how
staking rewards are split. Today each StakeV2 account's reward weight is
`balance × multiplier (× 1.1 if NFT staked)` with `multiplier` linear 1x→20x by lock
duration. The program is moving to `balance × √multiplier (× 1.1 NFT)` — the stored
`multiplier` field and the displayed 1x–20x badge DO NOT change; only the internal
weight (and therefore every APR) changes.

The transition is per-account: a new `weight_version` byte on each StakeV2 account
(0 = linear = all existing accounts today; 1 = sqrt). After the program upgrade, new
stakes are born version 1 and a permissionless crank migrates the ~6.2k existing
accounts within hours. Your surfaces must be version-aware BEFORE the program
upgrade happens: while every account is still version 0, your deployed changes must
produce zero visible difference.

## On-chain data facts you need

- StakeV2 accounts: owner = the program, account data size exactly **200 bytes**,
  first byte (steel discriminant) = **115**. Field offsets in the raw account data
  (all integers little-endian): `balance` u64 @ **48**, `multiplier` u64 @ **112**
  (fixed-point, scale 1e9: 1x = 1_000_000_000, 20x = 20_000_000_000),
  `is_nft_staked` u8 @ **168**, `weight_version` u8 @ **169** (today always 0).
- Legacy v1 Stake accounts (data size 112, discriminant 108, balance u64 @ 40) have
  weight = balance and are unaffected — no version byte, no change for them.
- Effective multiplier (fixed-point, matches the on-chain integer math exactly):
  - version 0: `m_eff = multiplier`
  - version 1: `m_eff = isqrt(multiplier * 10^9)` — floor integer sqrt, result in
    the same 1e9 scale.
  - weight = `balance * m_eff / 10^9` (floor division), then `* 11 / 10` (floor) if
    NFT staked — boost applied AFTER the curve, this order matters.
- Exact anchor values your tests MUST assert: `eff(1_000_000_000) = 1_000_000_000`,
  `eff(20_000_000_000) = 4_472_135_954`, `eff(0) = 0`.
- Reference implementation of the sqrt in Python: `math.isqrt(mult * 10**9)`. In
  TypeScript use a BigInt Newton isqrt with floor semantics — no floating point
  anywhere in weight math.

## godl-interface tasks

1. `src/features/stake/ui/stake-utils.ts`:
   - add `effectiveMultiplier(multiplier: bigint, weightVersion: number): bigint`
     (BigInt isqrt, floor; unit-test the anchor points above).
   - `weightedUnits(...)` and `calculateStakeApr(...)` must take the account's
     weight version and use the effective multiplier. Same for `stakingPower` in
     `StakeDisplay`.
   - The multiplier badge / "20x" copy: UNCHANGED. Only APR/weight figures move.
2. `src/features/stake/data-access/use-stake-items.ts` + generated client: expose
   `weightVersion`. Preferred path: update `api/idl.json` (StakeV2 `buffer: u8[31]`
   → `weightVersion: u8` + `buffer: u8[30]`) and regenerate the Codama client in
   `src/lib/program/client/generated`.
3. Deposit lock-APR preview (`summary.tsx` / `stake-item.tsx`): deposits made AFTER
   the program upgrade are born version 1 (sqrt), but until that upgrade new
   deposits are still linear. Put the preview's formula behind a config/env flag:
   default linear at your deploy time, flipped to sqrt when the program upgrade
   lands. Per-account displays need no flag — they read each account's own version.
4. `src/lib/data/analytics.ts` `getStakeStats`: the SQL power aggregation must
   branch per row:
   `CASE WHEN weight_version = 0 THEN div(balance::numeric * multiplier, 1e9)
        ELSE div(balance::numeric * floor(sqrt(multiplier::numeric * 1e9)), 1e9) END`
   Add a parity test comparing SQL output vs the TS `effectiveMultiplier` on the
   live multiplier set (floor(sqrt(numeric)) must match BigInt isqrt exactly; if the
   DB's numeric sqrt can't guarantee floor-exactness at 19-digit inputs, compute the
   effective multiplier in code instead of SQL). While in this file, fix the known
   pre-existing bug: the SQL ignores `is_nft_staked` — include the ×1.1 boost in the
   power sum (after the curve).
5. `/api/stake/stats` response shape: UNCHANGED. Numbers move on their own.

## godl-indexer tasks

- Index the byte at offset **169** of StakeV2 account data as a new
  `weight_version` column on the `stake_v2` table (byte at 168 is `is_nft_staked`,
  which you likely already index). DB migration + drizzle schema bump consumed by
  the interface. Backfill: existing rows default 0 — correct, since every live
  account is version 0 until the crank runs; the account-update stream fixes each
  row as it migrates.

## Hard rules

- Every APR figure shown anywhere must be computed from the version-aware effective
  weight. Showing an APR derived from the raw multiplier once sqrt is live would
  overstate locked yield ~4x. Non-negotiable.
- Do not change the displayed multiplier value or its copy.
- Do not change any instruction construction, account metas, or transaction logic.
- Zero visible change while all accounts are version 0 (which is the state at your
  deploy time) — that is the correctness bar for your PR.

## Report back (as important as the PRs)

1. In the interface's transaction-building code, find how the **ClaimYieldV2**
   instruction's account metas mark the **treasury** account
   (`5epGzdW6veQwLQiQs1L45uUQ8jdSLQHWL8RbC7uTWVY3`): writable or readonly? Quote the
   exact code. The program team excluded claim from auto-migration because this was
   unverifiable from their side; your answer may let them revisit it. Do NOT change
   the metas either way.
2. Confirm which byte offsets your indexer actually uses for StakeV2 parsing (guard
   against a drifted local IDL).
3. List every UI location that renders an APR or staking-power figure, and confirm
   each one routes through the version-aware helpers after your change.
