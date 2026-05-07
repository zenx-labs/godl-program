use serde::{Deserialize, Serialize};
use steel::*;

use crate::state::otc_treasury_pda;

use super::GodlAccount;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct OtcTreasury {
    /// The amount of SOL in the OTC treasury.
    pub sol_balance: u64,

    /// The amount of GODL in the OTC treasury.
    pub godl_balance: u64,

    /// buffer for future use
    pub buffer: [u8; 32],
}

impl OtcTreasury {
    pub fn pda(&self) -> (Pubkey, u8) {
        otc_treasury_pda()
    }
}

account!(GodlAccount, OtcTreasury);
