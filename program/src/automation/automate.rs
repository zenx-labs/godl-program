use godl_api::prelude::*;
use steel::*;

/// Sets automation parameters including pooled deployments.
pub fn process_automate(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    // Parse data.
    let args = Automate::try_from_bytes(data)?;
    let amount = u64::from_le_bytes(args.amount);
    let deposit = u64::from_le_bytes(args.deposit);
    let fee = u64::from_le_bytes(args.fee);
    let mask = u64::from_le_bytes(args.mask);
    let strategy = AutomationV2Strategy::from_u64(args.strategy as u64);
    let claim_and_fund = args.claim_and_fund == 1;
    let is_pooled = args.is_pooled == 1;

    // Load accounts.
    let [signer_info, automation_v2_info, executor_info, miner_info, pool_member_info, system_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    automation_v2_info
        .is_writable()?
        .has_seeds(&[AUTOMATION_V2, &signer_info.key.to_bytes()], &godl_api::ID)?;
    miner_info
        .is_writable()?
        .has_seeds(&[MINER, &signer_info.key.to_bytes()], &godl_api::ID)?;
    pool_member_info
        .is_writable()?
        .has_seeds(&[POOL_MEMBER, &signer_info.key.to_bytes()], &godl_api::ID)?;
    system_program.is_program(&system_program::ID)?;

    // Open miner account.
    let miner = if miner_info.data_is_empty() {
        create_program_account::<Miner>(
            miner_info,
            system_program,
            &signer_info,
            &godl_api::ID,
            &[MINER, &signer_info.key.to_bytes()],
        )?;
        let miner = miner_info.as_account_mut::<Miner>(&godl_api::ID)?;
        miner.authority = *signer_info.key;
        miner.referrer = Pubkey::default();
        miner.deployed = [0; 25];
        miner.cumulative = [0; 25];
        miner.checkpoint_fee = 0;
        miner.checkpoint_id = 0;
        miner.rewards_sol = 0;
        miner.rewards_godl = 0;
        miner.round_id = 0;
        miner.lifetime_deployed = 0;
        miner.lifetime_rewards_sol = 0;
        miner.lifetime_rewards_godl = 0;
        miner
    } else {
        miner_info
            .as_account_mut::<Miner>(&godl_api::ID)?
            .assert_mut_err(
                |m| m.authority == *signer_info.key,
                GodlError::NotAuthorized.into(),
            )?
    };

    // Close account if executor is Pubkey::default().
    if *executor_info.key == Pubkey::default() {
        automation_v2_info
            .as_account_mut::<AutomationV2>(&godl_api::ID)?
            .assert_mut_err(
                |a| a.authority == *signer_info.key,
                GodlError::NotAuthorized.into(),
            )?;
        automation_v2_info.close(signer_info)?;
        return Ok(());
    }

    // Open pool member account when opting into pooling.
    if is_pooled && pool_member_info.data_is_empty() {
        create_program_account::<PoolMember>(
            pool_member_info,
            system_program,
            signer_info,
            &godl_api::ID,
            &[POOL_MEMBER, &signer_info.key.to_bytes()],
        )?;
        let pool_member = pool_member_info.as_account_mut::<PoolMember>(&godl_api::ID)?;
        pool_member.authority = *signer_info.key;
        pool_member.round_id = 0;
        pool_member.deployed = [0; 25];
        pool_member.total_deployed = 0;
    } else if !pool_member_info.data_is_empty() {
        pool_member_info
            .as_account::<PoolMember>(&godl_api::ID)?
            .assert(|p| p.authority == *signer_info.key)?;
    }

    // Create automation.
    let automation_v2 = if automation_v2_info.data_is_empty() {
        create_program_account::<AutomationV2>(
            automation_v2_info,
            system_program,
            signer_info,
            &godl_api::ID,
            &[AUTOMATION_V2, &signer_info.key.to_bytes()],
        )?;
        let automation_v2 = automation_v2_info.as_account_mut::<AutomationV2>(&godl_api::ID)?;
        automation_v2.balance = 0;
        automation_v2.authority = *signer_info.key;
        automation_v2
    } else {
        automation_v2_info
            .as_account_mut::<AutomationV2>(&godl_api::ID)?
            .assert_mut_err(
                |a| a.authority == *signer_info.key,
                GodlError::NotAuthorized.into(),
            )?
    };

    // Set strategy and mask.
    automation_v2.amount = amount;
    automation_v2.balance += deposit;
    automation_v2.executor = *executor_info.key;
    automation_v2.fee = fee;
    automation_v2.mask = mask;
    automation_v2.set_strategy(strategy);
    automation_v2.set_claim_and_fund(claim_and_fund);
    automation_v2.set_is_pooled(is_pooled);

    // Top up checkpoint fee.
    if miner.checkpoint_fee == 0 {
        miner.checkpoint_fee = CHECKPOINT_FEE;
        miner_info.collect(CHECKPOINT_FEE, &signer_info)?;
    }

    // Transfer balance to executor.
    automation_v2_info.collect(deposit, signer_info)?;

    Ok(())
}
