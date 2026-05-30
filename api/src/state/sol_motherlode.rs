use serde::{Deserialize, Serialize};
use steel::*;

use super::GodlAccount;

/// SolMotherlode is a singleton account which contains the amount of SOL in the motherlode rewards pool.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct SolMotherlode {
    /// The amount of SOL in the motherlode rewards pool.
    pub amount: u64,
}

account!(GodlAccount, SolMotherlode);
