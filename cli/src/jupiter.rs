//! Jupiter Swap V2 `/build` client.
//!
//! All Jupiter HTTP concerns live here: request construction, auth, response
//! deserialization, and conversion into Solana SDK types. Callers receive a
//! [`SwapBuild`] containing instructions and lookup tables ready to splice
//! into a transaction.

use anyhow::{anyhow, Result};
use base64::Engine;
use serde::Deserialize;
use solana_sdk::{
    address_lookup_table::AddressLookupTableAccount,
    instruction::{AccountMeta, Instruction},
    pubkey,
    pubkey::Pubkey,
};
use std::{collections::HashMap, str::FromStr};

pub const DEFAULT_BASE_URL: &str = "https://proxy.godl.dev/api.jup.ag/swap/v2";

const WSOL_MINT: Pubkey = pubkey!("So11111111111111111111111111111111111111112");
const GODL_MINT: Pubkey = pubkey!("GodL6KZ9uuUoQwELggtVzQkKmU1LfqmDokPibPeDKkhF");

/// Cap on accounts the router may include. 64 is the protocol max; 55 leaves
/// headroom for our wrapping `pre_bury`/`bury` and any setup instructions.
const MAX_ACCOUNTS: u32 = 55;

/// Restrict routing to Meteora pools that have direct SOL↔GODL liquidity, so
/// `RouteV2` (which requires the taker to own every intermediate token
/// account) doesn't need to create new ATAs we'd have to manage later.
const DEXES_ALLOWLIST: &str = "Meteora DAMM v2";

/// Parsed `/build` response, ready to splice into a transaction alongside the
/// GODL `pre_bury` / `bury` instructions.
pub struct SwapBuild {
    pub setup_ixs: Vec<Instruction>,
    pub swap_accounts: Vec<AccountMeta>,
    pub swap_data: Vec<u8>,
    pub cleanup_ix: Option<Instruction>,
    pub lut_accounts: Vec<AddressLookupTableAccount>,
}

/// Lightweight wrapper around Jupiter's `/build` endpoint.
pub struct JupiterClient {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

impl JupiterClient {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        Self {
            base_url,
            api_key: api_key.into(),
            http: reqwest::Client::new(),
        }
    }

    /// GET `/build` for a SOL → GODL swap. Returns the parsed instructions and
    /// lookup tables; the caller assembles the final transaction.
    pub async fn build_sol_to_godl(
        &self,
        taker: Pubkey,
        payer: Pubkey,
        amount: u64,
    ) -> Result<SwapBuild> {
        let url = format!("{}/build", self.base_url);

        let resp = self
            .http
            .get(&url)
            .header("x-api-key", &self.api_key)
            .query(&[
                ("inputMint", WSOL_MINT.to_string()),
                ("outputMint", GODL_MINT.to_string()),
                ("amount", amount.to_string()),
                ("taker", taker.to_string()),
                ("payer", payer.to_string()),
                ("maxAccounts", MAX_ACCOUNTS.to_string()),
                ("slippageBps", "rtse".to_string()),
                ("wrapAndUnwrapSol", "false".to_string()),
                ("dexes", DEXES_ALLOWLIST.to_string()),
            ])
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("/build request failed: {status} {body}"));
        }

        resp.json::<BuildResponse>().await?.try_into()
    }
}

// --- response deserialization (private) --------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuildResponse {
    #[serde(default)]
    setup_instructions: Vec<ApiInstruction>,
    swap_instruction: ApiInstruction,
    #[serde(default)]
    cleanup_instruction: Option<ApiInstruction>,
    #[serde(default)]
    addresses_by_lookup_table_address: Option<HashMap<String, Vec<String>>>,
}

impl TryFrom<BuildResponse> for SwapBuild {
    type Error = anyhow::Error;

    fn try_from(resp: BuildResponse) -> Result<Self> {
        let setup_ixs = resp
            .setup_instructions
            .into_iter()
            .map(parse_instruction)
            .collect::<Result<_>>()?;

        let swap_ix = parse_instruction(resp.swap_instruction)?;

        let cleanup_ix = resp
            .cleanup_instruction
            .map(parse_instruction)
            .transpose()?;

        let lut_accounts = resp
            .addresses_by_lookup_table_address
            .unwrap_or_default()
            .into_iter()
            .map(|(key, addrs)| {
                Ok(AddressLookupTableAccount {
                    key: Pubkey::from_str(&key)?,
                    addresses: addrs
                        .into_iter()
                        .map(|s| Pubkey::from_str(&s).map_err(anyhow::Error::from))
                        .collect::<Result<_>>()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(SwapBuild {
            setup_ixs,
            swap_accounts: swap_ix.accounts,
            swap_data: swap_ix.data,
            cleanup_ix,
            lut_accounts,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiInstruction {
    program_id: String,
    accounts: Vec<ApiAccount>,
    data: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiAccount {
    pubkey: String,
    is_writable: bool,
    is_signer: bool,
}

fn parse_account(a: ApiAccount) -> Result<AccountMeta> {
    Ok(AccountMeta {
        pubkey: Pubkey::from_str(&a.pubkey)?,
        is_signer: a.is_signer,
        is_writable: a.is_writable,
    })
}

fn parse_instruction(ix: ApiInstruction) -> Result<Instruction> {
    Ok(Instruction {
        program_id: Pubkey::from_str(&ix.program_id)?,
        accounts: ix
            .accounts
            .into_iter()
            .map(parse_account)
            .collect::<Result<_>>()?,
        data: base64::engine::general_purpose::STANDARD.decode(&ix.data)?,
    })
}


