//! `spreadradar mcp` — the market, served to an MCP client over stdio.
//!
//! No window: the same market and scanner the app runs (`spread_core::run`,
//! `scanner::run`), started here for as long as the client keeps the server
//! open, and read from tool calls instead of painted. The watchlist is the
//! app's own `watchlist.json`; an asset added or removed here is followed or
//! dropped by an open app within a couple of seconds, and the other way round
//! (`Market::reload_if_changed`). The free version's limit applies here too.
//!
//! Trading is not here. A swap spends from the app's hot wallet, and the app
//! is where its dry run, its confirmation and its limits are in front of the
//! person who owns that wallet.

use std::time::{Duration, Instant};

use serde_json::{json, Value};
use spread_core::jupiter::JupiterApi;
use spread_core::market::normalize_symbol;
use spread_core::model::{ConnState, EventKind};
use spread_core::scanner::{ScanSnapshot, NEW_WITHIN_DAYS};
use spread_core::trade::TradeConfig;
use spread_core::{EngineConfig, MarketCmd, MarketSnapshot};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, watch};

use crate::licence::{self, StatusExt};

/// The newest protocol revision this server speaks. A client asking for an
/// older one gets its own back; everything used here exists in all of them.
const PROTOCOL: &str = "2025-06-18";

/// How long a call waits for quotes to arrive after the server starts, or
/// after an asset is selected.
const WARM_UP: Duration = Duration::from_secs(8);
/// Adding an asset looks it up on Jupiter, which shares a rate budget.
const ADD_WAIT: Duration = Duration::from_secs(30);
/// The first scan of every exchange's listings takes a while.
const SCAN_WAIT: Duration = Duration::from_secs(90);

const INSTRUCTIONS: &str = "SpreadRadar follows crypto assets on Binance, Bybit, OKX, MEXC, Gate and \
Jupiter (Solana) at once, all quoted in USDT, and scans for tokens Gate or MEXC listed recently that \
Binance spot doesn't list. watchlist gives every followed asset's price, best cross-venue edge and \
how many venues quote it. asset gives one asset in detail: each venue's bid and ask, every buy-here \
sell-there route in basis points before and after taker fees, and which venue's price lags the \
others; pass watch_secs to observe for longer, since lead/lag needs price moves to measure. \
add_asset and remove_asset change the watchlist the SpreadRadar app shows too: ask before removing \
one. new_listings gives the scanner's tokens, scored. The free version follows 3 assets. Prices \
come from public market data. An edge on screen is not a profit: transfers, withdrawal fees, \
slippage and the price moving all eat into it. Say so, and never present this as financial advice. \
Trading is not available here; it stays in the app.";

/// The running market and scanner, and the way to steer the market.
struct Live {
    market: watch::Receiver<MarketSnapshot>,
    scan: watch::Receiver<ScanSnapshot>,
    cmds: mpsc::Sender<MarketCmd>,
}

pub fn serve() {
    // The log goes to stderr: stdout is the protocol.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("spreadradar mcp: could not start: {e}");
            std::process::exit(1);
        }
    };
    runtime.block_on(async {
        let (trade_cfg, _notices) = TradeConfig::load();
        let jupiter = JupiterApi::new(trade_cfg.jupiter_api_key, trade_cfg.jupiter_rpm);
        let cfg = EngineConfig {
            poll_every: jupiter.feed_poll_every(),
            ..EngineConfig::default()
        };
        let (market_tx, market) = watch::channel(MarketSnapshot::default());
        let (scan_tx, scan) = watch::channel(ScanSnapshot::default());
        let (cmds, cmd_rx) = mpsc::channel::<MarketCmd>(16);
        tokio::spawn(spread_core::run(cfg, jupiter, cmd_rx, market_tx));
        tokio::spawn(spread_core::scanner::run(scan_tx));
        let _ = cmds
            .send(MarketCmd::SetLimit(licence::current().asset_limit()))
            .await;
        let live = Live { market, scan, cmds };

        let mut lines = BufReader::new(tokio::io::stdin()).lines();
        let mut stdout = tokio::io::stdout();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                write(&mut stdout, &error(Value::Null, -32700, "not JSON")).await;
                continue;
            };
            // A notification (no id) asks for nothing back.
            let Some(id) = message.get("id").cloned() else {
                continue;
            };
            let reply = handle(&live, id, &message).await;
            write(&mut stdout, &reply).await;
        }
    });
    // The feeds' sockets don't need a graceful goodbye.
    runtime.shutdown_background();
}

async fn handle(live: &Live, id: Value, message: &Value) -> Value {
    let method = message["method"].as_str().unwrap_or("");
    let params = &message["params"];
    match method {
        "initialize" => {
            let asked = params["protocolVersion"].as_str().unwrap_or(PROTOCOL);
            let version = if asked <= PROTOCOL { asked } else { PROTOCOL };
            result(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "spreadradar", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS,
                }),
            )
        }
        "ping" => result(id, json!({})),
        "tools/list" => result(id, json!({ "tools": tools() })),
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or("");
            let (text, is_error) = match call(live, name, &params["arguments"]).await {
                Ok(value) => (pretty(&value), false),
                Err(why) => (why, true),
            };
            result(
                id,
                json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }),
            )
        }
        _ => error(id, -32601, &format!("no method {method}")),
    }
}

async fn write(stdout: &mut tokio::io::Stdout, reply: &Value) {
    // One message per line, and nothing else ever on stdout: the client reads
    // it as the protocol, so a stray print is a corrupt message.
    let mut bytes = serde_json::to_vec(reply).unwrap_or_default();
    bytes.push(b'\n');
    let _ = stdout.write_all(&bytes).await;
    let _ = stdout.flush().await;
}

fn result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

fn tools() -> Value {
    let symbol = json!({ "type": "string", "description": "Ticker, e.g. SOL or $WIF." });
    json!([
        {
            "name": "watchlist",
            "description": "Every followed asset: median price across venues, best gross cross-venue \
                            edge right now in bps, how many venues quote it fresh, its Solana token \
                            on Jupiter and where Gate stands on listing it. Also the assets locked by \
                            the free version's limit.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "asset",
            "description": "One followed asset in detail: each venue's bid, ask, spread, deviation \
                            from the median and lag; every buy-on-one, sell-on-another route in bps, \
                            gross and net of taker fees, with the best seen; which venue follows \
                            which and by how many ms; and recent events.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "symbol": symbol,
                    "watch_secs": {
                        "type": "integer", "minimum": 0, "maximum": 120,
                        "description": "Watch this long before answering, to collect price moves for lead/lag and the best edges. Default 0.",
                    },
                },
                "required": ["symbol"],
            },
        },
        {
            "name": "add_asset",
            "description": "Adds an asset to the watchlist, looking up its Solana token on Jupiter. \
                            The free version follows 3 assets.",
            "inputSchema": { "type": "object", "properties": { "symbol": symbol }, "required": ["symbol"] },
        },
        {
            "name": "remove_asset",
            "description": "Removes an asset from the watchlist, in the app too. Ask the user first.",
            "inputSchema": { "type": "object", "properties": { "symbol": symbol }, "required": ["symbol"] },
        },
        {
            "name": "new_listings",
            "description": format!(
                "Tokens Gate or MEXC listed against USDT in the last {NEW_WITHIN_DAYS} days, or have \
                 scheduled, that Binance spot doesn't list: where each trades, its volume, spread and \
                 24 h change, the best edge between exchanges, market cap and holders, whether Binance \
                 already follows it through futures or Alpha, and the signals behind its score. Best \
                 score first. The first scan after start takes up to a minute or two."
            ),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100, "description": "Default 20." },
                    "min_score": { "type": "integer", "description": "Only tokens scored at least this." },
                    "fresh_only": { "type": "boolean", "description": "Only tokens that appeared since this server's first scan." },
                },
            },
        },
    ])
}

async fn call(live: &Live, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "watchlist" => Ok(watchlist(live).await),
        "asset" => asset(live, args).await,
        "add_asset" => add(live, args).await,
        "remove_asset" => remove(live, args).await,
        "new_listings" => new_listings(live, args).await,
        _ => Err(format!("no tool {name}")),
    }
}

fn symbol(args: &Value) -> Result<String, String> {
    normalize_symbol(args["symbol"].as_str().unwrap_or(""))
}

/// The latest market snapshot once `ready` holds, or the latest anyway after
/// `within`.
async fn market_when(
    live: &Live,
    within: Duration,
    ready: impl Fn(&MarketSnapshot) -> bool,
) -> MarketSnapshot {
    let until = Instant::now() + within;
    let mut rx = live.market.clone();
    loop {
        let m = rx.borrow_and_update().clone();
        if ready(&m) || Instant::now() >= until {
            return m;
        }
        let _ = tokio::time::timeout(until - Instant::now(), rx.changed()).await;
    }
}

/// At least half the venues that list an asset have quoted it: enough for a
/// median and a route. One venue alone (often Jupiter, polled first) is not.
fn warm(fresh: usize, listed: usize) -> bool {
    fresh >= 2 && fresh * 2 >= listed
}

fn round(v: f64, places: i32) -> f64 {
    let f = 10f64.powi(places);
    (v * f).round() / f
}

fn opt(v: Option<f64>, places: i32) -> Value {
    v.map_or(Value::Null, |v| json!(round(v, places)))
}

async fn watchlist(live: &Live) -> Value {
    // Right after the server starts, give every venue a moment to connect.
    let m = market_when(live, WARM_UP, |m| {
        !m.assets.is_empty()
            && m.assets
                .iter()
                .all(|a| warm(a.fresh, a.listed) && a.mid.is_some())
    })
    .await;
    let assets: Vec<Value> = m
        .assets
        .iter()
        .map(|a| {
            json!({
                "symbol": a.symbol,
                "price_usdt": a.mid,
                "best_gross_bps": opt(a.best_gross_bps, 2),
                "venues_fresh": a.fresh,
                "venues_listing": a.listed,
                "jupiter_token": a.jupiter.as_ref().map(|t| json!({ "name": t.name, "mint": t.mint })),
                "gate": a.gate.map(|g| g.describe()),
            })
        })
        .collect();
    let mut v = json!({ "assets": assets, "locked": m.locked });
    if let Some(n) = m.limit {
        v["limit"] = json!(format!(
            "The free version follows {n} assets; locked ones stay in the list unfollowed. \
             SpreadRadar Pro follows any number: {}",
            licence::buy_url()
        ));
    }
    v
}

async fn asset(live: &Live, args: &Value) -> Result<Value, String> {
    let symbol = symbol(args)?;
    let now = live.market.borrow().clone();
    if now.locked.contains(&symbol) {
        return Err(format!(
            "{symbol} is locked: the free version follows {} assets. Remove another to follow it, or get SpreadRadar Pro.",
            now.limit.unwrap_or_default()
        ));
    }
    if !now.assets.is_empty() && !now.assets.iter().any(|a| a.symbol == symbol) {
        return Err(format!(
            "{symbol} is not in the watchlist: add it with add_asset"
        ));
    }
    live.cmds
        .send(MarketCmd::Select(symbol.clone()))
        .await
        .map_err(|_| "the market stopped".to_string())?;
    let selected = |m: &MarketSnapshot| m.selected.as_deref() == Some(symbol.as_str());
    let m = market_when(live, WARM_UP, |m| {
        let listing = m.detail.venues.iter().filter(|v| !v.unlisted);
        let fresh = listing.clone().filter(|v| !v.stale).count();
        selected(m) && warm(fresh, listing.count())
    })
    .await;
    if !selected(&m) {
        return Err(format!(
            "{symbol} is not in the watchlist: add it with add_asset"
        ));
    }
    let watch_secs = args["watch_secs"].as_u64().unwrap_or(0).min(120);
    let m = if watch_secs > 0 {
        tokio::time::sleep(Duration::from_secs(watch_secs)).await;
        live.market.borrow().clone()
    } else {
        m
    };
    let d = &m.detail;
    let venues: Vec<Value> = d
        .venues
        .iter()
        .map(|v| {
            json!({
                "venue": v.venue.name(),
                "connection": match v.conn {
                    ConnState::Connecting => "connecting",
                    ConnState::Connected => "connected",
                    ConnState::Disconnected => "disconnected",
                },
                "bid": v.bid,
                "ask": v.ask,
                "spread_bps": opt(v.spread_bps, 2),
                "vs_median_bps": opt(v.dev_bps, 2),
                "lag_ms": opt(v.lag_ms, 0),
                "network_ms": opt(v.net_latency_ms, 0),
                "taker_fee_bps": v.fee_bps,
                "quotes_per_sec": v.rate,
                "stale": v.stale,
                "not_listed": v.unlisted,
            })
        })
        .collect();
    let routes: Vec<Value> = d
        .routes
        .iter()
        .take(10)
        .map(|r| {
            json!({
                "buy_on": r.buy.name(),
                "sell_on": r.sell.name(),
                "gross_bps": round(r.gross_bps, 2),
                "net_bps": round(r.net_bps, 2),
                "best_gross_bps_seen": round(r.best_gross_bps, 2),
            })
        })
        .collect();
    let lag: Vec<Value> = d
        .lag_pairs
        .iter()
        .map(|p| {
            json!({
                "leader": p.leader.name(),
                "follower": p.follower.name(),
                "median_delay_ms": round(p.median_ms, 0),
                "moves_matched": p.samples,
                "reliable": p.reliable,
            })
        })
        .collect();
    let events: Vec<Value> = d
        .events
        .iter()
        .take(20)
        .map(|e| {
            let kind = match e.kind {
                EventKind::Opportunity => "edge",
                EventKind::Stale => "stale",
                EventKind::Connection => "connection",
                EventKind::Listing => "listing",
            };
            json!({ "at": e.at, "kind": kind, "text": e.text })
        })
        .collect();
    let summary = m.assets.iter().find(|a| a.symbol == symbol);
    Ok(json!({
        "symbol": symbol,
        "price_usdt": summary.and_then(|a| a.mid),
        "jupiter_token": summary.and_then(|a| a.jupiter.as_ref()).map(|t| json!({ "name": t.name, "mint": t.mint })),
        "gate": summary.and_then(|a| a.gate).map(|g| g.describe()),
        "venues": venues,
        "routes": routes,
        "lead_lag": lag,
        "moves_measured": d.lag_samples,
        "events": events,
    }))
}

async fn add(live: &Live, args: &Value) -> Result<Value, String> {
    let symbol = symbol(args)?;
    let before = live.market.borrow().notice.clone();
    live.cmds
        .send(MarketCmd::Add(symbol.clone()))
        .await
        .map_err(|_| "the market stopped".to_string())?;
    // Done when the asset is followed, or the market said why not.
    let m = market_when(live, ADD_WAIT, |m| {
        m.adding.is_none()
            && (m.assets.iter().any(|a| a.symbol == symbol)
                || (m.notice.is_some() && m.notice != before))
    })
    .await;
    let followed = m.assets.iter().find(|a| a.symbol == symbol);
    match (followed, m.notice) {
        (Some(a), notice) => Ok(json!({
            "added": symbol,
            "jupiter_token": a.jupiter.as_ref().map(|t| t.name.clone()),
            "note": notice,
        })),
        (None, Some(notice)) => Err(notice),
        (None, None) if m.adding.is_some() => Err(format!(
            "{symbol} is still being looked up on Jupiter; check the watchlist in a moment"
        )),
        (None, None) => Err(format!("{symbol} was not added")),
    }
}

async fn remove(live: &Live, args: &Value) -> Result<Value, String> {
    let symbol = symbol(args)?;
    let now = live.market.borrow().clone();
    if !now.assets.iter().any(|a| a.symbol == symbol) && !now.locked.contains(&symbol) {
        return Err(format!("{symbol} is not in the watchlist"));
    }
    live.cmds
        .send(MarketCmd::Remove(symbol.clone()))
        .await
        .map_err(|_| "the market stopped".to_string())?;
    let m = market_when(live, Duration::from_secs(3), |m| {
        !m.assets.iter().any(|a| a.symbol == symbol) && !m.locked.contains(&symbol)
    })
    .await;
    let left: Vec<&str> = m.assets.iter().map(|a| a.symbol.as_str()).collect();
    Ok(json!({ "removed": symbol, "watchlist": left, "locked": m.locked }))
}

async fn new_listings(live: &Live, args: &Value) -> Result<Value, String> {
    let until = Instant::now() + SCAN_WAIT;
    let mut rx = live.scan.clone();
    let s = loop {
        let s = rx.borrow_and_update().clone();
        if s.last_scan.is_some() || Instant::now() >= until {
            break s;
        }
        let _ = tokio::time::timeout(until - Instant::now(), rx.changed()).await;
    };
    if s.last_scan.is_none() {
        return Err(format!(
            "The first scan hasn't finished yet{}. Try again in a minute.",
            s.progress.map(|p| format!(" ({p})")).unwrap_or_default()
        ));
    }
    let limit = args["limit"].as_u64().unwrap_or(20).clamp(1, 100) as usize;
    let min_score = args["min_score"].as_i64();
    let fresh_only = args["fresh_only"].as_bool().unwrap_or(false);
    let rows: Vec<Value> = s
        .rows
        .iter()
        .filter(|r| min_score.is_none_or(|m| i64::from(r.score) >= m))
        .filter(|r| !fresh_only || r.fresh)
        .take(limit)
        .map(|r| {
            let venues: Vec<Value> = r
                .venues
                .iter()
                .map(|q| {
                    json!({
                        "exchange": q.exchange.name(),
                        "volume_usd": opt(q.volume_usd, 0),
                        "spread_bps": opt(q.spread_bps, 1),
                    })
                })
                .collect();
            json!({
                "symbol": r.symbol,
                "name": r.name,
                "listed": r.listed_date,
                "age": r.age,
                "trades_on": venues,
                "volume_usd": round(r.volume_usd, 0),
                "change_24h_pct": opt(r.change_pct, 2),
                "spread_bps": opt(r.spread_bps, 1),
                "best_edge": r.edge.as_ref().map(|e| json!({
                    "route": e.route(),
                    "gross_bps": round(e.gross_bps, 1),
                    "net_bps": round(e.net_bps, 1),
                    "open": e.open(),
                    "suspect": e.suspect(),
                    "blocked": e.blocked,
                })),
                "market_cap_usd": opt(r.market_cap, 0),
                "holders": r.holders,
                "binance_futures": r.binance_futures,
                "binance_alpha": r.binance_alpha,
                "signals": r.signals.iter().map(|g| json!({ "text": g.text, "good": g.good })).collect::<Vec<_>>(),
                "score": r.score,
                "new_since_start": r.fresh,
            })
        })
        .collect();
    let mut v = json!({ "last_scan": s.last_scan, "tokens": s.rows.len(), "shown": rows });
    if !s.errors.is_empty() {
        v["sources_failed"] = json!(s.errors);
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_tool_has_a_unique_name_and_an_object_schema() {
        let tools = tools();
        let tools = tools.as_array().unwrap();
        let names: HashSet<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names.len(), tools.len());
        for t in tools {
            assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
            for required in t["inputSchema"]["required"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let key = required.as_str().unwrap();
                assert!(
                    t["inputSchema"]["properties"].get(key).is_some(),
                    "{} requires {key} but does not describe it",
                    t["name"]
                );
            }
        }
    }

    #[test]
    fn one_venue_quoting_is_not_a_warm_market() {
        assert!(!warm(1, 6));
        assert!(!warm(2, 6));
        assert!(warm(3, 6));
        assert!(warm(2, 2));
        assert!(!warm(1, 1), "a median needs two");
    }

    #[test]
    fn numbers_are_rounded_for_reading() {
        assert_eq!(round(1.23456, 2), 1.23);
        assert_eq!(opt(Some(-0.456), 1), json!(-0.5));
        assert_eq!(opt(None, 1), Value::Null);
    }
}
