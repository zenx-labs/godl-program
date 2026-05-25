use solana_program::pubkey::Pubkey;
use spl_associated_token_account::get_associated_token_address;
use steel::*;

use crate::{
    consts::{ADMIN_GODL_FEE, BOARD, CHEST_ADDRESS, MINT_ADDRESS, MPL_CORE_PROGRAM, NFT_BOOST_COLLECTION, OTC_ORACLE_SIGNER, SOL_MINT},
    instruction::*,
    state::*,
};

pub fn log(signer: Pubkey, msg: &[u8]) -> Instruction {
    let mut data = Log {}.to_bytes();
    data.extend_from_slice(msg);
    Instruction {
        program_id: crate::ID,
        accounts: vec![AccountMeta::new(signer, true)],
        data: data,
    }
}

pub fn initialize(
    signer: Pubkey,
    admin: Pubkey,
    bury_authority: Pubkey,
    fee_collector: Pubkey,
    godl_per_round: u64,
) -> Instruction {
    let config_address = config_pda().0;
    let treasury_address = treasury_pda().0;
    let board_address = board_pda().0;
    let round_address = round_pda(0).0;
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(config_address, false),
            AccountMeta::new(treasury_address, false),
            AccountMeta::new(board_address, false),
            AccountMeta::new(round_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: Initialize {
            admin: admin.to_bytes(),
            bury_authority: bury_authority.to_bytes(),
            fee_collector: fee_collector.to_bytes(),
            godl_per_round: godl_per_round.to_le_bytes(),
        }
        .to_bytes(),
    }
}

pub fn program_log(accounts: &[AccountInfo], msg: &[u8]) -> Result<(), ProgramError> {
    invoke_signed(&log(*accounts[0].key, msg), accounts, &crate::ID, &[BOARD])
}
// let [signer_info, automation_info, executor_info, miner_info, system_program] = accounts else {


pub fn automate(
    signer: Pubkey,
    amount: u64,
    deposit: u64,
    executor: Pubkey,
    fee: u64,
    mask: u64,
    strategy: u8,
    claim_and_fund: bool,
    is_pooled: bool,
) -> Instruction {
    let automation_v2_address = automation_v2_pda(signer).0;
    let miner_address = miner_pda(signer).0;
    let pool_member_address = pool_member_pda(signer).0;
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(automation_v2_address, false),
            AccountMeta::new(executor, false),
            AccountMeta::new(miner_address, false),
            AccountMeta::new(pool_member_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: Automate {
            amount: amount.to_le_bytes(),
            deposit: deposit.to_le_bytes(),
            fee: fee.to_le_bytes(),
            mask: mask.to_le_bytes(),
            strategy: strategy as u8,
            claim_and_fund: claim_and_fund as u8,
            is_pooled: is_pooled as u8,
        }
        .to_bytes(),
    }
}

// let [signer_info, automation_v2_info, system_program] = accounts else {

pub fn fund_automation(signer: Pubkey, amount: u64) -> Instruction {
    let automation_v2_address = automation_v2_pda(signer).0;
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(automation_v2_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: FundAutomation {
            deposit: amount.to_le_bytes(),
        }
        .to_bytes(),
    }
}

pub fn initialize_referrer(signer: Pubkey) -> Instruction {
    let referrer_address = referrer_pda(signer).0;
    let referrer_tokens_address = get_associated_token_address(&referrer_address, &MINT_ADDRESS);
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(referrer_address, false),
            AccountMeta::new(referrer_tokens_address, false),
            AccountMeta::new(MINT_ADDRESS, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(spl_token::ID, false),
            AccountMeta::new_readonly(spl_associated_token_account::ID, false),
        ],
        data: InitializeReferrer {}.to_bytes(),
    }
}

pub fn set_referrer(signer: Pubkey, referrer: Option<Pubkey>) -> Instruction {
    let miner_address = miner_pda(signer).0;
    let mut accounts = vec![
        AccountMeta::new(signer, true),
        AccountMeta::new(miner_address, false),
    ];
    let referrer_key = referrer.unwrap_or_default();
    if let Some(address) = referrer {
        accounts.push(AccountMeta::new(address, false));
    }
    Instruction {
        program_id: crate::ID,
        accounts,
        data: SetReferrer {
            referrer: referrer_key.to_bytes(),
        }
        .to_bytes(),
    }
}

// let [signer_info, miner_info,  system_program, rest @ ..] =

pub fn claim_sol(signer: Pubkey) -> Instruction {
    claim_sol_with_referrer(signer, None)
}

pub fn claim_sol_with_referrer(signer: Pubkey, referrer: Option<Pubkey>) -> Instruction {
    let miner_address = miner_pda(signer).0;
    let mut accounts = vec![
        AccountMeta::new(signer, true),
        AccountMeta::new(miner_address, false),
        AccountMeta::new_readonly(system_program::ID, false),
    ];
    if let Some(referrer_address) = referrer {
        accounts.push(AccountMeta::new(referrer_address, false));
    }
    Instruction {
        program_id: crate::ID,
        accounts,
        data: ClaimSOL {}.to_bytes(),
    }
}

// let [signer_info, miner_info, automation_v2_info, system_program, rest @ ..] =

pub fn claim_sol_and_fund_automation(
    signer: Pubkey,
    miner: Pubkey,
    referrer: Option<Pubkey>,
) -> Instruction {
    claim_sol_and_fund_automation_with_referrer(signer, miner, referrer)
}

pub fn claim_sol_and_fund_automation_with_referrer(
    signer: Pubkey,
    miner: Pubkey,
    referrer: Option<Pubkey>,
) -> Instruction {
    let miner_address = miner_pda(miner).0;
    let automation_v2_address = automation_v2_pda(miner).0;
    let mut accounts = vec![
        AccountMeta::new(signer, true),
        AccountMeta::new(miner_address, false),
        AccountMeta::new(automation_v2_address, false),
        AccountMeta::new_readonly(system_program::ID, false),
    ];
    if let Some(referrer_address) = referrer {
        accounts.push(AccountMeta::new(referrer_address, false));
    }
    Instruction {
        program_id: crate::ID,
        accounts,
        data: ClaimSOLAndFundAutomation {}.to_bytes(),
    }
}

// let [signer_info, miner_info, mint_info, recipient_info, treasury_info, treasury_tokens_info, system_program, token_program, associated_token_program] =

pub fn claim_godl(signer: Pubkey) -> Instruction {
    claim_godl_with_referrer(signer, None)
}

pub fn claim_godl_with_referrer(signer: Pubkey, referrer: Option<Pubkey>) -> Instruction {
    let miner_address = miner_pda(signer).0;
    let treasury_address = treasury_pda().0;
    let treasury_tokens_address = get_associated_token_address(&treasury_address, &MINT_ADDRESS);
    let recipient_address = get_associated_token_address(&signer, &MINT_ADDRESS);
    let mut accounts = vec![
        AccountMeta::new(signer, true),
        AccountMeta::new(miner_address, false),
        AccountMeta::new(MINT_ADDRESS, false),
        AccountMeta::new(recipient_address, false),
        AccountMeta::new(treasury_address, false),
        AccountMeta::new(treasury_tokens_address, false),
        AccountMeta::new_readonly(system_program::ID, false),
        AccountMeta::new_readonly(spl_token::ID, false),
        AccountMeta::new_readonly(spl_associated_token_account::ID, false),
    ];
    if let Some(referrer_address) = referrer {
        let referrer_tokens_address =
            get_associated_token_address(&referrer_address, &MINT_ADDRESS);
        accounts.push(AccountMeta::new(referrer_address, false));
        accounts.push(AccountMeta::new(referrer_tokens_address, false));
    }
    Instruction {
        program_id: crate::ID,
        accounts,
        data: ClaimGODL {}.to_bytes(),
    }
}

pub fn inject_unrefined_rewards(signer: Pubkey, miner: Pubkey, amount: u64) -> Instruction {
    let miner_address = miner_pda(miner).0;
    let config_address = config_pda().0;
    let treasury_address = treasury_pda().0;
    let signer_tokens_address = get_associated_token_address(&signer, &MINT_ADDRESS);
    let treasury_tokens_address = get_associated_token_address(&treasury_address, &MINT_ADDRESS);
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new_readonly(config_address, false),
            AccountMeta::new(miner_address, false),
            AccountMeta::new(treasury_address, false),
            AccountMeta::new(signer_tokens_address, false),
            AccountMeta::new(treasury_tokens_address, false),
            AccountMeta::new_readonly(spl_token::ID, false),
        ],
        data: InjectUnrefinedRewards {
            amount: amount.to_le_bytes(),
        }
        .to_bytes(),
    }
}

pub fn claim_referral(signer: Pubkey) -> Instruction {
    let referrer_address = referrer_pda(signer).0;
    let referrer_tokens_address = get_associated_token_address(&referrer_address, &MINT_ADDRESS);
    let authority_tokens_address = get_associated_token_address(&signer, &MINT_ADDRESS);
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(referrer_address, false),
            AccountMeta::new(referrer_tokens_address, false),
            AccountMeta::new(authority_tokens_address, false),
            AccountMeta::new(MINT_ADDRESS, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(spl_token::ID, false),
            AccountMeta::new_readonly(spl_associated_token_account::ID, false),
        ],
        data: ClaimReferral {}.to_bytes(),
    }
}

// let [signer_info, authority_info, automation_v2_info, board_info, miner_info, round_info, config_info, system_program] =

pub fn deploy(
    signer: Pubkey,
    authority: Pubkey,
    var_address: Pubkey,
    amount: u64,
    round_id: u64,
    squares: [bool; 25],
    is_pooled: bool,
) -> Instruction {
    deploy_with_pool(
        signer,
        authority,
        var_address,
        amount,
        round_id,
        squares,
        is_pooled,
    )
}

pub fn deploy_with_pool(
    signer: Pubkey,
    authority: Pubkey,
    var_address: Pubkey,
    amount: u64,
    round_id: u64,
    squares: [bool; 25],
    is_pooled: bool,
) -> Instruction {
    build_deploy(
        signer,
        authority,
        var_address,
        amount,
        round_id,
        squares,
        is_pooled,
    )
}


fn build_deploy(
    signer: Pubkey,
    authority: Pubkey,
    var_address: Pubkey,
    amount: u64,
    round_id: u64,
    squares: [bool; 25],
    is_pooled: bool,
) -> Instruction {
    let automation_v2_address = automation_v2_pda(authority).0;
    let config_address = config_pda().0;
    let board_address = board_pda().0;
    let miner_address = miner_pda(authority).0;
    let round_address = round_pda(round_id).0;
    let pool_round_address = pool_round_pda(round_id).0;
    let pool_member_address = pool_member_pda(authority).0;

    // Convert array of 25 booleans into a 32-bit mask where each bit represents whether
    // that square index is selected (1) or not (0)
    let mut mask: u32 = 0;
    for (i, &square) in squares.iter().enumerate() {
        if square {
            mask |= 1 << i;
        }
    }

    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(authority, false),
            AccountMeta::new(automation_v2_address, false),
            AccountMeta::new(board_address, false),
            AccountMeta::new(miner_address, false),
            AccountMeta::new(round_address, false),
            AccountMeta::new(pool_round_address, false),
            AccountMeta::new(pool_member_address, false),
            AccountMeta::new(config_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
            // Entropy accounts.
            AccountMeta::new(var_address, false),
            AccountMeta::new_readonly(entropy_api::ID, false),
        ],
        data: Deploy {
            amount: amount.to_le_bytes(),
            squares: mask.to_le_bytes(),
            is_pooled: is_pooled as u8,
        }
        .to_bytes(),
    }
}

// let [pool, user_source_token, user_destination_token, a_vault, b_vault, a_token_vault, b_token_vault, a_vault_lp_mint, b_vault_lp_mint, a_vault_lp, b_vault_lp, protocol_token_fee, user_key, vault_program, token_program] =

pub fn bury_tokens(signer: Pubkey, amount: u64, no_burn: bool) -> Instruction {
    let board_address = board_pda().0;
    let config_address = config_pda().0;
    let mint_address = MINT_ADDRESS;
    let treasury_address = treasury_pda().0;
    let treasury_godl_address = get_associated_token_address(&treasury_address, &MINT_ADDRESS);
    let admin_godl_address = get_associated_token_address(&ADMIN_GODL_FEE, &MINT_ADDRESS);
    let (warchest_address, warchest_godl_address) = if no_burn {
        (
            CHEST_ADDRESS,
            get_associated_token_address(&CHEST_ADDRESS, &MINT_ADDRESS),
        )
    } else {
        (Pubkey::default(), Pubkey::default())
    };
    let warchest_account = AccountMeta::new_readonly(warchest_address, false);
    let warchest_godl_account = if no_burn {
        AccountMeta::new(warchest_godl_address, false)
    } else {
        AccountMeta::new_readonly(warchest_godl_address, false)
    };
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(board_address, false),
            AccountMeta::new_readonly(config_address, false),
            AccountMeta::new(mint_address, false),
            AccountMeta::new(treasury_address, false),
            AccountMeta::new(treasury_godl_address, false),
            AccountMeta::new(ADMIN_GODL_FEE, false),
            AccountMeta::new(admin_godl_address, false),
            warchest_account,
            warchest_godl_account,
            AccountMeta::new_readonly(spl_token::ID, false),
            AccountMeta::new_readonly(crate::ID, false),
        ],
        data: BuryTokens {
            amount: amount.to_le_bytes(),
        }
        .to_bytes(),
    }
}

pub fn bury(
    signer: Pubkey,
    swap_accounts: &[AccountMeta],
    swap_data: &[u8],
    no_burn: bool,
) -> Instruction {
    let board_address = board_pda().0;
    let config_address = config_pda().0;
    let mint_address = MINT_ADDRESS;
    let treasury_address = treasury_pda().0;
    let treasury_godl_address = get_associated_token_address(&treasury_address, &MINT_ADDRESS);
    let treasury_sol_address = get_associated_token_address(&treasury_address, &SOL_MINT);
    let admin_godl_address = get_associated_token_address(&ADMIN_GODL_FEE, &MINT_ADDRESS);
    let (warchest_address, warchest_godl_address) = if no_burn {
        (
            CHEST_ADDRESS,
            get_associated_token_address(&CHEST_ADDRESS, &MINT_ADDRESS),
        )
    } else {
        (Pubkey::default(), Pubkey::default())
    };
    let warchest_account = AccountMeta::new_readonly(warchest_address, false);
    let warchest_godl_account = if no_burn {
        AccountMeta::new(warchest_godl_address, false)
    } else {
        AccountMeta::new_readonly(warchest_godl_address, false)
    };
    let mut accounts = vec![
        AccountMeta::new(signer, true),
        AccountMeta::new(board_address, false),
        AccountMeta::new_readonly(config_address, false),
        AccountMeta::new(mint_address, false),
        AccountMeta::new(treasury_address, false),
        AccountMeta::new(treasury_godl_address, false),
        AccountMeta::new(treasury_sol_address, false),
        AccountMeta::new(ADMIN_GODL_FEE, false),
        AccountMeta::new(admin_godl_address, false),
        warchest_account,
        warchest_godl_account,
        AccountMeta::new_readonly(spl_token::ID, false),
        AccountMeta::new_readonly(crate::ID, false),
    ];
    for account in swap_accounts.iter() {
        let mut acc_clone = account.clone();
        acc_clone.is_signer = false;
        accounts.push(acc_clone);
    }
    let mut data = Bury {}.to_bytes();
    data.extend_from_slice(swap_data);
    Instruction {
        program_id: crate::ID,
        accounts,
        data,
    }
}

pub fn pre_bury(signer: Pubkey, bury_amount: u64, chest_amount: u64, admin_amount: u64) -> Instruction {
    let config_address = config_pda().0;
    let treasury_address = treasury_pda().0;
    let treasury_sol_address = get_associated_token_address(&treasury_address, &SOL_MINT);
    Instruction {
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new_readonly(config_address, false),
            AccountMeta::new(treasury_address, false),
            AccountMeta::new(treasury_sol_address, false),
            AccountMeta::new(CHEST_ADDRESS, false),
            AccountMeta::new(ADMIN_GODL_FEE, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        program_id: crate::ID,
        data: PreBury {
            bury_amount: bury_amount.to_le_bytes(),
            chest_amount: chest_amount.to_le_bytes(),
            admin_amount: admin_amount.to_le_bytes(),
        }
        .to_bytes(),
    }
}

pub fn withdraw_vault(signer: Pubkey, amount: u64) -> Instruction {
    let config_address = config_pda().0;
    let treasury_address = treasury_pda().0;
    Instruction {
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new_readonly(config_address, false),
            AccountMeta::new(treasury_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        program_id: crate::ID,
        data: WithdrawVault {
            amount: amount.to_le_bytes(),
        }
        .to_bytes(),
    }
}


pub fn reset(
    signer: Pubkey,
    fee_collector: Pubkey,
    round_id: u64,
    top_miner: Pubkey,
    var_address: Pubkey,
) -> Instruction {
    let board_address = board_pda().0;
    let config_address = config_pda().0;
    let mint_address = MINT_ADDRESS;
    let round_address = round_pda(round_id).0;
    let round_next_address = round_pda(round_id + 1).0;
    let pool_round_next_address = pool_round_pda(round_id + 1).0;
    let top_miner_address = miner_pda(top_miner).0;
    let top_miner_pool_member_address = pool_member_pda(top_miner).0;
    let sol_motherlode_address = sol_motherlode_pda().0;
    let treasury_address = treasury_pda().0;
    let treasury_tokens_address = treasury_tokens_address(treasury_address);
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(board_address, false),
            AccountMeta::new(config_address, false),
            AccountMeta::new(fee_collector, false),
            AccountMeta::new(mint_address, false),
            AccountMeta::new(round_address, false),
            AccountMeta::new(round_next_address, false),
            AccountMeta::new(pool_round_next_address, false),
            AccountMeta::new(top_miner_address, false),
            AccountMeta::new(top_miner_pool_member_address, false),
            AccountMeta::new(sol_motherlode_address, false),
            AccountMeta::new(treasury_address, false),
            AccountMeta::new(treasury_tokens_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(spl_token::ID, false),
            AccountMeta::new_readonly(crate::ID, false),
            AccountMeta::new_readonly(sysvar::slot_hashes::ID, false),
            // Entropy accounts.
            AccountMeta::new(var_address, false),
            AccountMeta::new_readonly(entropy_api::ID, false),
        ],
        data: Reset {}.to_bytes(),
    }
}

/// Liveness backstop for `reset`. Anyone may sign.
///
/// `top_miner` must be the actual winning miner on the winning square — the
/// program rejects garbage candidates. For the empty-winning-square and
/// split-reward cases (where the program never inspects `top_miner_info`),
/// `Pubkey::default()` is accepted.
pub fn reset_permissionless(
    signer: Pubkey,
    fee_collector: Pubkey,
    round_id: u64,
    top_miner: Pubkey,
    var_address: Pubkey,
) -> Instruction {
    let board_address = board_pda().0;
    let config_address = config_pda().0;
    let mint_address = MINT_ADDRESS;
    let round_address = round_pda(round_id).0;
    let round_next_address = round_pda(round_id + 1).0;
    let pool_round_next_address = pool_round_pda(round_id + 1).0;
    let top_miner_address = miner_pda(top_miner).0;
    let top_miner_pool_member_address = pool_member_pda(top_miner).0;
    let sol_motherlode_address = sol_motherlode_pda().0;
    let treasury_address = treasury_pda().0;
    let treasury_tokens_address = treasury_tokens_address(treasury_address);
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(board_address, false),
            AccountMeta::new(config_address, false),
            AccountMeta::new(fee_collector, false),
            AccountMeta::new(mint_address, false),
            AccountMeta::new(round_address, false),
            AccountMeta::new(round_next_address, false),
            AccountMeta::new(pool_round_next_address, false),
            AccountMeta::new(top_miner_address, false),
            AccountMeta::new(top_miner_pool_member_address, false),
            AccountMeta::new(sol_motherlode_address, false),
            AccountMeta::new(treasury_address, false),
            AccountMeta::new(treasury_tokens_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(spl_token::ID, false),
            AccountMeta::new_readonly(crate::ID, false),
            AccountMeta::new_readonly(sysvar::slot_hashes::ID, false),
            // Entropy accounts.
            AccountMeta::new(var_address, false),
            AccountMeta::new_readonly(entropy_api::ID, false),
        ],
        data: ResetPermissionless {}.to_bytes(),
    }
}


pub fn close(signer: Pubkey, round_id: u64, rent_payer: Pubkey) -> Instruction {
    let board_address = board_pda().0;
    let treasury_address = treasury_pda().0;
    let round_address = round_pda(round_id).0;
    let pool_round_address = pool_round_pda(round_id).0;
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(board_address, false),
            AccountMeta::new(rent_payer, false),
            AccountMeta::new(round_address, false),
            AccountMeta::new(pool_round_address, false),
            AccountMeta::new(treasury_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: Close {}.to_bytes(),
    }
}

pub fn checkpoint(signer: Pubkey, authority: Pubkey, round_id: u64) -> Instruction {
    let miner_address = miner_pda(authority).0;
    let board_address = board_pda().0;
    let round_address = round_pda(round_id).0;
    let treasury_address = treasury_pda().0;
    let pool_round_address = pool_round_pda(round_id).0;
    let pool_member_address = pool_member_pda(authority).0;
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(board_address, false),
            AccountMeta::new(miner_address, false),
            AccountMeta::new(round_address, false),
            AccountMeta::new(treasury_address, false),
            AccountMeta::new(pool_round_address, false),
            AccountMeta::new(pool_member_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: Checkpoint {}.to_bytes(),
    }
}

pub fn set_admin(signer: Pubkey, admin: Pubkey) -> Instruction {
    let config_address = config_pda().0;
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(config_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: SetAdmin {
            admin: admin.to_bytes(),
        }
        .to_bytes(),
    }
}

pub fn set_motherlode_denominator(signer: Pubkey, motherlode_denominator: u64) -> Instruction {
    let config_address = config_pda().0;
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(config_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: SetMotherlodeDenominator {
            motherlode_denominator: motherlode_denominator.to_le_bytes(),
        }
        .to_bytes(),
    }
}

pub fn set_godl_per_round(signer: Pubkey, godl_per_round: u64) -> Instruction {
    let config_address = config_pda().0;
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(config_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: SetGodlPerRound {
            godl_per_round: godl_per_round.to_le_bytes(),
        }
        .to_bytes(),
    }
}

pub fn set_fee_collector(signer: Pubkey, fee_collector: Pubkey) -> Instruction {
    let config_address = config_pda().0;
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(config_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: SetFeeCollector {
            fee_collector: fee_collector.to_bytes(),
        }
        .to_bytes(),
    }
}

// let [signer_info, mint_info, sender_info, stake_info, stake_tokens_info, treasury_info, system_program, token_program, associated_token_program] =

pub fn deposit(signer: Pubkey, amount: u64) -> Instruction {
    let mint_address = MINT_ADDRESS;
    let stake_address = stake_pda(signer).0;
    let stake_tokens_address = get_associated_token_address(&stake_address, &MINT_ADDRESS);
    let sender_address = get_associated_token_address(&signer, &MINT_ADDRESS);
    let treasury_address = treasury_pda().0;
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(mint_address, false),
            AccountMeta::new(sender_address, false),
            AccountMeta::new(stake_address, false),
            AccountMeta::new(stake_tokens_address, false),
            AccountMeta::new(treasury_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(spl_token::ID, false),
            AccountMeta::new_readonly(spl_associated_token_account::ID, false),
        ],
        data: Deposit {
            amount: amount.to_le_bytes(),
        }
        .to_bytes(),
    }
}

// let [signer_info, mint_info, recipient_info, stake_info, stake_tokens_info, treasury_info, system_program, token_program, associated_token_program] =

pub fn withdraw(signer: Pubkey, amount: u64) -> Instruction {
    let stake_address = stake_pda(signer).0;
    let stake_tokens_address = get_associated_token_address(&stake_address, &MINT_ADDRESS);
    let mint_address = MINT_ADDRESS;
    let recipient_address = get_associated_token_address(&signer, &MINT_ADDRESS);
    let treasury_address = treasury_pda().0;
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(mint_address, false),
            AccountMeta::new(recipient_address, false),
            AccountMeta::new(stake_address, false),
            AccountMeta::new(stake_tokens_address, false),
            AccountMeta::new(treasury_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(spl_token::ID, false),
            AccountMeta::new_readonly(spl_associated_token_account::ID, false),
        ],
        data: Withdraw {
            amount: amount.to_le_bytes(),
        }
        .to_bytes(),
    }
}

// let [signer_info, mint_info, recipient_info, stake_info, treasury_info, treasury_tokens_info, system_program, token_program, associated_token_program] =

pub fn claim_yield(signer: Pubkey, amount: u64) -> Instruction {
    let stake_address = stake_pda(signer).0;
    let mint_address = MINT_ADDRESS;
    let recipient_address = get_associated_token_address(&signer, &MINT_ADDRESS);
    let treasury_address = treasury_pda().0;
    let treasury_tokens_address = treasury_tokens_address(treasury_address);
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(mint_address, false),
            AccountMeta::new(recipient_address, false),
            AccountMeta::new(stake_address, false),
            AccountMeta::new(treasury_address, false),
            AccountMeta::new(treasury_tokens_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(spl_token::ID, false),
            AccountMeta::new_readonly(spl_associated_token_account::ID, false),
        ],
        data: ClaimYield {
            amount: amount.to_le_bytes(),
        }
        .to_bytes(),
    }
}

pub fn set_stake_executor_v2(signer: Pubkey, id: u64, executor: Pubkey) -> Instruction {
    let stake_address = stake_v2_pda(signer, id).0;

    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(stake_address, false),
        ],
        data: SetStakeExecutorV2 {
            id: id.to_le_bytes(),
            executor: executor.to_bytes(),
        }
        .to_bytes(),
    }
}

pub fn compound_yield_v2(signer: Pubkey, authority: Pubkey, id: u64) -> Instruction {
    let stake_address = stake_v2_pda(authority, id).0;
    let stake_tokens_address = get_associated_token_address(&stake_address, &MINT_ADDRESS);
    let mint_address = MINT_ADDRESS;
    let treasury_address = treasury_pda().0;
    let treasury_tokens_address = treasury_tokens_address(treasury_address);

    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(mint_address, false),
            AccountMeta::new(stake_address, false),
            AccountMeta::new(stake_tokens_address, false),
            AccountMeta::new(treasury_address, false),
            AccountMeta::new(treasury_tokens_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(spl_token::ID, false),
            AccountMeta::new_readonly(spl_associated_token_account::ID, false),
        ],
        data: CompoundYieldV2 {
            id: id.to_le_bytes(),
        }
        .to_bytes(),
    }
}

pub fn new_var(
    signer: Pubkey,
    provider: Pubkey,
    id: u64,
    commit: [u8; 32],
    samples: u64,
) -> Instruction {
    let board_address = board_pda().0;
    let config_address = config_pda().0;
    let var_address = entropy_api::state::var_pda(board_address, id).0;
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(board_address, false),
            AccountMeta::new(config_address, false),
            AccountMeta::new(provider, false),
            AccountMeta::new(var_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(entropy_api::ID, false),
        ],
        data: NewVar {
            id: id.to_le_bytes(),
            commit: commit,
            samples: samples.to_le_bytes(),
        }
        .to_bytes(),
    }
}

pub fn set_swap_program(signer: Pubkey, new_program: Pubkey) -> Instruction {
    let config_address = config_pda().0;
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(config_address, false),
            AccountMeta::new_readonly(new_program, false),
        ],
        data: SetSwapProgram {}.to_bytes(),
    }
}

pub fn set_var_address(signer: Pubkey, new_var_address: Pubkey) -> Instruction {
    let board_address = board_pda().0;
    let config_address = config_pda().0;
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(board_address, false),
            AccountMeta::new(config_address, false),
            AccountMeta::new(new_var_address, false),
        ],
        data: SetVarAddress {}.to_bytes(),
    }
}

pub fn inject_godl_motherlode(signer: Pubkey, amount: u64) -> Instruction {
    let treasury_address = treasury_pda().0;
    let signer_tokens_address = get_associated_token_address(&signer, &MINT_ADDRESS);
    let treasury_tokens_address = get_associated_token_address(&treasury_address, &MINT_ADDRESS);
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(treasury_address, false),
            AccountMeta::new(signer_tokens_address, false),
            AccountMeta::new(treasury_tokens_address, false),
            AccountMeta::new_readonly(spl_token::ID, false),
        ],
        data: InjectGodlMotherlode {
            amount: amount.to_le_bytes(),
        }
        .to_bytes(),
    }
}

pub fn stake_nft(signer: Pubkey, id: u64, asset: Pubkey) -> Instruction {
    let stake_address = stake_v2_pda(signer, id).0;
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(asset, false),
            AccountMeta::new_readonly(NFT_BOOST_COLLECTION, false),
            AccountMeta::new(stake_address, false),
            AccountMeta::new(treasury_pda().0, false),
            AccountMeta::new_readonly(MPL_CORE_PROGRAM, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: StakeNft {
            id: id.to_le_bytes(),
        }
        .to_bytes(),
    }
}

pub fn unstake_nft(signer: Pubkey, id: u64, asset: Pubkey) -> Instruction {
    let stake_address = stake_v2_pda(signer, id).0;
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(asset, false),
            AccountMeta::new_readonly(NFT_BOOST_COLLECTION, false),
            AccountMeta::new(stake_address, false),
            AccountMeta::new(treasury_pda().0, false),
            AccountMeta::new_readonly(MPL_CORE_PROGRAM, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: UnstakeNft {
            id: id.to_le_bytes(),
        }
        .to_bytes(),
    }
}

pub fn initialize_sol_motherlode(signer: Pubkey) -> Instruction {
    let sol_motherlode_address = sol_motherlode_pda().0;
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(sol_motherlode_address, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: InitializeSolMotherlode {}.to_bytes(),
    }
}

pub fn initialize_otc_treasury(signer: Pubkey) -> Instruction {
    let otc_treasury_address = otc_treasury_pda().0;
    let otc_treasury_tokens = otc_treasury_tokens_address();
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new_readonly(MINT_ADDRESS, false),
            AccountMeta::new(otc_treasury_address, false),
            AccountMeta::new(otc_treasury_tokens, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(spl_token::ID, false),
            AccountMeta::new_readonly(spl_associated_token_account::ID, false),
        ],
        data: InitializeOtcTreasury {}.to_bytes(),
    }
}

pub fn fund_godl_otc(signer: Pubkey, amount: u64) -> Instruction {
    let otc_treasury_address = otc_treasury_pda().0;
    let otc_treasury_tokens = otc_treasury_tokens_address();
    let signer_tokens = get_associated_token_address(&signer, &MINT_ADDRESS);
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new_readonly(MINT_ADDRESS, false),
            AccountMeta::new(signer_tokens, false),
            AccountMeta::new(otc_treasury_address, false),
            AccountMeta::new(otc_treasury_tokens, false),
            AccountMeta::new_readonly(spl_token::ID, false),
        ],
        data: FundGodlOtc {
            amount: amount.to_le_bytes(),
        }
        .to_bytes(),
    }
}

pub fn execute_otc_trade(
    buyer: Pubkey,
    stake_id: u64,
    sol_in: u64,
    godl_out: u64,
    godl_bonus: u64,
    expiry_slot: u64,
    lock_duration: i64,
) -> Instruction {
    let miner_address = miner_pda(buyer).0;
    let otc_user_address = otc_user_pda(buyer).0;
    let otc_treasury_address = otc_treasury_pda().0;
    let otc_treasury_tokens = otc_treasury_tokens_address();
    let treasury_address = treasury_pda().0;
    let treasury_tokens = treasury_tokens_address(treasury_address);
    let stake_address = stake_v2_pda(buyer, stake_id).0;
    let stake_tokens = get_associated_token_address(&stake_address, &MINT_ADDRESS);
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(buyer, true),
            AccountMeta::new_readonly(OTC_ORACLE_SIGNER, true),
            AccountMeta::new_readonly(MINT_ADDRESS, false),
            AccountMeta::new(miner_address, false),
            AccountMeta::new(otc_user_address, false),
            AccountMeta::new(otc_treasury_address, false),
            AccountMeta::new(otc_treasury_tokens, false),
            AccountMeta::new(treasury_address, false),
            AccountMeta::new(treasury_tokens, false),
            AccountMeta::new(stake_address, false),
            AccountMeta::new(stake_tokens, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(spl_token::ID, false),
            AccountMeta::new_readonly(spl_associated_token_account::ID, false),
        ],
        data: ExecuteOtcTrade {
            stake_id: stake_id.to_le_bytes(),
            sol_in: sol_in.to_le_bytes(),
            godl_out: godl_out.to_le_bytes(),
            godl_bonus: godl_bonus.to_le_bytes(),
            expiry_slot: expiry_slot.to_le_bytes(),
            lock_duration: lock_duration.to_le_bytes(),
        }
        .to_bytes(),
    }
}

pub fn withdraw_sol_otc(signer: Pubkey) -> Instruction {
    let otc_treasury_address = otc_treasury_pda().0;
    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(signer, true),
            AccountMeta::new(otc_treasury_address, false),
            AccountMeta::new(CHEST_ADDRESS, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: WithdrawSolOtc {}.to_bytes(),
    }
}
