use serde::{Deserialize, Serialize};
use steel::*;

use crate::{
    consts::{INTERMISSION_SLOTS, ROUND_SLOTS},
    state::config_pda,
};

use super::GodlAccount;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct Config {
    /// The address that can update the config.
    pub admin: Pubkey,

    /// The amount of GODL minted per round.
    pub godl_per_round: u64,

    /// The adress with authority to call bury.
    pub bury_authority: Pubkey,

    /// The address that receives admin fees.
    pub fee_collector: Pubkey,

    /// The program id for the protocol swaps.
    pub swap_program: Pubkey,

    /// The address of the entropy var account.
    pub var_address: Pubkey,

    /// The denominator to calculate the motherlode.
    pub motherlode_denominator: u64,

    /// The number of mining slots in one round (`board.end_slot = start_slot + round_slots`).
    /// Zero means unset and falls back to `ROUND_SLOTS`. Appended after the original layout so
    /// a resized mainnet account keeps every earlier field at its offset.
    pub round_slots: u64,

    /// The number of slots between a round's `end_slot` and the earliest reset. Zero means
    /// unset and falls back to `INTERMISSION_SLOTS`.
    pub intermission_slots: u64,

    /// Reserved for future fields, so the next addition needs no account resize.
    pub buffer: [u8; 32],
}

impl Config {
    pub fn pda() -> (Pubkey, u8) {
        config_pda()
    }

    /// Mining slots per round, falling back to the compile-time default when unset.
    pub fn round_slots(&self) -> u64 {
        if self.round_slots == 0 {
            ROUND_SLOTS
        } else {
            self.round_slots
        }
    }

    /// Intermission slots after `end_slot`, falling back to the compile-time default when unset.
    pub fn intermission_slots(&self) -> u64 {
        if self.intermission_slots == 0 {
            INTERMISSION_SLOTS
        } else {
            self.intermission_slots
        }
    }
}

account!(GodlAccount, Config);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_offsets_are_frozen() {
        // Raw account offsets = struct offsets + 8 (steel discriminant). The pre-timing layout
        // was 176 bytes (184 on chain); the timing fields and buffer are appended, so
        // `UpdateProtocolConfig` can grow a live account in place.
        assert_eq!(core::mem::size_of::<Config>(), 224);
        assert_eq!(core::mem::offset_of!(Config, motherlode_denominator), 168);
        assert_eq!(core::mem::offset_of!(Config, round_slots), 176);
        assert_eq!(core::mem::offset_of!(Config, intermission_slots), 184);
        assert_eq!(core::mem::offset_of!(Config, buffer), 192);
    }

    #[test]
    fn zero_timing_fields_fall_back_to_defaults() {
        let mut c = Config::zeroed();
        assert_eq!(c.round_slots(), ROUND_SLOTS);
        assert_eq!(c.intermission_slots(), INTERMISSION_SLOTS);
        c.round_slots = 300;
        c.intermission_slots = 50;
        assert_eq!(c.round_slots(), 300);
        assert_eq!(c.intermission_slots(), 50);
    }
}
