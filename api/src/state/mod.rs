mod automation;
mod automation_v2;
mod board;
mod config;
mod miner;
mod pool_member;
mod pool_round;
mod referrer;
mod round;
mod sol_motherlode;
mod stake;
mod stake_v2;
mod treasury;
mod otc_treasury;
mod otc_user;

pub use automation::*;
pub use automation_v2::*;
pub use board::*;
pub use config::*;
pub use miner::*;
pub use pool_member::*;
pub use pool_round::*;
pub use referrer::*;
pub use round::*;
pub use sol_motherlode::*;
pub use stake::*;
pub use stake_v2::*;
pub use treasury::*;
pub use otc_treasury::*;
pub use otc_user::*;

use crate::consts::*;

use steel::*;

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoPrimitive, TryFromPrimitive)]
pub enum GodlAccount {
    Automation = 100,
    AutomationV2 = 111,
    Config = 101,
    Miner = 103,
    Treasury = 104,

    //
    Board = 105,
    Stake = 108,
    Round = 109,
    Referrer = 110,
    SolMotherlode = 112,
    PoolRound = 113,
    PoolMember = 114,
    StakeV2 = 115,
    OtcTreasury = 118,
    OtcUser = 119,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoPrimitive, TryFromPrimitive)]
pub enum GodlAccountOLD {
    ConfigOLD = 101,
}

pub fn automation_pda(authority: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[AUTOMATION, &authority.to_bytes()], &crate::ID)
}

pub fn automation_v2_pda(authority: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[AUTOMATION_V2, &authority.to_bytes()], &crate::ID)
}

pub fn board_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[BOARD], &crate::ID)
}

pub fn config_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[CONFIG], &crate::ID)
}

pub fn miner_pda(authority: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[MINER, &authority.to_bytes()], &crate::ID)
}

pub fn referrer_pda(authority: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[REFERRER, &authority.to_bytes()], &crate::ID)
}

pub fn round_pda(id: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[ROUND, &id.to_le_bytes()], &crate::ID)
}

pub fn pool_round_pda(id: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[POOL_ROUND, &id.to_le_bytes()], &crate::ID)
}

pub fn stake_pda(authority: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[STAKE, &authority.to_bytes()], &crate::ID)
}

pub fn pool_member_pda(authority: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[POOL_MEMBER, &authority.to_bytes()], &crate::ID)
}

pub fn treasury_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[TREASURY], &crate::ID)
}

pub fn sol_motherlode_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[SOL_MOTHERLODE], &crate::ID)
}

pub fn treasury_tokens_address(treasury_address: Pubkey) -> Pubkey {
    spl_associated_token_account::get_associated_token_address(&treasury_address, &MINT_ADDRESS)
}

pub fn stake_v2_pda(authority: Pubkey, id: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[STAKE_V2, &authority.to_bytes(), &id.to_le_bytes()], &crate::ID)
}

pub fn otc_treasury_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[OTC_TREASURY], &crate::ID)
}

pub fn otc_treasury_tokens_address() -> Pubkey {
    spl_associated_token_account::get_associated_token_address(&otc_treasury_pda().0, &MINT_ADDRESS)
}

pub fn otc_user_pda(authority: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[OTC_USER, &authority.to_bytes()], &crate::ID)
}
