//! The handful of Solana JSON-RPC calls the trade flow needs, over reqwest.

use anyhow::{anyhow, bail, Result};
use base64::Engine as _;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::jupiter::LAMPORTS_PER_SOL;

pub struct Rpc {
    client: reqwest::Client,
    url: String,
}

pub struct Simulation {
    /// `None` when the transaction would succeed.
    pub err: Option<Value>,
    pub logs: Vec<String>,
    /// What the transaction would leave in each account asked about, in the
    /// order asked: `None` for one it does not load. Empty when it fails.
    pub accounts: Vec<Option<Account>>,
}

/// An account as the node holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct Account {
    pub lamports: u64,
    /// The program the account belongs to.
    pub owner: String,
    pub data: Vec<u8>,
}

impl Account {
    /// From the node's JSON, with the data in base64. `None` for `null`
    /// (no such account) or anything that is not an account.
    fn read(v: &Value) -> Option<Self> {
        Some(Self {
            lamports: v["lamports"].as_u64()?,
            owner: v["owner"].as_str()?.to_owned(),
            data: base64::engine::general_purpose::STANDARD
                .decode(v["data"][0].as_str()?)
                .ok()?,
        })
    }
}

pub struct SignatureStatus {
    pub err: Option<Value>,
    /// "processed", "confirmed" or "finalized".
    pub confirmation: Option<String>,
}

/// What a confirmed swap did to the wallet, from the transaction's metadata.
pub struct Settlement {
    /// Change in the wallet's balance of the token, in whole units.
    pub token_delta: f64,
    pub fee_sol: f64,
}

impl Rpc {
    pub fn new(client: reqwest::Client, url: String) -> Self {
        Self { client, url }
    }

    async fn call<T: DeserializeOwned>(&self, method: &str, params: Value) -> Result<T> {
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
        let resp: Value = self
            .client
            .post(&self.url)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if let Some(err) = resp.get("error") {
            let msg = err
                .get("message")
                .and_then(Value::as_str)
                .map_or_else(|| err.to_string(), str::to_owned);
            bail!("{method}: {msg}");
        }
        Ok(serde_json::from_value(resp["result"].clone())?)
    }

    pub async fn sol_balance(&self, owner: &str) -> Result<f64> {
        let r: Value = self
            .call("getBalance", json!([owner, { "commitment": "confirmed" }]))
            .await?;
        let lamports = r["value"]
            .as_u64()
            .ok_or_else(|| anyhow!("getBalance: no value"))?;
        Ok(lamports as f64 / LAMPORTS_PER_SOL)
    }

    /// Sum over all of the owner's accounts for this mint; 0 when there are none.
    pub async fn token_balance(&self, owner: &str, mint: &str) -> Result<f64> {
        let r: Value = self
            .call(
                "getTokenAccountsByOwner",
                json!([owner, { "mint": mint }, { "encoding": "jsonParsed", "commitment": "confirmed" }]),
            )
            .await?;
        let accounts = r["value"]
            .as_array()
            .ok_or_else(|| anyhow!("getTokenAccountsByOwner: no value"))?;
        Ok(accounts
            .iter()
            .filter_map(|a| {
                a["account"]["data"]["parsed"]["info"]["tokenAmount"]["uiAmount"].as_f64()
            })
            // Not `.sum()`: an empty f64 sum is -0.0, which displays as "-0.00".
            .fold(0.0, |acc, v| acc + v))
    }

    /// The wallet's balance in lamports, as of the newest block the node has:
    /// what the simulation starts from.
    pub async fn lamports(&self, owner: &str) -> Result<u64> {
        let r: Value = self
            .call("getBalance", json!([owner, { "commitment": "processed" }]))
            .await?;
        r["value"]
            .as_u64()
            .ok_or_else(|| anyhow!("getBalance: no value"))
    }

    /// The owner's accounts for this mint, as of the newest block the node has.
    pub async fn token_accounts(&self, owner: &str, mint: &str) -> Result<Vec<(String, Account)>> {
        let r: Value = self
            .call(
                "getTokenAccountsByOwner",
                json!([owner, { "mint": mint }, { "encoding": "base64", "commitment": "processed" }]),
            )
            .await?;
        r["value"]
            .as_array()
            .ok_or_else(|| anyhow!("getTokenAccountsByOwner: no value"))?
            .iter()
            .map(|a| {
                let address = a["pubkey"].as_str()?.to_owned();
                Some((address, Account::read(&a["account"])?))
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| anyhow!("getTokenAccountsByOwner: an account could not be read"))
    }

    /// Runs the signed transaction without sending it, and reports what it
    /// would leave in each of `watched`.
    pub async fn simulate(&self, tx_b64: &str, watched: &[String]) -> Result<Simulation> {
        self.simulate_with(tx_b64, watched, json!({ "sigVerify": true }))
            .await
    }

    async fn simulate_with(
        &self,
        tx_b64: &str,
        watched: &[String],
        mut config: Value,
    ) -> Result<Simulation> {
        config["encoding"] = json!("base64");
        config["commitment"] = json!("processed");
        config["accounts"] = json!({ "encoding": "base64", "addresses": watched });
        let r: Value = self
            .call("simulateTransaction", json!([tx_b64, config]))
            .await?;
        let v = &r["value"];
        Ok(Simulation {
            err: v.get("err").filter(|e| !e.is_null()).cloned(),
            logs: v["logs"]
                .as_array()
                .map(|l| {
                    l.iter()
                        .filter_map(|s| s.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default(),
            accounts: v["accounts"]
                .as_array()
                .map(|l| l.iter().map(Account::read).collect())
                .unwrap_or_default(),
        })
    }

    /// The same for a transaction nobody signed, as if it had been: how the
    /// tests see a swap through for a wallet whose key they do not hold.
    #[cfg(test)]
    pub async fn simulate_unsigned(&self, tx_b64: &str, watched: &[String]) -> Result<Simulation> {
        self.simulate_with(
            tx_b64,
            watched,
            json!({ "sigVerify": false, "replaceRecentBlockhash": true }),
        )
        .await
    }

    /// Preflight is skipped because the same bytes were just simulated. The
    /// caller re-sends until confirmed, so the node does not retry either.
    pub async fn send(&self, tx_b64: &str) -> Result<String> {
        self.call(
            "sendTransaction",
            json!([tx_b64, { "encoding": "base64", "skipPreflight": true, "maxRetries": 0 }]),
        )
        .await
    }

    pub async fn signature_status(&self, sig: &str) -> Result<Option<SignatureStatus>> {
        let r: Value = self.call("getSignatureStatuses", json!([[sig]])).await?;
        let s = &r["value"][0];
        if s.is_null() {
            return Ok(None);
        }
        Ok(Some(SignatureStatus {
            err: s.get("err").filter(|e| !e.is_null()).cloned(),
            confirmation: s["confirmationStatus"].as_str().map(str::to_owned),
        }))
    }

    pub async fn block_height(&self) -> Result<u64> {
        self.call("getBlockHeight", json!([{ "commitment": "confirmed" }]))
            .await
    }

    /// `None` while the node has not indexed the transaction yet.
    pub async fn settlement(
        &self,
        sig: &str,
        owner: &str,
        mint: &str,
    ) -> Result<Option<Settlement>> {
        let r: Value = self
            .call(
                "getTransaction",
                json!([sig, { "encoding": "jsonParsed", "maxSupportedTransactionVersion": 0, "commitment": "confirmed" }]),
            )
            .await?;
        if r.is_null() {
            return Ok(None);
        }
        let meta = &r["meta"];
        let sum = |key: &str| -> f64 {
            meta[key]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|b| b["owner"] == owner && b["mint"] == mint)
                .filter_map(|b| b["uiTokenAmount"]["uiAmount"].as_f64())
                .fold(0.0, |acc, v| acc + v)
        };
        Ok(Some(Settlement {
            token_delta: sum("postTokenBalances") - sum("preTokenBalances"),
            fee_sol: meta["fee"].as_u64().unwrap_or(0) as f64 / LAMPORTS_PER_SOL,
        }))
    }
}
