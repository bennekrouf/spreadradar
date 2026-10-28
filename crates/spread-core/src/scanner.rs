//! New-listing scanner, independent of the watchlist.
//!
//! Finds tokens Gate or MEXC list against USDT in the last `NEW_WITHIN_DAYS`
//! (or has scheduled), that Binance spot doesn't list, and scores how serious
//! they look: volume, how many other exchanges list them, market cap, and
//! whether Binance already follows them through futures or Binance Alpha.
//!
//! Everything comes from public bulk endpoints, fetched every `SCAN_EVERY`.
//! Gate publishes each pair's opening time; MEXC doesn't, so its first daily
//! candle stands in, fetched once per token and cached on disk.
//! Tokens are matched across exchanges by ticker, so two different tokens
//! sharing a ticker look like one.
//!
//! Each row also carries its best cross-exchange edge: buy at the ask on one
//! exchange, sell at the bid on another, from the same tickers.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use futures_util::StreamExt;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use tokio::sync::watch;

use crate::model::{Event, EventKind, Venue};
use crate::EngineConfig;
mod assess;
mod sources;
use assess::*;

/// Listings trickle in a few a day and volumes move slowly: every five
/// minutes is timely and keeps the ~8 MB of bulk downloads per scan light.
pub const SCAN_EVERY: Duration = Duration::from_secs(300);
/// Ages move with the clock: rows are rebuilt from the last scan this often.
const REFRESH_EVERY: Duration = Duration::from_secs(60);
pub const NEW_WITHIN_DAYS: i64 = 30;
/// Daily candles asked from MEXC: fewer back means the token is that young.
const MEXC_DATE_DAYS: usize = 60;
/// Concurrent MEXC candle requests, well under its public limit.
const MEXC_DATE_CONCURRENCY: usize = 5;
const MAX_EVENTS: usize = 200;
/// Bid/ask spread on the main venue at or under this is a good sign…
pub const TIGHT_SPREAD_BPS: f64 = 20.0;
/// …and at or over this, a warning: 1 % lost on every round trip.
pub const WIDE_SPREAD_BPS: f64 = 100.0;
/// An edge at or over this after fees is worth a look and goes in the log.
pub const EDGE_NET_BPS: f64 = 30.0;
/// A gap this wide is nearly always two tokens sharing a ticker, or a dead book.
pub const SUSPECT_EDGE_BPS: f64 = 2_000.0;
/// 24 h volume an exchange needs to count as one leg of an edge: under it the
/// best bid or ask is often an order nobody has touched in hours.
const EDGE_MIN_VOLUME_USD: f64 = 10_000.0;
const DAY: i64 = 86_400;

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Exchange {
    Gate,
    Mexc,
    Bybit,
    Okx,
}

/// Buy at the ask on one exchange, sell at the bid on another.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub buy: Exchange,
    pub sell: Exchange,
    /// In bps of the mid between the two exchanges.
    pub gross_bps: f64,
    /// Gross minus the taker fee on both legs.
    pub net_bps: f64,
    /// Why the coins can't move from `buy` to `sell`. Only Gate publishes
    /// deposit and withdrawal status, so `None` elsewhere means unknown.
    pub blocked: Option<String>,
}

impl Edge {
    /// Too wide to be the same token on a live book.
    pub fn suspect(&self) -> bool {
        self.gross_bps >= SUSPECT_EDGE_BPS
    }

    /// Clears fees by `EDGE_NET_BPS`, looks real and isn't known blocked.
    pub fn open(&self) -> bool {
        self.net_bps >= EDGE_NET_BPS && !self.suspect() && self.blocked.is_none()
    }

    /// "MEXC → Gate".
    pub fn route(&self) -> String {
        format!("{} → {}", self.buy.name(), self.sell.name())
    }
}

/// One exchange's USDT market for a token.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VenueQuote {
    pub exchange: Exchange,
    pub volume_usd: Option<f64>,
    pub spread_bps: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Signal {
    pub text: String,
    /// A reason to take the token seriously, or a warning.
    pub good: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewListing {
    pub symbol: String,
    pub name: String,
    /// Unix seconds of the earliest known USDT listing on Gate or MEXC; in
    /// the future when Gate has scheduled it.
    pub listed_at: i64,
    /// "3 d ago", "in 5 h".
    pub age: String,
    /// Local date and time, e.g. "24 Sep 14:30". MEXC dates are the UTC day.
    pub listed_date: String,
    /// Where it trades against USDT.
    pub venues: Vec<VenueQuote>,
    pub volume_usd: f64,
    /// 24 h price change on its most traded venue, in %.
    pub change_pct: Option<f64>,
    /// Bid/ask spread on its most traded venue, in bps of the mid.
    pub spread_bps: Option<f64>,
    /// Best edge between two exchanges with enough volume; often negative.
    pub edge: Option<Edge>,
    pub market_cap: Option<f64>,
    pub holders: Option<u64>,
    pub binance_futures: bool,
    pub binance_alpha: bool,
    pub signals: Vec<Signal>,
    /// Good signals minus warnings, for sorting.
    pub score: i32,
    /// Showed up after this session's first scan.
    pub fresh: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScanSnapshot {
    /// Best score first.
    pub rows: Vec<NewListing>,
    /// Local time of the last finished scan.
    pub last_scan: Option<String>,
    /// What the scanner is doing right now, if anything.
    pub progress: Option<String>,
    /// Sources that failed on the last scan; their previous data is kept.
    pub errors: Vec<String>,
    /// New tokens and Binance moves, newest first.
    pub events: Vec<Event>,
}

// ---------------------------------------------------------------------------
// Exchange responses, only the fields used
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct BinancePrice {
    symbol: String,
}

#[derive(Deserialize)]
struct AlphaList {
    data: Vec<AlphaToken>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AlphaToken {
    symbol: String,
    name: String,
    market_cap: Option<String>,
    holders: Option<String>,
}

#[derive(Deserialize, Clone)]
struct GatePair {
    base: String,
    base_name: String,
    quote: String,
    /// Unix seconds trading opens; in the future when scheduled.
    buy_start: i64,
}

#[derive(Deserialize)]
struct GateTicker {
    currency_pair: String,
    quote_volume: Option<String>,
    change_percentage: Option<String>,
    highest_bid: Option<String>,
    lowest_ask: Option<String>,
}

#[derive(Deserialize, Clone)]
struct GateCurrency {
    currency: String,
    market_cap: Option<String>,
    /// "stocks", "metals", "indices"…: not crypto.
    #[serde(default)]
    category: Vec<String>,
    #[serde(default)]
    deposit_disabled: bool,
    #[serde(default)]
    withdraw_disabled: bool,
}

#[derive(Deserialize)]
struct MexcInfo {
    symbols: Vec<MexcSymbol>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct MexcSymbol {
    symbol: String,
    /// "1" online.
    status: String,
    base_asset: String,
    quote_asset: String,
    full_name: Option<String>,
    #[serde(default)]
    is_spot_trading_allowed: bool,
    #[serde(default)]
    concept_plates: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MexcTicker {
    symbol: String,
    quote_volume: Option<String>,
    /// A fraction: 0.05 is +5 %.
    price_change_percent: Option<String>,
    bid_price: Option<String>,
    ask_price: Option<String>,
}

#[derive(Deserialize)]
struct BybitResp {
    result: BybitList,
}

#[derive(Deserialize)]
struct BybitList {
    list: Vec<BybitTicker>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BybitTicker {
    symbol: String,
    turnover24h: Option<String>,
    /// A fraction, like MEXC's.
    price24h_pcnt: Option<String>,
    bid1_price: Option<String>,
    ask1_price: Option<String>,
}

#[derive(Deserialize)]
struct OkxResp {
    data: Vec<OkxTicker>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OkxTicker {
    inst_id: String,
    vol_ccy24h: Option<String>,
    last: Option<String>,
    open24h: Option<String>,
    bid_px: Option<String>,
    ask_px: Option<String>,
}

fn num(s: &Option<String>) -> Option<f64> {
    s.as_deref()
        .and_then(|s| s.parse().ok())
        .filter(|v: &f64| v.is_finite())
}

// ---------------------------------------------------------------------------
// Sources
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Default)]
struct Ticker {
    volume_usd: Option<f64>,
    change_pct: Option<f64>,
    bid: Option<f64>,
    ask: Option<f64>,
}

impl Ticker {
    fn spread_bps(&self) -> Option<f64> {
        let (bid, ask) = (self.bid?, self.ask?);
        (bid > 0.0 && ask >= bid).then(|| (ask - bid) / ((ask + bid) / 2.0) * 1e4)
    }

    /// Both sides quoted and not crossed.
    fn mid(&self) -> Option<f64> {
        self.spread_bps()?;
        Some((self.bid? + self.ask?) / 2.0)
    }
}

/// Latest data from every source. A source that fails keeps its previous
/// data, so one slow exchange doesn't empty the table.
#[derive(Default)]
struct Sources {
    binance_spot: HashSet<String>,
    binance_futures: HashSet<String>,
    alpha: HashMap<String, AlphaToken>,
    gate_pairs: Vec<GatePair>,
    gate_tickers: HashMap<String, Ticker>,
    gate_currencies: HashMap<String, GateCurrency>,
    mexc_symbols: Vec<MexcSymbol>,
    mexc_tickers: HashMap<String, Ticker>,
    bybit: HashMap<String, Ticker>,
    okx: HashMap<String, Ticker>,
}

/// "PEPEUSDT" → "PEPE" for the stablecoin quotes every Binance listing has.
fn binance_base(symbol: &str) -> Option<&str> {
    ["FDUSD", "USDT", "USDC"]
        .iter()
        .find_map(|q| symbol.strip_suffix(q))
        .filter(|b| !b.is_empty())
}

/// Futures quote memecoins in thousands: "1000PEPE", "1MBABYDOGE".
fn futures_base(symbol: &str) -> Option<&str> {
    let base = binance_base(symbol)?;
    Some(
        ["1000000", "10000", "1000", "1M"]
            .iter()
            .find_map(|p| base.strip_prefix(p))
            .unwrap_or(base),
    )
}

async fn get<T: DeserializeOwned>(http: &reqwest::Client, url: &str) -> Result<T> {
    Ok(http
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

/// Leveraged tokens: "SUI5S" named "SUI5xShort", "BTC3L".
fn is_leveraged(base: &str, name: &str) -> bool {
    let mut rev = base.chars().rev();
    let shape =
        matches!(rev.next(), Some('L' | 'S')) && rev.next().is_some_and(|c| c.is_ascii_digit());
    let name = name.to_ascii_lowercase();
    shape && (name.contains("long") || name.contains("short") || name.is_empty())
}

fn gate_excluded(p: &GatePair, currencies: &HashMap<String, GateCurrency>) -> bool {
    is_leveraged(&p.base, &p.base_name)
        || currencies
            .get(&p.base)
            .is_some_and(|c| !c.category.is_empty())
}

fn mexc_excluded(s: &MexcSymbol) -> bool {
    let stock = s
        .concept_plates
        .as_ref()
        .is_some_and(|c| c.iter().any(|p| p == "Tokenized Stocks"));
    stock || is_leveraged(&s.base_asset, s.full_name.as_deref().unwrap_or(""))
}

// ---------------------------------------------------------------------------
// Rows
// ---------------------------------------------------------------------------

/// MEXC listing day per symbol: `Some(unix secs)` for its first daily
/// candle, `None` when it is older than `MEXC_DATE_DAYS`.
type MexcDates = HashMap<String, Option<i64>>;

/// "12 bps (0.12 %)", "0.3 bps (0.003 %)": one-tick spreads are well under 1 bp.
pub fn fmt_spread(bps: f64) -> String {
    format!("{} ({})", fmt_bps(bps), fmt_pct(bps))
}

/// Whole bps from 10 up, one decimal below.
pub fn fmt_bps(bps: f64) -> String {
    if bps < 10.0 {
        format!("{bps:.1} bps")
    } else {
        format!("{bps:.0} bps")
    }
}

/// "+48 bps", "-27 bps": an edge, which is negative most of the time.
pub fn fmt_edge(bps: f64) -> String {
    let sign = if bps < 0.0 { "-" } else { "+" };
    format!("{sign}{}", fmt_bps(bps.abs()))
}

pub fn fmt_pct(bps: f64) -> String {
    if bps < 10.0 {
        format!("{:.3} %", bps / 100.0)
    } else {
        format!("{:.2} %", bps / 100.0)
    }
}

/// "$1.2M", "$340k", "$12.3B".
pub fn fmt_usd(v: f64) -> String {
    match v.abs() {
        a if a >= 1e9 => format!("${:.1}B", v / 1e9),
        a if a >= 1e6 => format!("${:.1}M", v / 1e6),
        a if a >= 1e3 => format!("${:.0}k", v / 1e3),
        _ => format!("${v:.0}"),
    }
}

/// "3 d ago", "5 h ago", "in 2 h".
fn age(listed_at: i64, now: i64) -> String {
    let d = now - listed_at;
    let span = |s: i64| match s {
        s if s >= 2 * DAY => format!("{} d", s / DAY),
        s if s >= 3600 => format!("{} h", s / 3600),
        s => format!("{} min", (s / 60).max(1)),
    };
    if d >= 0 {
        format!("{} ago", span(d))
    } else {
        format!("in {}", span(-d))
    }
}

// ---------------------------------------------------------------------------
// MEXC listing dates
// ---------------------------------------------------------------------------

fn dates_path() -> PathBuf {
    crate::trade::config_dir().join("mexc_listing_dates.json")
}

fn load_dates() -> MexcDates {
    std::fs::read_to_string(dates_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_dates(dates: &MexcDates) {
    let saved = std::fs::create_dir_all(crate::trade::config_dir()).and_then(|()| {
        std::fs::write(
            dates_path(),
            serde_json::to_string(dates).unwrap_or_default(),
        )
    });
    if let Err(e) = saved {
        tracing::warn!("scanner: could not save MEXC listing dates: {e}");
    }
}

/// `Some(Some(day))` for a young token, `Some(None)` when it is older than
/// the candles asked for, `None` when MEXC has no candle yet or failed.
async fn mexc_listing_day(http: &reqwest::Client, symbol: &str) -> Option<Option<i64>> {
    let url = format!(
        "https://api.mexc.com/api/v3/klines?symbol={symbol}&interval=1d&limit={MEXC_DATE_DAYS}"
    );
    let candles: Vec<Vec<serde_json::Value>> = get(http, &url).await.ok()?;
    let first_ms = candles.first()?.first()?.as_i64()?;
    Some((candles.len() < MEXC_DATE_DAYS).then_some(first_ms / 1000))
}

// ---------------------------------------------------------------------------
// Task
// ---------------------------------------------------------------------------

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

struct Scanner {
    http: reqwest::Client,
    src: Sources,
    dates: MexcDates,
    snap: ScanSnapshot,
    events: VecDeque<Event>,
    /// Symbols of every row seen this session; `None` before the first scan.
    seen: Option<HashSet<String>>,
    /// Last rows, to notice Binance picking a token up.
    prev: HashMap<String, NewListing>,
    out: watch::Sender<ScanSnapshot>,
}

pub async fn run(out: watch::Sender<ScanSnapshot>) {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("spreadradar")
        .build()
        // Settings the builder refuses still leave the plain client.
        .unwrap_or_else(|_| reqwest::Client::new());
    let mut s = Scanner {
        http,
        src: Sources::default(),
        dates: load_dates(),
        snap: ScanSnapshot::default(),
        events: VecDeque::new(),
        seen: None,
        prev: HashMap::new(),
        out,
    };
    let mut scan = tokio::time::interval(SCAN_EVERY);
    scan.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut refresh = tokio::time::interval(REFRESH_EVERY);
    refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = scan.tick() => s.scan().await,
            _ = refresh.tick() => s.rebuild(false),
        }
        if s.out.is_closed() {
            return;
        }
    }
}

impl Scanner {
    async fn scan(&mut self) {
        self.progress(Some("Scanning Gate, MEXC, Binance, Bybit and OKX…".into()));
        self.snap.errors = self.src.update(&self.http).await;
        if self.src.binance_spot.is_empty() {
            // Without it every token would look unlisted on Binance.
            self.progress(Some(
                "Waiting for Binance's symbol list; retrying next scan".into(),
            ));
            return;
        }
        self.date_mexc().await;
        self.snap.last_scan = Some(chrono::Local::now().format("%H:%M:%S").to_string());
        self.snap.progress = None;
        self.rebuild(true);
    }

    /// Dates the MEXC candidates not dated yet, a few requests at a time.
    async fn date_mexc(&mut self) {
        let todo: Vec<String> = self
            .src
            .mexc_candidates()
            .filter(|s| !self.dates.contains_key(&s.symbol))
            .map(|s| s.symbol.clone())
            .collect();
        if todo.is_empty() {
            return;
        }
        let total = todo.len();
        let http = self.http.clone();
        let mut results = futures_util::stream::iter(todo)
            .map(|symbol| {
                let http = http.clone();
                async move {
                    let day = mexc_listing_day(&http, &symbol).await;
                    (symbol, day)
                }
            })
            .buffer_unordered(MEXC_DATE_CONCURRENCY);
        let mut done = 0;
        while let Some((symbol, day)) = results.next().await {
            done += 1;
            if let Some(day) = day {
                self.dates.insert(symbol, day);
            }
            if done % 25 == 0 {
                self.progress(Some(format!("Finding MEXC listing dates: {done}/{total}")));
            }
        }
        save_dates(&self.dates);
    }

    fn progress(&mut self, text: Option<String>) {
        self.snap.progress = text;
        self.out.send_replace(self.snap.clone());
    }

    /// Rebuilds rows from the last data. After a scan, also logs tokens that
    /// are new this session and tokens Binance picked up.
    fn rebuild(&mut self, after_scan: bool) {
        if self.src.binance_spot.is_empty() {
            return;
        }
        let now = unix_now();
        let mut rows = build(&self.src, &self.dates, now);
        if after_scan {
            self.log_changes(&rows);
        }
        let seen = self.seen.as_ref();
        for r in &mut rows {
            r.fresh = seen.is_some_and(|s| !s.contains(&r.symbol))
                || self.prev.get(&r.symbol).is_some_and(|p| p.fresh);
        }
        self.prev = rows.iter().map(|r| (r.symbol.clone(), r.clone())).collect();
        if after_scan {
            self.seen
                .get_or_insert_with(HashSet::new)
                .extend(rows.iter().map(|r| r.symbol.clone()));
        }
        self.snap.rows = rows;
        self.snap.events = self.events.iter().cloned().collect();
        self.out.send_replace(self.snap.clone());
    }

    /// Logs edges that open, move to another route or close since the
    /// last scan.
    fn log_edges(&mut self, rows: &[NewListing]) {
        let mut edge_log = |text: String| {
            tracing::info!("scanner: {text}");
            self.events
                .push_front(Event::now(EventKind::Opportunity, text));
        };
        for r in rows {
            let was = self
                .prev
                .get(&r.symbol)
                .and_then(|p| p.edge.as_ref())
                .filter(|e| e.open());
            match (r.edge.as_ref().filter(|e| e.open()), was) {
                (Some(e), _) => {
                    if was.is_none_or(|w| w.buy != e.buy || w.sell != e.sell) {
                        edge_log(format!(
                            "{}: buy on {}, sell on {}: {} net ({} gross)",
                            r.symbol,
                            e.buy.name(),
                            e.sell.name(),
                            fmt_edge(e.net_bps),
                            fmt_edge(e.gross_bps)
                        ));
                    }
                }
                (None, Some(w)) => edge_log(format!("{}: {} edge closed", r.symbol, w.route())),
                (None, None) => {}
            }
        }
    }

    fn log_changes(&mut self, rows: &[NewListing]) {
        self.log_edges(rows);
        let mut log = |text: String| {
            tracing::info!("scanner: {text}");
            self.events.push_front(Event::now(EventKind::Listing, text));
        };
        let Some(seen) = self.seen.as_ref() else {
            log(format!(
                "First scan: {} tokens listed on Gate or MEXC in the last {NEW_WITHIN_DAYS} days are not on Binance spot",
                rows.len()
            ));
            self.events.truncate(MAX_EVENTS);
            return;
        };
        for r in rows {
            let venues = r
                .venues
                .iter()
                .map(|v| v.exchange.name())
                .collect::<Vec<_>>()
                .join(", ");
            if !seen.contains(&r.symbol) {
                let when = if r.listed_at > unix_now() {
                    format!("opens {}", r.age)
                } else {
                    format!("listed {}", r.age)
                };
                log(format!(
                    "{} ({}): new on {venues}, {when}, {} vol, score {}",
                    r.symbol,
                    r.name,
                    fmt_usd(r.volume_usd),
                    r.score
                ));
            }
            if let Some(p) = self.prev.get(&r.symbol) {
                if r.binance_futures && !p.binance_futures {
                    log(format!("{}: now on Binance futures", r.symbol));
                }
                if r.binance_alpha && !p.binance_alpha {
                    log(format!("{}: now on Binance Alpha", r.symbol));
                }
            }
        }
        for symbol in self.prev.keys() {
            if self.src.binance_spot.contains(symbol) {
                log(format!("{symbol}: now listed on Binance spot"));
            }
        }
        self.events.truncate(MAX_EVENTS);
    }
}

#[cfg(test)]
mod tests;
