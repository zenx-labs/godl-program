mod admin;
mod automation;
mod initialize;
mod miner;
mod otc;
mod referral;
mod stake;
mod stake_v2;

use admin::*;
use automation::*;
use initialize::*;
use miner::*;
use otc::*;
use referral::*;
use stake::*;
use stake_v2::*;

use godl_api::instruction::*;
use solana_security_txt::security_txt;
use steel::*;

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    let (ix, data) = parse_instruction(&godl_api::ID, program_id, data)?;

    match ix {
        GodlInstruction::Initialize => process_initialize(accounts, data)?,

        // Miner
        GodlInstruction::Automate => process_automate(accounts, data)?,
        GodlInstruction::Checkpoint => process_checkpoint(accounts, data)?,
        GodlInstruction::ClaimSOL => process_claim_sol(accounts, data)?,
        GodlInstruction::ClaimSOLAndFundAutomation => {
            process_claim_sol_and_fund_automation(accounts, data)?
        }
        GodlInstruction::FundAutomation => process_fund_automation(accounts, data)?,
        GodlInstruction::ClaimGODL => process_claim_godl(accounts, data)?,
        GodlInstruction::InjectUnrefinedRewards => {
            process_inject_unrefined_rewards(accounts, data)?
        }
        GodlInstruction::InitializeReferrer => process_initialize_referrer(accounts, data)?,
        GodlInstruction::SetReferrer => process_set_referrer(accounts, data)?,
        GodlInstruction::ClaimReferral => process_claim_referral(accounts, data)?,
        GodlInstruction::Deploy => process_deploy(accounts, data)?,
        GodlInstruction::Log => process_log(accounts, data)?,
        GodlInstruction::Close => process_close(accounts, data)?,
        GodlInstruction::Reset => process_reset(accounts, data)?,
        GodlInstruction::ResetPermissionless => process_reset_permissionless(accounts, data)?,

        // Staker
        GodlInstruction::Deposit => process_deposit(accounts, data)?,
        GodlInstruction::Withdraw => process_withdraw(accounts, data)?,
        GodlInstruction::ClaimYield => process_claim_yield(accounts, data)?,

        // Stake V2
        GodlInstruction::DepositV2 => process_deposit_v2(accounts, data)?,
        GodlInstruction::WithdrawV2 => process_withdraw_v2(accounts, data)?,
        GodlInstruction::ClaimYieldV2 => process_claim_yield_v2(accounts, data)?,
        GodlInstruction::CompoundYieldV2 => process_compound_yield_v2(accounts, data)?,
        GodlInstruction::SetStakeExecutorV2 => process_set_stake_executor_v2(accounts, data)?,
        GodlInstruction::ReconcileStakeV2 => process_reconcile_stake_v2(accounts, data)?,
        GodlInstruction::MigrateStakeWeight => process_migrate_stake_weight(accounts, data)?,
        GodlInstruction::StakeNft => process_stake_nft(accounts, data)?,
        GodlInstruction::UnstakeNft => process_unstake_nft(accounts, data)?,

        // Admin
        GodlInstruction::Bury => process_bury(accounts, data)?,
        GodlInstruction::BuryTokens => process_bury_tokens(accounts, data)?,
        GodlInstruction::PreBury => process_pre_bury(accounts, data)?,
        GodlInstruction::SetAdmin => process_set_admin(accounts, data)?,
        GodlInstruction::SetFeeCollector => process_set_fee_collector(accounts, data)?,
        GodlInstruction::SetSwapProgram => process_set_swap_program(accounts, data)?,
        GodlInstruction::SetVarAddress => process_set_var_address(accounts, data)?,
        GodlInstruction::NewVar => process_new_var(accounts, data)?,
        GodlInstruction::SetMotherlodeDenominator => {
            process_set_motherlode_denominator(accounts, data)?
        }
        GodlInstruction::SetGodlPerRound => process_set_godl_per_round(accounts, data)?,
        GodlInstruction::WithdrawVault => process_withdraw_vault(accounts, data)?,
        GodlInstruction::InitializeSolMotherlode => {
            process_initialize_sol_motherlode(accounts, data)?
        }
        GodlInstruction::InjectGodlMotherlode => process_inject_godl_motherlode(accounts, data)?,
        GodlInstruction::RebaseTotalStaked => process_rebase_total_staked(accounts, data)?,

        // OTC
        GodlInstruction::InitializeOtcTreasury => process_initialize_otc_treasury(accounts, data)?,
        GodlInstruction::FundGodlOtc => process_fund_godl_otc(accounts, data)?,
        GodlInstruction::ExecuteOtcTrade => process_execute_otc_trade(accounts, data)?,
        GodlInstruction::WithdrawSolOtc => process_withdraw_sol_otc(accounts, data)?,

        // Migration
        GodlInstruction::TransferMintAuthority => process_transfer_mint_authority(accounts, data)?,
    }

    Ok(())
}

entrypoint!(process_instruction);

security_txt! { 
    name: "Godl",
    project_url: "https://godl.supply",
    contacts: "email:bootapollo@pm.me,telegram:bootapollo",
    policy: "https://github.com/zenx-labs/godl-program/blob/main/SECURITY.md",
    preferred_languages: "en",
    source_code: "https://github.com/zenx-labs/godl-program"
}
