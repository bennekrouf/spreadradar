//! Assessing a new listing: its rows, its best edge, and whether it is worth a
//! look.

use super::*;

pub(super) fn build(src: &Sources, dates: &MexcDates, now: i64) -> Vec<NewListing> {
    // Base → (earliest listing, known to be old, name).
    let mut found: HashMap<&str, (Option<i64>, bool, &str)> = HashMap::new();
    for p in &src.gate_pairs {
        if src.binance_spot.contains(&p.base) || gate_excluded(p, &src.gate_currencies) {
            continue;
        }
        let e = found.entry(&p.base).or_insert((None, false, &p.base_name));
        e.0 = Some(e.0.map_or(p.buy_start, |t| t.min(p.buy_start)));
    }
    for s in src.mexc_candidates() {
        // Not dated yet: shown once it is.
        let Some(date) = dates.get(&s.symbol) else {
            continue;
        };
        let e = found.entry(&s.base_asset).or_insert((
            None,
            false,
            s.full_name.as_deref().unwrap_or(""),
        ));
        match date {
            Some(t) => e.0 = Some(e.0.map_or(*t, |x| x.min(*t))),
            None => e.1 = true,
        }
    }

    let mut rows: Vec<NewListing> = found
        .into_iter()
        .filter_map(|(base, (listed_at, old, name))| {
            let listed_at = listed_at?;
            (!old && now - listed_at <= NEW_WITHIN_DAYS * DAY)
                .then(|| row(src, base, name, listed_at, now))
        })
        .collect();
    rows.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(b.volume_usd.total_cmp(&a.volume_usd))
    });
    rows
}

pub(super) fn row(src: &Sources, base: &str, name: &str, listed_at: i64, now: i64) -> NewListing {
    let gate_listed = src.gate_pairs.iter().any(|p| p.base == base);
    let mexc_listed = src.mexc_symbols.iter().any(|s| s.base_asset == base);
    // A listed pair may have no ticker yet, e.g. before trading opens.
    let listed = |yes: bool, t: Option<&Ticker>| yes.then(|| t.copied().unwrap_or_default());
    let tickers: Vec<(Exchange, Ticker)> = [
        (
            Exchange::Gate,
            listed(gate_listed, src.gate_tickers.get(base)),
        ),
        (
            Exchange::Mexc,
            listed(mexc_listed, src.mexc_tickers.get(base)),
        ),
        (Exchange::Bybit, src.bybit.get(base).copied()),
        (Exchange::Okx, src.okx.get(base).copied()),
    ]
    .into_iter()
    .filter_map(|(e, t)| Some((e, t?)))
    .collect();
    // Price change and spread come from the most traded venue.
    let main = tickers
        .iter()
        .filter_map(|(_, t)| Some((t.volume_usd?, *t)))
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, t)| t);
    let venues: Vec<VenueQuote> = tickers
        .iter()
        .map(|(e, t)| VenueQuote {
            exchange: *e,
            volume_usd: t.volume_usd,
            spread_bps: t.spread_bps(),
        })
        .collect();
    let edge = best_edge(&tickers, src.gate_currencies.get(base));
    let alpha = src.alpha.get(base);
    let market_cap = alpha
        .and_then(|a| num(&a.market_cap))
        .or_else(|| {
            src.gate_currencies
                .get(base)
                .and_then(|c| num(&c.market_cap))
        })
        .filter(|m| *m > 0.0);
    let name = if name.is_empty() {
        alpha.map_or("", |a| a.name.as_str())
    } else {
        name
    };

    let mut r = NewListing {
        symbol: base.to_owned(),
        name: name.to_owned(),
        listed_at,
        age: age(listed_at, now),
        listed_date: chrono::DateTime::from_timestamp(listed_at, 0)
            .map(|t| {
                t.with_timezone(&chrono::Local)
                    .format("%d %b %H:%M")
                    .to_string()
            })
            .unwrap_or_default(),
        volume_usd: venues.iter().filter_map(|v| v.volume_usd).sum(),
        venues,
        change_pct: main.and_then(|t| t.change_pct),
        spread_bps: main.and_then(|t| t.spread_bps()),
        edge,
        market_cap,
        holders: alpha.and_then(|a| a.holders.as_deref()?.parse().ok()),
        binance_futures: src.binance_futures.contains(base),
        binance_alpha: alpha.is_some(),
        signals: Vec::new(),
        score: 0,
        fresh: false,
    };
    (r.signals, r.score) = assess(&r, now);
    r
}

/// The edge with the highest net among exchanges with enough volume, the
/// same way the Market tab prices a route.
pub(super) fn best_edge(
    tickers: &[(Exchange, Ticker)],
    gate: Option<&GateCurrency>,
) -> Option<Edge> {
    let legs: Vec<(Exchange, Ticker, f64)> = tickers
        .iter()
        .filter(|(_, t)| t.volume_usd.is_some_and(|v| v >= EDGE_MIN_VOLUME_USD))
        .filter_map(|(e, t)| Some((*e, *t, t.mid()?)))
        .collect();
    let mut best: Option<Edge> = None;
    for (buy, b, buy_mid) in &legs {
        for (sell, s, sell_mid) in &legs {
            if buy == sell {
                continue;
            }
            let (Some(ask), Some(bid)) = (b.ask, s.bid) else {
                continue;
            };
            let gross_bps = (bid - ask) / ((buy_mid + sell_mid) / 2.0) * 1e4;
            let net_bps = gross_bps - buy.taker_fee_bps() - sell.taker_fee_bps();
            if best.as_ref().is_some_and(|e| e.net_bps >= net_bps) {
                continue;
            }
            let blocked = gate.and_then(|c| {
                if *buy == Exchange::Gate && c.withdraw_disabled {
                    Some("Gate withdrawals closed")
                } else if *sell == Exchange::Gate && c.deposit_disabled {
                    Some("Gate deposits closed")
                } else {
                    None
                }
            });
            best = Some(Edge {
                buy: *buy,
                sell: *sell,
                gross_bps,
                net_bps,
                blocked: blocked.map(str::to_owned),
            });
        }
    }
    best
}

/// The reasons to look closer, and the warnings, behind the score.
pub(super) fn assess(r: &NewListing, now: i64) -> (Vec<Signal>, i32) {
    let mut out = Vec::new();
    let mut good = |text: String, points: i32| out.push((Signal { text, good: true }, points));
    if r.binance_futures {
        good("Binance futures".into(), 2);
    }
    if r.binance_alpha {
        good("Binance Alpha".into(), 1);
    }
    let others: Vec<&str> = r
        .venues
        .iter()
        .filter(|v| matches!(v.exchange, Exchange::Bybit | Exchange::Okx))
        .map(|v| v.exchange.name())
        .collect();
    if !others.is_empty() {
        good(format!("on {}", others.join(" + ")), others.len() as i32);
    }
    let on = |e: Exchange| r.venues.iter().any(|v| v.exchange == e);
    if on(Exchange::Gate) && on(Exchange::Mexc) {
        good("Gate + MEXC".into(), 1);
    }
    if r.volume_usd >= 1_000_000.0 {
        good(format!("vol {}", fmt_usd(r.volume_usd)), 2);
    } else if r.volume_usd >= 250_000.0 {
        good(format!("vol {}", fmt_usd(r.volume_usd)), 1);
    }
    if let Some(m) = r.market_cap.filter(|m| *m >= 10_000_000.0) {
        good(
            format!("mcap {}", fmt_usd(m)),
            if m >= 100_000_000.0 { 2 } else { 1 },
        );
    }
    if let Some(h) = r.holders.filter(|h| *h >= 10_000) {
        good(format!("{}k holders", h / 1000), 1);
    }
    if let Some(s) = r.spread_bps.filter(|s| *s <= TIGHT_SPREAD_BPS) {
        good(format!("tight spread {}", fmt_spread(s)), 1);
    }

    let mut bad = |text: String| out.push((Signal { text, good: false }, -1));
    let upcoming = r.listed_at > now;
    if !upcoming && r.volume_usd < 50_000.0 {
        bad(format!("thin: {} vol", fmt_usd(r.volume_usd)));
    }
    if let Some(m) = r.market_cap.filter(|m| *m < 2_000_000.0) {
        bad(format!("tiny mcap {}", fmt_usd(m)));
    }
    match r.change_pct {
        Some(c) if c >= 100.0 => bad(format!("pumped {c:+.0} %")),
        Some(c) if c <= -50.0 => bad(format!("dumped {c:+.0} %")),
        _ => {}
    }
    if let Some(s) = r.spread_bps.filter(|s| *s >= WIDE_SPREAD_BPS) {
        bad(format!("wide spread {}", fmt_spread(s)));
    }
    let score = out.iter().map(|(_, p)| p).sum();
    (out.into_iter().map(|(s, _)| s).collect(), score)
}

impl NewListing {
    /// The row as one line of plain text, for pasting elsewhere.
    pub fn summary_line(&self) -> String {
        let venues: Vec<String> = self
            .venues
            .iter()
            .map(|v| match v.volume_usd {
                Some(vol) => format!("{} {}", v.exchange.name(), fmt_usd(vol)),
                None => v.exchange.name().to_owned(),
            })
            .collect();
        let pick = |good: bool| {
            self.signals
                .iter()
                .filter(|s| s.good == good)
                .map(|s| s.text.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut parts = vec![
            format!("{} ({})", self.symbol, self.name),
            format!("listed {} ({})", self.age, self.listed_date),
            venues.join(", "),
            format!("vol {}", fmt_usd(self.volume_usd)),
        ];
        if let Some(c) = self.change_pct {
            parts.push(format!("24h {c:+.1} %"));
        }
        if let Some(m) = self.market_cap {
            parts.push(format!("mcap {}", fmt_usd(m)));
        }
        if let Some(s) = self.spread_bps {
            parts.push(format!("spread {}", fmt_spread(s)));
        }
        if let Some(e) = &self.edge {
            let mut edge = format!("edge {} net {}", fmt_edge(e.net_bps), e.route());
            if let Some(b) = &e.blocked {
                edge.push_str(&format!(" ({b})"));
            }
            parts.push(edge);
        }
        let mut score = format!("score {}", self.score);
        for (label, list) in [("+", pick(true)), ("-", pick(false))] {
            if !list.is_empty() {
                score.push_str(&format!(" {label} {list}"));
            }
        }
        parts.push(score);
        parts.join(" | ")
    }
}
