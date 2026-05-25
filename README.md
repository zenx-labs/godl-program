# GODL

GODL is a crypto mining protocol.

## API

- [`Consts`](api/src/consts.rs) – Program constants.
- [`Error`](api/src/error.rs) – Custom program errors.
- [`Event`](api/src/event.rs) – Custom program events.
- [`Instruction`](api/src/instruction.rs) – Declared instructions and arguments.

## Instructions

#### Mining

- [`Initialize`](program/src/initialize.rs) - Initializes program variables.
- [`Automate`](program/src/automation/automate.rs) - Configures automation with pooled deployment metadata.
- [`FundAutomation`](program/src/automation/fund_automation.rs) - Funds an automation account.
- [`ClaimSOLAndFundAutomation`](program/src/automation/claim_sol_and_fund_automation.rs) - Claims SOL rewards and funds automation in one step.
- [`Checkpoint`](program/src/miner/checkpoint.rs) - Checkpoints rewards with pooled top-miner sharing.
- [`ClaimGODL`](program/src/miner/claim_godl.rs) - Claims GODL mining rewards.
- [`ClaimSOL`](program/src/miner/claim_sol.rs) - Claims SOL mining rewards.
- ~~[`Close`](program/src/miner/close.rs) - Deprecated. Use `Close`.~~
- [`Close`](program/src/miner/close.rs) - Closes an expired round and its pool round, returning rent to the payer.
- ~~[`DeployV2`](program/src/miner/deploy_v2.rs) - Deprecated. Use `Deploy`.~~
- [`Deploy`](program/src/miner/deploy.rs) – Deploys SOL with optional pooling metadata.
- [`InjectGodlMotherlode`](program/src/miner/inject_godl_motherlode.rs) - Injects GODL into the motherlode rewards pool.
- [`Log`](program/src/miner/log.rs) – Logs non-truncatable event data.
- ~~[`ResetV2`](program/src/miner/reset_v2.rs) - Deprecated. Use `Reset`.~~
- [`Reset`](program/src/miner/reset.rs) - Resets the board and flags pooled top-miner wins.

#### Referral

- [`InitializeReferrer`](program/src/referral/initialize_referrer.rs) - Initializes a referrer account.
- [`SetReferrer`](program/src/referral/set_referrer.rs) - Sets a referrer for a miner.
- [`ClaimReferral`](program/src/referral/claim_referral.rs) - Claims accrued referral rewards.

#### Staking

- [`Deposit`](program/src/stake/deposit.rs) - Deposits GODL into a stake account.
- [`Withdraw`](program/src/stake/withdraw.rs) - Withdraws GODL from a stake account.
- [`ClaimYield`](program/src/stake/claim_yield.rs) - Claims staking yield.

#### Staking V2

- [`DepositV2`](program/src/stake_v2/deposit_v2.rs) - Deposits GODL into a stake_v2 position with optional lock multiplier.
- [`WithdrawV2`](program/src/stake_v2/withdraw_v2.rs) - Withdraws unlocked GODL from a stake_v2 position.
- [`ClaimYieldV2`](program/src/stake_v2/claim_yield_v2.rs) - Claims accrued GODL rewards from a stake_v2 position.
- [`CompoundYieldV2`](program/src/stake_v2/compound_yield_v2.rs) - Claims rewards and redeposits them into the same stake_v2 position.
- [`SetStakeExecutorV2`](program/src/stake_v2/set_executor_v2.rs) - Updates the delegated executor for a stake_v2 position.
- [`StakeNft`](program/src/stake_v2/stake_nft.rs) - Stakes a Metaplex Core NFT to a stake_v2 account for a 10% weight boost.
- [`UnstakeNft`](program/src/stake_v2/unstake_nft.rs) - Unstakes a Metaplex Core NFT from a stake_v2 account.

#### Admin

- [`Bury`](program/src/admin/bury.rs) - Swaps vaulted SOL to GODL, shares with stakers/admin, and burns the rest.
- [`BuryTokens`](program/src/admin/bury_tokens.rs) - Shares GODL with stakers and admin, then burns the rest (manual bury).
- [`InjectUnrefinedRewards`](program/src/admin/inject_unrefined_rewards.rs) - Adds unrefined GODL rewards to a miner on behalf of the bury authority.
- [`InitializeSolMotherlode`](program/src/admin/initialize_sol_motherlode.rs) - Initializes the SOL motherlode account.
- [`NewVar`](program/src/admin/new_var.rs) - Creates a new entropy variable account.
- [`PreBury`](program/src/admin/pre_bury.rs) - Prepares a bury transaction.
- [`SetAdmin`](program/src/admin/set_admin.rs) - Re-assigns the admin authority.
- [`SetFeeCollector`](program/src/admin/set_fee_collector.rs) - Updates the fee collection address.
- [`SetGodlPerRound`](program/src/admin/set_godl_per_round.rs) - Updates the GODL minted per round.
- [`SetMotherlodeDenominator`](program/src/admin/set_motherlode_denominator.rs) - Updates the motherlode payout denominator.
- [`SetSwapProgram`](program/src/admin/set_swap_program.rs) - Updates the configured swap program.
- [`SetVarAddress`](program/src/admin/set_var_address.rs) - Updates the entropy variable address.
- [`WithdrawVault`](program/src/admin/withdraw_vault.rs) - Withdraws SOL from the treasury vault.

## State

- [`Automation`](api/src/state/automation.rs) - Tracks automation configs.
- [`AutomationV2`](api/src/state/automation_v2.rs) - Packed automation configuration for v2 executors.
- [`Board`](api/src/state/board.rs) - Tracks the current round number and timestamps.
- [`Config`](api/src/state/config.rs) - Global program configs.
- [`Miner`](api/src/state/miner.rs) - Tracks a miner's game state.
- [`PoolMember`](api/src/state/pool_member.rs) - Records an authority's pooled deployments per round.
- [`PoolRound`](api/src/state/pool_round.rs) - Aggregates pooled SOL per square for a round.
- [`Referrer`](api/src/state/referrer.rs) - Tracks a referrer's reward balances.
- [`Round`](api/src/state/round.rs) - Tracks the game state of a given round.
- [`SolMotherlode`](api/src/state/sol_motherlode.rs) - Tracks SOL allocated to the motherlode rewards pool.
- [`Stake`](api/src/state/stake.rs) - Manages a user's staking activity.
- [`StakeV2`](api/src/state/stake_v2.rs) - State for a lockable stake with multiplier and delegated executor.
- [`Treasury`](api/src/state/treasury.rs) - Mints, burns, and escrows GODL tokens.

## Mining Pool

- Use [`Automate`](program/src/automation/automate.rs) to configure automation strategies that deploy via the pool without manual toggles.
- [`Deploy`](program/src/miner/deploy.rs) (exposed via the CLI `--pooled` flag) opts miners into the shared pool for a round and provisions the `PoolMember`/`PoolRound` accounts.
- [`Checkpoint`](program/src/miner/checkpoint.rs) distributes non-split top-miner rewards proportionally between pooled members whenever `Reset` flagged the round as a pool win.
- [`Reset`](program/src/miner/reset.rs) verifies the provided top miner and corresponding pool membership before marking `round.top_miner = POOL_ADDRESS`, enabling proportional payouts.

## CLI

- `close-rounds` – Closes expired round accounts that belong to the current payer, batching up to 20 closures per transaction to reclaim rent efficiently.

## Tests

To run the test suite, use the Solana toolchain:

```
cargo test-sbf
```

For line coverage, use llvm-cov:

```
cargo llvm-cov
```
