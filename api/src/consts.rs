use solana_program::{pubkey, pubkey::Pubkey};

/// The authority allowed to initialize the program.
pub const DEPLOYER_ADDRESS: Pubkey = pubkey!("devwFHLkNQ4bBpBLfshAiyfr3Pie5ZNrChpKM7dZ4kF");

/// The decimal precision of the GODL token.
/// There are 100 billion indivisible units per GODL (called "grams").
pub const TOKEN_DECIMALS: u8 = 11;

/// One GODL token, denominated in indivisible units.
pub const ONE_GODL: u64 = 10u64.pow(TOKEN_DECIMALS as u32);

/// The duration of one minute, in seconds.
pub const ONE_MINUTE: i64 = 60;

/// The duration of one hour, in seconds.
pub const ONE_HOUR: i64 = 60 * ONE_MINUTE;

/// The duration of one day, in seconds.
pub const ONE_DAY: i64 = 24 * ONE_HOUR;

/// The number of seconds for when the winning square expires.
pub const ONE_WEEK: i64 = 7 * ONE_DAY;

/// The number of slots in one week.
pub const ONE_MINUTE_SLOTS: u64 = 150;

/// The number of slots in one hour.
pub const ONE_HOUR_SLOTS: u64 = 60 * ONE_MINUTE_SLOTS;

/// The number of slots in 12 hours.
pub const TWELVE_HOURS_SLOTS: u64 = 12 * ONE_HOUR_SLOTS;

/// The number of slots in one day.
pub const ONE_DAY_SLOTS: u64 = 24 * ONE_HOUR_SLOTS;

/// The number of slots in one week.
pub const ONE_WEEK_SLOTS: u64 = 7 * ONE_DAY_SLOTS;

/// The number of slots for breather between rounds.
pub const INTERMISSION_SLOTS: u64 = 25;

/// The maximum token supply (2.1 million).
pub const MAX_SUPPLY: u64 = ONE_GODL * 2_100_000;

/// The maximum GODL per round.
pub const MAX_GODL_PER_ROUND: u64 = ONE_GODL * 100;

/// The seed of the automation account PDA.
pub const AUTOMATION: &[u8] = b"automation";

/// The seed of the automation_v2 account PDA.
pub const AUTOMATION_V2: &[u8] = b"automation_v2";

/// The seed of the board account PDA.
pub const BOARD: &[u8] = b"board";

/// The seed of the config account PDA.
pub const CONFIG: &[u8] = b"config";

/// The seed of the miner account PDA.
pub const MINER: &[u8] = b"miner";

/// The seed of the referrer account PDA.
pub const REFERRER: &[u8] = b"referrer";

/// Sentinel used to mark a miner as locked without a referrer.
pub const REFERRER_LOCKED_SENTINEL: Pubkey = Pubkey::new_from_array([0xFF; 32]);

/// The seed of the stake account PDA.
pub const STAKE: &[u8] = b"stake";

/// The seed of the stake_v2 account PDA.
pub const STAKE_V2: &[u8] = b"stake_v2";

/// The seed of the round account PDA.
pub const ROUND: &[u8] = b"round";

/// The seed of the pool round account PDA.
pub const POOL_ROUND: &[u8] = b"pool_round";

/// The seed of the pool member account PDA.
pub const POOL_MEMBER: &[u8] = b"pool_member";

/// The seed of the treasury account PDA.
pub const TREASURY: &[u8] = b"treasury";

/// The seed of the sol motherlode account PDA.
pub const SOL_MOTHERLODE: &[u8] = b"sol_motherlode";

/// The seed of OTC treasury account PDA.
pub const OTC_TREASURY: &[u8] = b"otc_treasury";

/// The seed of the per-user OTC account PDA.
pub const OTC_USER: &[u8] = b"otc_user";

/// The duration of one month, in seconds (30 days).
pub const ONE_MONTH: i64 = 30 * ONE_DAY;

/// The address of the mint account.
pub const MINT_ADDRESS: Pubkey = pubkey!("GodL6KZ9uuUoQwELggtVzQkKmU1LfqmDokPibPeDKkhF");

/// The address of the sol mint account.
pub const SOL_MINT: Pubkey = pubkey!("So11111111111111111111111111111111111111112");

/// The address to indicate GODL rewards are split between all miners.
pub const SPLIT_ADDRESS: Pubkey = pubkey!("SpLiT11111111111111111111111111111111111112");

/// The address used to signal a pooled top miner win.
pub const POOL_ADDRESS: Pubkey = pubkey!("PooL111111111111111111111111111111111111111");

/// Admin fee in basis points.
pub const ADMIN_FEE_BPS: u64 = 50;

/// Sol motherlode fee in basis points.
pub const SOL_MOTHERLODE_BPS: u64 = 100;

/// Referral share in basis points.
pub const REFERRAL_BPS: u64 = 50;

/// Stakers share in basis points from bury.
pub const STAKERS_BPS: u64 = 300; // 3%

/// Admin share in basis points from bury.
pub const ADMIN_BPS: u64 = 0; // 0%

/// Denominator for fee calculations.
pub const DENOMINATOR_BPS: u64 = 10_000;

/// The fee paid to bots if they checkpoint a user.
pub const CHECKPOINT_FEE: u64 = 10_000; // 0.00001 SOL

/// The addres of the chest account.
pub const CHEST_ADDRESS: Pubkey = pubkey!("CHESThUvZPLLSYGa6YKD1LBDe5ktz9KEKnDGDdTiEcmU");

/// The address of the crank bot.
pub const CRANK_BOT: Pubkey = pubkey!("CrAnKBPND9W77vpPQioULYrbZeTJDnaLTRZ8WNr6AWPz");

/// The address of the admin sol fee account.
pub const ADMIN_SOL_FEE: Pubkey = pubkey!("7Y7E5QpdQeqAAqjPPMhZRdTF97bcNH6oULMcyvqGzH7k");

/// The address of the admin godl fee account.
pub const ADMIN_GODL_FEE: Pubkey = pubkey!("24vRA7Wh97HPRYiwD5WgiNGYeKdtG9Ajz73cJdg2ZGhR");

/// The NFT collection eligible for stake boosting.
pub const NFT_BOOST_COLLECTION: Pubkey = pubkey!("DTzRfkhPq73e1m3Fwu7AMPie5aNWwMGQMMxgdBhGATgY");

/// The MPL Core program address.
pub const MPL_CORE_PROGRAM: Pubkey = pubkey!("CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d");

/// NFT boost numerator (11/10 = 1.10x = +10%).
pub const NFT_BOOST_NUMERATOR: u128 = 11;

/// NFT boost denominator.
pub const NFT_BOOST_DENOMINATOR: u128 = 10;

/// Stake V2 multiplier constants
pub const MAX_LOCK_DURATION: i64 = 2 * 365 * ONE_DAY;
/// Maximum stake multiplier (e.g. 20x)
pub const MAX_STAKE_MULTIPLIER: u64 = 20;
/// Fixed-point scale for stake multipliers
pub const STAKE_MULTIPLIER_SCALE: u64 = 1_000_000_000;
/// Curve exponent (currently unused; linear curve)
pub const STAKE_CURVE_EXPONENT: f64 = 1.0;

/// OTC Oracle signer
pub const OTC_ORACLE_SIGNER: Pubkey = pubkey!("otcSPJAKfmCgMmFkKUGk2kjwSQLSCHw5MJrJyouNFKt");
