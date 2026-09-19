use serde::{Deserialize, Serialize};
use steel::*;

pub enum GodlEvent {
    Reset = 0,
    Bury = 1,
    Deploy = 2,
    BuyGold = 3,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct ResetEvent {
    /// The event discriminator.
    pub disc: u64,

    /// The block that was opened for trading.
    pub round_id: u64,

    /// The start slot of the next block.
    pub start_slot: u64,

    /// The end slot of the next block.
    pub end_slot: u64,

    /// The winning square of the round.
    pub winning_square: u64,

    /// The top miner of the round.
    pub top_miner: Pubkey,

    /// The number of miners on the winning square.
    pub num_winners: u64,

    /// The amount of GODL payout for the motherlode.
    pub motherlode: u64,

    /// The total amount of SOL prospected in the round.
    pub total_deployed: u64,

    /// The total amount of SOL put in the GODL vault.
    pub total_vaulted: u64,

    /// The total amount of SOL won by miners for the round.
    pub total_winnings: u64,

    /// The total amount of GODL minted for the round.
    pub total_minted: u64,

    /// The timestamp of the event.
    pub ts: i64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct BuryEvent {
    /// The event discriminator.
    pub disc: u64,

    /// The amount of GODL buried.
    pub godl_buried: u64,

    /// The amount of GODL shared with stakers.
    pub godl_shared: u64,

    /// The amount of SOL swapped.
    pub sol_amount: u64,

    /// The new circulating supply of GODL.
    pub new_circulating_supply: u64,

    /// The timestamp of the event.
    pub ts: i64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct DeployEvent {
    /// The event discriminator.
    pub disc: u64,

    /// The authority of the deployer.
    pub authority: Pubkey,

    /// The amount of SOL deployed per square.
    pub amount: u64,

    /// The mask of the squares deployed to.
    pub mask: u64,

    /// The round id.
    pub round_id: u64,

    /// The timestamp of the event.
    pub ts: i64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct BuyGoldEvent {
    /// The event discriminator.
    pub disc: u64,

    /// The amount of SOL swapped (0 for a manual deposit).
    pub sol_amount: u64,

    /// The amount of XAUt0 received.
    pub xaut_amount: u64,

    /// The amount of XAUt0 credited to the rewards factor (includes previously pending XAUt0).
    pub xaut_distributed: u64,

    /// The unrefined GODL supply the distribution was split across.
    pub total_unclaimed: u64,

    /// The timestamp of the event.
    pub ts: i64,
}

event!(ResetEvent);
event!(BuryEvent);
event!(DeployEvent);
event!(BuyGoldEvent);
