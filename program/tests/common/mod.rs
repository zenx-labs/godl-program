//! Shared integration-test fixtures: crafted accounts, the `EnvBuilder`
//! environment, submit/assert helpers, and hand-rolled StakeV2 instruction
//! builders. Used by `sqrt_weight.rs` and `stake_lifecycle.rs`.
//!
//! Fixtures are crafted directly with `ProgramTest::add_account` rather than
//! driven through `Initialize` (which requires the mainnet deployer key).
#![allow(dead_code)]

use godl_api::prelude::*;
use solana_program_test::{processor, BanksClientError, ProgramTest, ProgramTestContext};
use solana_sdk::{
    account::Account,
    instruction::InstructionError,
    program_option::COption,
    program_pack::Pack,
    signature::{Keypair, Signer},
    transaction::{Transaction, TransactionError},
};
use spl_associated_token_account::get_associated_token_address;
use steel::*;

pub const GODL: u64 = ONE_GODL; // 1e11 grams
pub const SQRT20: u128 = 4_472_135_954; // effective_multiplier(20 * SCALE)
pub const SCALE: u64 = STAKE_MULTIPLIER_SCALE;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

pub fn pod_account<T: Discriminator + Pod>(value: &T) -> Account {
    let mut data = vec![0u8; 8 + core::mem::size_of::<T>()];
    data[0] = T::discriminator();
    data[8..].copy_from_slice(bytemuck::bytes_of(value));
    Account {
        lamports: 10_000_000_000,
        data,
        owner: godl_api::ID,
        executable: false,
        rent_epoch: 0,
    }
}

pub fn token_account(owner: &Pubkey, amount: u64) -> Account {
    let mut data = vec![0u8; spl_token::state::Account::LEN];
    spl_token::state::Account {
        mint: MINT_ADDRESS,
        owner: *owner,
        amount,
        delegate: COption::None,
        state: spl_token::state::AccountState::Initialized,
        is_native: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    }
    .pack_into_slice(&mut data);
    Account {
        lamports: 10_000_000_000,
        data,
        owner: spl_token::ID,
        executable: false,
        rent_epoch: 0,
    }
}

pub fn mint_account(supply: u64) -> Account {
    let mut data = vec![0u8; spl_token::state::Mint::LEN];
    spl_token::state::Mint {
        mint_authority: COption::Some(treasury_pda().0),
        supply,
        decimals: TOKEN_DECIMALS,
        is_initialized: true,
        freeze_authority: COption::None,
    }
    .pack_into_slice(&mut data);
    Account {
        lamports: 10_000_000_000,
        data,
        owner: spl_token::ID,
        executable: false,
        rent_epoch: 0,
    }
}

pub fn system_account(lamports: u64) -> Account {
    Account {
        lamports,
        data: vec![],
        owner: solana_sdk::system_program::ID,
        executable: false,
        rent_epoch: 0,
    }
}

#[derive(Clone, Copy)]
pub struct StakeSpec {
    pub authority: Pubkey,
    pub id: u64,
    pub balance: u64,
    pub multiplier: u64,
    pub nft: bool,
    pub version: u8,
    pub created_at: i64,
    pub lock_duration: i64,
    pub rewards: u64,
    pub no_vault: bool,
}

impl StakeSpec {
    pub fn address(&self) -> Pubkey {
        stake_v2_pda(self.authority, self.id).0
    }

    pub fn state(&self) -> StakeV2 {
        let mut s = StakeV2::zeroed();
        s.id = self.id;
        s.authority = self.authority;
        s.balance = self.balance;
        s.multiplier = self.multiplier;
        s.executor = self.authority;
        s.is_nft_staked = self.nft as u8;
        s.weight_version = self.version;
        s.created_at = self.created_at;
        s.lock_duration = self.lock_duration;
        s.rewards = self.rewards;
        s
    }

    pub fn units(&self) -> u64 {
        self.state().weighted_units().unwrap()
    }

    /// Lock crafting without clock-warp: the ProgramTest clock sits near real
    /// time, so `created_at = 0` plus a small duration is an EXPIRED lock,
    /// while a huge duration is an ACTIVE lock.
    pub fn with_lock(mut self, created_at: i64, lock_duration: i64) -> Self {
        self.created_at = created_at;
        self.lock_duration = lock_duration;
        self
    }

    /// Pre-seeded claimable rewards. Needed for balance-0 stakes, which accrue
    /// nothing from a rewards-factor delta (weighted units are zero).
    pub fn with_rewards(mut self, rewards: u64) -> Self {
        self.rewards = rewards;
        self
    }

    /// Skip creating the stake's vault ATA (drained-and-closed vault shape).
    pub fn without_vault(mut self) -> Self {
        self.no_vault = true;
        self
    }
}

pub struct EnvBuilder {
    admin: Pubkey,
    stakes: Vec<(Pubkey, StakeSpec)>,
    stake_rewards_factor: Numeric,
    treasury_godl: u64,
    funded: Vec<Pubkey>,
    token_accounts: Vec<(Pubkey, u64)>,
    extra_accounts: Vec<(Pubkey, Account)>,
}

impl EnvBuilder {
    pub fn new(admin: Pubkey) -> Self {
        Self {
            admin,
            stakes: vec![],
            stake_rewards_factor: Numeric::ZERO,
            treasury_godl: 1_000_000 * GODL,
            funded: vec![admin],
            token_accounts: vec![],
            extra_accounts: vec![],
        }
    }

    pub fn stake(mut self, spec: StakeSpec) -> Self {
        self.stakes.push((spec.address(), spec));
        self
    }

    /// A stake at an arbitrary (non-canonical) address — the exploit-era
    /// on-curve account shape.
    pub fn stake_at(mut self, address: Pubkey, spec: StakeSpec) -> Self {
        self.stakes.push((address, spec));
        self
    }

    pub fn rewards_factor(mut self, f: Numeric) -> Self {
        self.stake_rewards_factor = f;
        self
    }

    pub fn fund(mut self, k: Pubkey) -> Self {
        self.funded.push(k);
        self
    }

    pub fn token(mut self, owner: Pubkey, amount: u64) -> Self {
        self.token_accounts.push((owner, amount));
        self
    }

    pub fn account(mut self, addr: Pubkey, acc: Account) -> Self {
        self.extra_accounts.push((addr, acc));
        self
    }

    pub async fn start(self) -> ProgramTestContext {
        let mut pt = ProgramTest::new("godl", godl_api::ID, processor!(godl::process_instruction));

        let mut config = Config::zeroed();
        config.admin = self.admin;
        config.bury_authority = self.admin;
        config.fee_collector = self.admin;
        pt.add_account(config_pda().0, pod_account(&config));
        pt.add_account(board_pda().0, pod_account(&Board::zeroed()));

        let total: u128 = self.stakes.iter().map(|(_, s)| s.units() as u128).sum();
        let mut treasury = Treasury::zeroed();
        treasury.total_staked = u64::try_from(total).unwrap();
        treasury.stake_rewards_factor = self.stake_rewards_factor;
        let treasury_address = treasury_pda().0;
        pt.add_account(treasury_address, pod_account(&treasury));

        pt.add_account(MINT_ADDRESS, mint_account(2_000_000 * GODL));
        pt.add_account(
            get_associated_token_address(&treasury_address, &MINT_ADDRESS),
            token_account(&treasury_address, self.treasury_godl),
        );
        // bury_tokens validates the admin fee ATA even when ADMIN_BPS is 0.
        pt.add_account(
            get_associated_token_address(&ADMIN_GODL_FEE, &MINT_ADDRESS),
            token_account(&ADMIN_GODL_FEE, 0),
        );

        for (address, spec) in &self.stakes {
            pt.add_account(*address, pod_account(&spec.state()));
            if !spec.no_vault {
                pt.add_account(
                    get_associated_token_address(address, &MINT_ADDRESS),
                    token_account(address, spec.balance),
                );
            }
            pt.add_account(spec.authority, system_account(100_000_000_000));
        }
        for k in &self.funded {
            pt.add_account(*k, system_account(100_000_000_000));
        }
        for (owner, amount) in &self.token_accounts {
            pt.add_account(
                get_associated_token_address(owner, &MINT_ADDRESS),
                token_account(owner, *amount),
            );
        }
        // Added last so tests can override builder-created accounts (e.g. seed
        // residual tokens into a stake's vault).
        for (addr, acc) in self.extra_accounts {
            pt.add_account(addr, acc);
        }

        pt.start_with_context().await
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub async fn send(
    ctx: &mut ProgramTestContext,
    signers: &[&Keypair],
    ixs: &[Instruction],
) -> Result<(), TransactionError> {
    let blockhash = ctx.get_new_latest_blockhash().await.unwrap();
    let tx =
        Transaction::new_signed_with_payer(ixs, Some(&signers[0].pubkey()), signers, blockhash);
    ctx.banks_client
        .process_transaction(tx)
        .await
        .map_err(|e| match e {
            BanksClientError::TransactionError(t) => t,
            BanksClientError::SimulationError { err, .. } => err,
            other => panic!("unexpected banks client error: {other:?}"),
        })
}

pub async fn get<T: AccountDeserialize + Copy>(ctx: &mut ProgramTestContext, addr: Pubkey) -> T {
    let acc = ctx
        .banks_client
        .get_account(addr)
        .await
        .unwrap()
        .unwrap_or_else(|| panic!("account {addr} not found"));
    *T::try_from_bytes(&acc.data).unwrap()
}

pub async fn token_balance(ctx: &mut ProgramTestContext, addr: Pubkey) -> u64 {
    match ctx.banks_client.get_account(addr).await.unwrap() {
        Some(acc) => spl_token::state::Account::unpack(&acc.data).unwrap().amount,
        None => 0,
    }
}

pub async fn lamports(ctx: &mut ProgramTestContext, addr: Pubkey) -> u64 {
    match ctx.banks_client.get_account(addr).await.unwrap() {
        Some(acc) => acc.lamports,
        None => 0,
    }
}

pub async fn account_exists(ctx: &mut ProgramTestContext, addr: Pubkey) -> bool {
    ctx.banks_client.get_account(addr).await.unwrap().is_some()
}

/// The load-bearing invariant: treasury.total_staked == Σ weighted_units over
/// every stake account (all of which the caller must list).
pub async fn assert_invariant(ctx: &mut ProgramTestContext, stake_addrs: &[Pubkey]) {
    let treasury: Treasury = get(ctx, treasury_pda().0).await;
    let mut sum: u128 = 0;
    for addr in stake_addrs {
        let s: StakeV2 = get(ctx, *addr).await;
        sum += s.weighted_units().unwrap() as u128;
    }
    assert_eq!(
        treasury.total_staked as u128, sum,
        "total_staked invariant violated"
    );
}

pub fn assert_custom_err(result: Result<(), TransactionError>, code: u32) {
    match result {
        Err(TransactionError::InstructionError(_, InstructionError::Custom(c))) if c == code => {}
        other => panic!("expected Custom({code}), got {other:?}"),
    }
}

// The interface builds StakeV2 instructions from the IDL (this repo's SDK has
// no builders for them), so the tests construct the metas by hand, in the
// exact order the handlers destructure.

pub fn ix_deposit_v2(signer: Pubkey, id: u64, amount: u64, lock_duration: i64) -> Instruction {
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

pub fn ix_withdraw_v2(signer: Pubkey, id: u64, amount: u64) -> Instruction {
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

pub fn ix_claim_yield_v2(
    signer: Pubkey,
    id: u64,
    amount: u64,
    treasury_writable: bool,
) -> Instruction {
    let stake = stake_v2_pda(signer, id).0;
    let treasury = treasury_pda().0;
    let treasury_meta = if treasury_writable {
        AccountMeta::new(treasury, false)
    } else {
        AccountMeta::new_readonly(treasury, false)
    };
    Instruction {
        program_id: godl_api::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new_readonly(MINT_ADDRESS, false),
            AccountMeta::new(get_associated_token_address(&signer, &MINT_ADDRESS), false),
            AccountMeta::new(stake, false),
            treasury_meta,
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

pub fn spec(
    authority: Pubkey,
    id: u64,
    balance: u64,
    multiplier: u64,
    nft: bool,
    version: u8,
) -> StakeSpec {
    StakeSpec {
        authority,
        id,
        balance,
        multiplier,
        nft,
        version,
        created_at: 0,
        lock_duration: 0,
        rewards: 0,
        no_vault: false,
    }
}
