use steel::*;

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq, IntoPrimitive)]
#[repr(u32)]
pub enum GodlError {
    #[error("Amount too small")]
    AmountTooSmall = 0,

    #[error("Not authorized")]
    NotAuthorized = 1,

    #[error("Invalid referrer account")]
    InvalidReferrerAccount = 2,

    #[error("Invalid lock duration")]
    InvalidLockDuration = 3,

    #[error("Stake is locked")]
    StakeLocked = 4,

    #[error("Stake is locked")]
    StakeUnlocked = 5,

    #[error("Stake V2 is not released yet")]
    StakeV2NotReleased = 6,

    #[error("Stake amount too large")]
    StakeOverflow = 7,

    #[error("Miner has already deployed in this round")]
    AlreadyDeployedThisRound = 8,

    #[error("Stake account already exists")]
    StakeAlreadyExists = 9,

    #[error("NFT already staked to this account")]
    NftAlreadyStaked = 10,

    #[error("No NFT staked to this account")]
    NftNotStaked = 11,

    #[error("NFT does not belong to the required collection")]
    InvalidCollection = 12,

    #[error("OTC treasury has insufficient GODL balance")]
    InsufficientOtcGodlBalance = 13,

    #[error("OTC quote has expired")]
    OtcQuoteExpired = 14,

    #[error("Total staked does not match the expected value")]
    RebaseMismatch = 15,
}

error!(GodlError);
