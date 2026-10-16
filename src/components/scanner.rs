use dioxus::prelude::*;
use std::time::Duration;

use spread_core::scanner::{
    fmt_bps, fmt_edge, fmt_pct, fmt_spread, fmt_usd, NewListing, ScanSnapshot, EDGE_NET_BPS,
    NEW_WITHIN_DAYS, SCAN_EVERY, SUSPECT_EDGE_BPS, TIGHT_SPREAD_BPS, WIDE_SPREAD_BPS,
};
use spread_core::MarketCmd;
use tokio::sync::mpsc;

use super::{EventLog, Tab};

/// How long a Copy button says "Copied".
const COPIED_FOR: Duration = Duration::from_millis(1500);

/// Minimum score filter choices: label and threshold.
const MIN_SCORES: [(&str, i32); 3] = [("All", i32::MIN), ("Score 2+", 2), ("Score 4+", 4)];

#[component]
pub fn NewListingsPanel(scan: ScanSnapshot, watched: Vec<String>) -> Element {
    let tx = use_context::<mpsc::Sender<MarketCmd>>();
    let mut tab = use_context::<Signal<Tab>>();
    let mut min_score = use_signal(|| i32::MIN);
    let mut by_edge = use_signal(|| false);
    // Follows the token's spread in the watchlist and shows it.
    let watch = use_callback(move |symbol: String| {
        if let Err(e) = tx.try_send(MarketCmd::Add(symbol)) {
            tracing::warn!("watch dropped: {e}");
        }
        tab.set(Tab::Market);
    });

    let mut shown: Vec<&NewListing> = scan
        .rows
        .iter()
        .filter(|r| r.score >= min_score())
        .collect();
    if by_edge() {
        // Edges worth a look first, then the rest by net; none at the end.
        let key = |r: &NewListing| {
            r.edge
                .as_ref()
                .map(|e| (e.open(), e.net_bps))
                .unwrap_or((false, f64::NEG_INFINITY))
        };
        shown.sort_by(|a, b| {
            let (ka, kb) = (key(a), key(b));
            kb.0.cmp(&ka.0).then(kb.1.total_cmp(&ka.1))
        });
    }
    let every = SCAN_EVERY.as_secs() / 60;

    rsx! {
        header { class: "top",
            h1 { "New listings" }
            span { class: "muted",
                "On Gate or MEXC in the last {NEW_WITHIN_DAYS} days or scheduled, not on Binance spot. Scanned every {every} min."
            }
        }
        div { class: "card",
            div { class: "scan-bar",
                span { class: "muted small",
                    match (&scan.last_scan, &scan.progress) {
                        (_, Some(p)) => p.clone(),
                        (Some(t), None) => format!("Last scan {t} · {} tokens", scan.rows.len()),
                        (None, None) => "Starting…".into(),
                    }
                }
                div { class: "filters",
                    for (label, edge) in [("By score", false), ("By edge", true)] {
                        button {
                            class: if by_edge() == edge { "chip-btn active" } else { "chip-btn" },
                            onclick: move |_| by_edge.set(edge),
                            "{label}"
                        }
                    }
                    span { class: "filter-gap" }
                    for (label, min) in MIN_SCORES {
                        button {
                            class: if min_score() == min { "chip-btn active" } else { "chip-btn" },
                            onclick: move |_| min_score.set(min),
                            "{label}"
                        }
                    }
                }
            }
            for e in scan.errors.iter() {
                p { class: "error small", "{e} (showing its previous data)" }
            }
            if shown.is_empty() && scan.last_scan.is_some() {
                p { class: "muted", "Nothing matches." }
            }
            if !shown.is_empty() {
                table { class: "listings",
                    thead {
                        tr {
                            th { "Token" }
                            th { "Listed" }
                            th { "Exchanges" }
                            th { class: "num", "24h vol" }
                            th { class: "num", "24h" }
                            th { class: "num", "Mkt cap" }
                            th { class: "num", title: "Bid/ask spread on the most traded exchange", "Spread" }
                            th { class: "num",
                                title: "Best buy-here, sell-there gap between two exchanges, after the taker fee on both legs. From the last scan's best bid and ask, so up to {every} min old, with no order book depth. Exchanges under $10k of 24h volume are left out.",
                                "Edge"
                            }
                            th { title: "Why it may be serious (green) or not (red)", "Signals" }
                            th { class: "num", title: "Green signals minus red ones", "Score" }
                            th {}
                        }
                    }
                    tbody {
                        for r in shown {
                            ListingRow {
                                key: "{r.symbol}",
                                row: r.clone(),
                                watched: watched.contains(&r.symbol),
                                on_watch: watch,
                            }
                        }
                    }
                }
            }
        }
        EventLog { title: "Detections", events: scan.events.clone() }
    }
}

#[component]
fn ListingRow(row: NewListing, watched: bool, on_watch: Callback<String>) -> Element {
    let r = row;
    let symbol = r.symbol.clone();
    let line = r.summary_line();
    let mut copied = use_signal(|| None::<bool>);
    let copy = move |_| {
        let ok = arboard::Clipboard::new().and_then(|mut c| c.set_text(line.clone()));
        if let Err(e) = &ok {
            tracing::warn!("copy failed: {e}");
        }
        copied.set(Some(ok.is_ok()));
        spawn(async move {
            tokio::time::sleep(COPIED_FOR).await;
            copied.set(None);
        });
    };
    let spread_class = match r.spread_bps {
        Some(s) if s <= TIGHT_SPREAD_BPS => "num pos",
        Some(s) if s >= WIDE_SPREAD_BPS => "num down",
        _ => "num",
    };
    let edge_class = match &r.edge {
        Some(e) if e.open() => "num pos strong",
        Some(e) if e.net_bps >= EDGE_NET_BPS => "num warn",
        _ => "num muted",
    };
    let change_class = match r.change_pct {
        Some(c) if c > 0.0 => "num pos",
        Some(c) if c < 0.0 => "num down",
        _ => "num muted",
    };
    rsx! {
        tr { class: if r.fresh { "row early" } else { "row" },
            td {
                strong { "{r.symbol}" }
                if r.fresh {
                    span { class: "badge new", title: "Appeared since the app started", "new" }
                }
                div { class: "muted small", "{r.name}" }
            }
            td { class: "nowrap", title: "{r.listed_date}", "{r.age}" }
            td {
                for v in r.venues.iter() {
                    span { class: "chip",
                        title: format!(
                            "{} · spread {}",
                            v.volume_usd.map_or("no 24h volume".to_owned(), |vol| format!("{} 24h", fmt_usd(vol))),
                            v.spread_bps.map_or("–".to_owned(), fmt_spread),
                        ),
                        "{v.exchange.name()}"
                    }
                }
            }
            td { class: "num", {fmt_usd(r.volume_usd)} }
            td { class: change_class, {r.change_pct.map_or("–".to_owned(), |c| format!("{c:+.1} %"))} }
            td { class: "num", {r.market_cap.map_or("–".to_owned(), fmt_usd)} }
            td { class: spread_class,
                match r.spread_bps {
                    Some(s) => rsx! {
                        {fmt_bps(s)}
                        div { class: "muted small", {fmt_pct(s)} }
                    },
                    None => rsx! { "–" },
                }
            }
            td { class: edge_class,
                title: r.edge.as_ref().map(|e| format!(
                    "Buy on {} at the ask, sell on {} at the bid: {} gross, {} net of {} + {} fees",
                    e.buy.name(),
                    e.sell.name(),
                    fmt_edge(e.gross_bps),
                    fmt_edge(e.net_bps),
                    fmt_bps(e.buy.taker_fee_bps()),
                    fmt_bps(e.sell.taker_fee_bps()),
                )),
                match &r.edge {
                    Some(e) => rsx! {
                        {fmt_edge(e.net_bps)}
                        div { class: "muted small nowrap", {e.route()} }
                        if let Some(b) = &e.blocked {
                            div { class: "small down", "{b}" }
                        } else if e.suspect() {
                            div {
                                class: "small down",
                                title: "Gaps over {fmt_bps(SUSPECT_EDGE_BPS)} are nearly always two different tokens sharing a ticker",
                                "same token?"
                            }
                        }
                    },
                    None => rsx! { "–" },
                }
            }
            td {
                for s in r.signals.iter() {
                    span { class: if s.good { "chip good" } else { "chip bad" }, "{s.text}" }
                }
            }
            td { class: if r.score > 0 { "num pos strong" } else { "num muted" }, "{r.score}" }
            td { class: "nowrap",
                button {
                    class: "btn small-btn",
                    title: "Copy this line as text",
                    onclick: copy,
                    match copied() {
                        Some(true) => "Copied",
                        Some(false) => "Failed",
                        None => "Copy",
                    }
                }
                button {
                    class: "btn small-btn",
                    disabled: watched,
                    title: if watched { "{r.symbol} is already in the watchlist" } else { "Add {r.symbol} to the watchlist to follow its spread" },
                    onclick: move |_| on_watch.call(symbol.clone()),
                    if watched { "Watching" } else { "Watch" }
                }
            }
        }
    }
}
