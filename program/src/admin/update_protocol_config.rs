use godl_api::prelude::*;
use solana_program::{rent::Rent, sysvar::Sysvar};
use steel::*;

/// Sets the round length and intermission, in slots (admin only).
///
/// Mirrors ore's `UpdateProtocolConfig`: both values live in `Config` so the operator can
/// retune them when Solana's slot time moves, without a program upgrade. `Deploy` reads
/// `round_slots` when it opens a round and `Reset` waits `intermission_slots` past `end_slot`.
///
/// The first call after the upgrade that added these fields also grows the mainnet config
/// account to the current layout, with the signer topping up rent. Every `Config` read fails
/// until that happens (steel checks the exact account length), so this call must follow the
/// upgrade immediately. The new fields are appended, so the old ones keep their offsets.
pub fn process_update_protocol_config(accounts: &[AccountInfo<'_>], data: &[u8]) -> ProgramResult {
    // Parse data.
    let args = UpdateProtocolConfig::try_from_bytes(data)?;
    let round_slots = u64::from_le_bytes(args.round_slots);
    let intermission_slots = u64::from_le_bytes(args.intermission_slots);

    // Load accounts.
    let [signer_info, config_info, system_program] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    signer_info.is_signer()?;
    config_info
        .is_writable()?
        .has_seeds(&[CONFIG], &godl_api::ID)?;
    system_program.is_program(&system_program::ID)?;

    // One-time migration: grow a pre-timing config account to the current layout. The added
    // bytes are zeroed, so the new fields read as unset until they are assigned below.
    let expected_len = 8 + core::mem::size_of::<Config>();
    if config_info.data_len() < expected_len {
        config_info.has_owner(&godl_api::ID)?;
        let required = Rent::get()?.minimum_balance(expected_len);
        let shortfall = required.saturating_sub(config_info.lamports());
        if shortfall > 0 {
            config_info.collect(shortfall, signer_info)?;
        }
        config_info.resize(expected_len)?;
    }
    let config = config_info
        .as_account_mut::<Config>(&godl_api::ID)?
        .assert_mut_err(
            |c| c.admin == *signer_info.key,
            GodlError::NotAuthorized.into(),
        )?;

    // Validate. Zero is reserved for "unset" and a day caps a fat-fingered value.
    if round_slots == 0
        || round_slots > ONE_DAY_SLOTS
        || intermission_slots == 0
        || intermission_slots > ONE_DAY_SLOTS
    {
        return Err(GodlError::InvalidProtocolConfig.into());
    }

    // Set protocol config.
    config.round_slots = round_slots;
    config.intermission_slots = intermission_slots;

    Ok(())
}
