use godl_api::prelude::*;
use solana_program::log::sol_log;
use spl_token::amount_to_ui_amount;
use steel::*;

/// Injects unrefined GODL rewards into a miner account.
pub fn process_inject_unrefined_rewards(
    accounts: &[AccountInfo<'_>],
    data: &[u8],
) -> ProgramResult {
    // Parse data.
    let args = InjectUnrefinedRewards::try_from_bytes(data)?;
    let amount = u64::from_le_bytes(args.amount);

    // Load accounts.
    let [signer_info, config_info, miner_info, treasury_info, signer_tokens_info, treasury_tokens_info, token_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    config_info
        .as_account::<Config>(&godl_api::ID)?
        .assert(|c| c.bury_authority == *signer_info.key)?;
    miner_info.is_writable()?;
    let miner_authority = miner_info.as_account::<Miner>(&godl_api::ID)?.authority;
    miner_info.has_seeds(&[MINER, &miner_authority.to_bytes()], &godl_api::ID)?;
    let miner = miner_info.as_account_mut::<Miner>(&godl_api::ID)?;
    treasury_info
        .is_writable()?
        .has_seeds(&[TREASURY], &godl_api::ID)?;
    let treasury = treasury_info.as_account_mut::<Treasury>(&godl_api::ID)?;
    signer_tokens_info
        .is_writable()?
        .as_associated_token_account(signer_info.key, &MINT_ADDRESS)?;
    treasury_tokens_info
        .is_writable()?
        .as_associated_token_account(&treasury_info.key, &MINT_ADDRESS)?;
    token_program.is_program(&spl_token::ID)?;

    // Basic safety checks.
    if amount == 0 {
        return Err(GodlError::AmountTooSmall.into());
    }

    // Transfer GODL from signer to treasury.
    transfer(
        signer_info,
        signer_tokens_info,
        treasury_tokens_info,
        token_program,
        amount,
    )?;

    // Update rewards before adding new unrefined rewards.
    miner.update_rewards(treasury);

    // Inject unrefined rewards.
    miner.rewards_godl = miner
        .rewards_godl
        .checked_add(amount)
        .ok_or(ProgramError::InvalidInstructionData)?;
    miner.lifetime_rewards_godl = miner
        .lifetime_rewards_godl
        .checked_add(amount)
        .ok_or(ProgramError::InvalidInstructionData)?;
    treasury.total_unclaimed = treasury
        .total_unclaimed
        .checked_add(amount)
        .ok_or(ProgramError::InvalidInstructionData)?;

    sol_log(
        &format!(
            "Injected {} unrefined GODL to miner {}",
            amount_to_ui_amount(amount, TOKEN_DECIMALS),
            miner_authority
        )
        .as_str(),
    );

    Ok(())
}
