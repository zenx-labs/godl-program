use serde::{Deserialize, Serialize};
use steel::*;

use crate::state::otc_user_pda;

use super::GodlAccount;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct OtcUser {
    /// The authority of this OTC user account.
    pub authority: Pubkey,

    /// The cumulative SOL the user has paid into the OTC desk.
    pub total_sol_in: u64,

    /// The cumulative GODL the user has received as locked stake from the OTC desk.
    pub total_godl_out: u64,

    /// The cumulative GODL bonus the user has received as unrefined miner rewards from the OTC desk.
    pub total_godl_bonus: u64,

    /// Buffer for future use.
    pub buffer: [u8; 32],
}

impl OtcUser {
    pub fn pda(&self) -> (Pubkey, u8) {
        otc_user_pda(self.authority)
    }
}

account!(GodlAccount, OtcUser);
