use steel::*;

// ENUM CURSOR: 51
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, TryFromPrimitive)]
pub enum GodlInstruction {
    // Miner
    Initialize = 1,
    ClaimSOL = 3,
    ClaimGODL = 4,
    InjectUnrefinedRewards = 41,
    Log = 8,

    // Staker
    Deposit = 10,
    Withdraw = 11,
    ClaimYield = 12,

    // Admin
    Bury = 13,
    PreBury = 14,
    SetAdmin = 15,
    SetFeeCollector = 16,
    SetSwapProgram = 17,
    SetVarAddress = 18,
    NewVar = 19,
    SetMotherlodeDenominator = 20,
    SetGodlPerRound = 21,
    WithdrawVault = 25,

    // Referral
    InitializeReferrer = 22,
    SetReferrer = 23,
    ClaimReferral = 24,

    // New instructions
    ClaimSOLAndFundAutomation = 27,
    FundAutomation = 29,
    BuryTokens = 30,
    InitializeSolMotherlode = 32,
    Deploy = 33,
    Checkpoint = 34,
    Reset = 35,
    Close = 36,
    Automate = 37,

    // Stake V2
    DepositV2 = 38,
    WithdrawV2 = 39,
    ClaimYieldV2 = 40,
    CompoundYieldV2 = 42,
    SetStakeExecutorV2 = 43,
    
    InjectGodlMotherlode = 44,
    StakeNft = 45,
    UnstakeNft = 46,

    // OTC
    InitializeOtcTreasury = 47,
    FundGodlOtc = 48,
    ExecuteOtcTrade = 49,
    WithdrawSolOtc = 50,

    // Permissionless reset (liveness backstop for `Reset`).
    ResetPermissionless = 51,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Automate {
    pub amount: [u8; 8],
    pub deposit: [u8; 8],
    pub fee: [u8; 8],
    pub mask: [u8; 8],
    pub strategy: u8,
    pub claim_and_fund: u8,
    pub is_pooled: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Initialize {
    pub admin: [u8; 32],
    pub bury_authority: [u8; 32],
    pub fee_collector: [u8; 32],
    pub godl_per_round: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ClaimSOL {}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ClaimSOLAndFundAutomation {}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct FundAutomation {
    pub deposit: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ClaimGODL {}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct InjectUnrefinedRewards {
    pub amount: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct DeployV2 {
    pub amount: [u8; 8],
    pub squares: [u8; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Deploy {
    pub amount: [u8; 8],
    pub squares: [u8; 4],
    pub is_pooled: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Log {}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ResetV2 {}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Reset {}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ResetPermissionless {}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct InitializeReferrer {}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct SetReferrer {
    pub referrer: [u8; 32],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ClaimReferral {}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Mine {
    pub nonce: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Swap {
    pub amount: [u8; 8],
    pub direction: u8,
    pub precision: u8,
    pub seed: [u8; 32],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Uncommit {
    pub amount: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct SetAdmin {
    pub admin: [u8; 32],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct SetFeeCollector {
    pub fee_collector: [u8; 32],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct SetGodlPerRound {
    pub godl_per_round: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct PreBury {
    pub bury_amount: [u8; 8],
    pub chest_amount: [u8; 8],
    pub admin_amount: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Bury {}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct BuryTokens {
    pub amount: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Deposit {
    pub amount: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Withdraw {
    pub amount: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ClaimYield {
    pub amount: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct DepositV2 {
    pub id: [u8; 8],
    pub amount: [u8; 8],
    pub lock_duration: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct WithdrawV2 {
    pub id: [u8; 8],
    pub amount: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ClaimYieldV2 {
    pub id: [u8; 8],
    pub amount: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct CompoundYieldV2 {
    pub id: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct SetStakeExecutorV2 {
    pub id: [u8; 8],
    pub executor: [u8; 32],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Checkpoint {}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Close {}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct NewVar {
    pub id: [u8; 8],
    pub commit: [u8; 32],
    pub samples: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct SetMotherlodeDenominator {
    pub motherlode_denominator: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct SetSwapProgram {}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct SetVarAddress {}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct WithdrawVault {
    pub amount: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct InitializeSolMotherlode {}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct InjectGodlMotherlode {
    pub amount: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct StakeNft {
    pub id: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct UnstakeNft {
    pub id: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct InitializeOtcTreasury {}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct FundGodlOtc {
    pub amount: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ExecuteOtcTrade {
    pub stake_id: [u8; 8],
    pub sol_in: [u8; 8],
    pub godl_out: [u8; 8],
    pub godl_bonus: [u8; 8],
    pub expiry_slot: [u8; 8],
    pub lock_duration: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct WithdrawSolOtc {}

instruction!(GodlInstruction, Automate);
instruction!(GodlInstruction, Initialize);
instruction!(GodlInstruction, Close);
instruction!(GodlInstruction, Checkpoint);
instruction!(GodlInstruction, ClaimSOL);
instruction!(GodlInstruction, ClaimSOLAndFundAutomation);
instruction!(GodlInstruction, FundAutomation);
instruction!(GodlInstruction, ClaimGODL);
instruction!(GodlInstruction, InjectUnrefinedRewards);
instruction!(GodlInstruction, Deploy);
instruction!(GodlInstruction, Log);
instruction!(GodlInstruction, PreBury);
instruction!(GodlInstruction, Bury);
instruction!(GodlInstruction, BuryTokens);
instruction!(GodlInstruction, Reset);
instruction!(GodlInstruction, ResetPermissionless);
instruction!(GodlInstruction, SetAdmin);
instruction!(GodlInstruction, SetFeeCollector);
instruction!(GodlInstruction, SetGodlPerRound);
instruction!(GodlInstruction, Deposit);
instruction!(GodlInstruction, Withdraw);
instruction!(GodlInstruction, ClaimYield);
instruction!(GodlInstruction, DepositV2);
instruction!(GodlInstruction, WithdrawV2);
instruction!(GodlInstruction, ClaimYieldV2);
instruction!(GodlInstruction, CompoundYieldV2);
instruction!(GodlInstruction, SetStakeExecutorV2);
instruction!(GodlInstruction, NewVar);
instruction!(GodlInstruction, SetMotherlodeDenominator);
instruction!(GodlInstruction, SetSwapProgram);
instruction!(GodlInstruction, SetVarAddress);
instruction!(GodlInstruction, InitializeReferrer);
instruction!(GodlInstruction, SetReferrer);
instruction!(GodlInstruction, ClaimReferral);
instruction!(GodlInstruction, WithdrawVault);
instruction!(GodlInstruction, InitializeSolMotherlode);
instruction!(GodlInstruction, InjectGodlMotherlode);
instruction!(GodlInstruction, StakeNft);
instruction!(GodlInstruction, UnstakeNft);
instruction!(GodlInstruction, InitializeOtcTreasury);
instruction!(GodlInstruction, FundGodlOtc);
instruction!(GodlInstruction, ExecuteOtcTrade);
instruction!(GodlInstruction, WithdrawSolOtc);
