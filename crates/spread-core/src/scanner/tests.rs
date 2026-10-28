use super::*;

const NOW: i64 = 1_790_000_000;

fn sources() -> Sources {
    let pair = |base: &str, name: &str, start: i64| GatePair {
        base: base.into(),
        base_name: name.into(),
        quote: "USDT".into(),
        buy_start: start,
    };
    let mexc = |base: &str| MexcSymbol {
        symbol: format!("{base}USDT"),
        status: "1".into(),
        base_asset: base.into(),
        quote_asset: "USDT".into(),
        full_name: Some(base.into()),
        is_spot_trading_allowed: true,
        concept_plates: None,
    };
    let mut src = Sources {
        binance_spot: ["SOL".to_owned()].into(),
        binance_futures: ["NEWT".to_owned()].into(),
        gate_pairs: vec![
            pair("SOL", "Solana", NOW - 900 * DAY),
            pair("NEWT", "Newt", NOW - 3 * DAY),
            pair("SOON", "Soon", NOW + 2 * 3600),
            pair("SUI5S", "SUI5xShort", NOW - DAY),
            pair("OLDG", "Old on Gate", NOW - 200 * DAY),
            pair("AAPLX", "Apple xStock", NOW - DAY),
        ],
        mexc_symbols: vec![mexc("NEWT"), mexc("MX1"), mexc("OLDM")],
        ..Default::default()
    };
    src.gate_currencies.insert(
        "AAPLX".into(),
        GateCurrency {
            currency: "AAPLX".into(),
            market_cap: None,
            category: vec!["stocks".into()],
            deposit_disabled: false,
            withdraw_disabled: false,
        },
    );
    let ticker = |vol: f64, change: f64, bid: f64, ask: f64| Ticker {
        volume_usd: Some(vol),
        change_pct: Some(change),
        bid: Some(bid),
        ask: Some(ask),
    };
    // NEWT: 1.00 / 1.0012 on Gate, its busiest venue, is about a 12 bps spread.
    src.gate_tickers
        .insert("NEWT".into(), ticker(800_000.0, 12.0, 1.0, 1.0012));
    src.mexc_tickers
        .insert("NEWT".into(), ticker(400_000.0, 10.0, 1.0, 1.05));
    src.mexc_tickers
        .insert("MX1".into(), ticker(20_000.0, 250.0, 1.0, 1.02));
    src.bybit
        .insert("NEWT".into(), ticker(300_000.0, 11.0, 1.0, 1.002));
    src
}

#[test]
fn finds_recent_listings_binance_spot_lacks() {
    let dates: MexcDates = [
        ("NEWTUSDT".into(), Some(NOW - 5 * DAY)),
        ("MX1USDT".into(), Some(NOW - DAY)),
        ("OLDMUSDT".into(), None),
    ]
    .into();
    let rows = build(&sources(), &dates, NOW);
    let symbols: Vec<&str> = rows.iter().map(|r| r.symbol.as_str()).collect();
    // SOL is on Binance, SUI5S leveraged, AAPLX a stock, OLDG and OLDM too old.
    assert_eq!(symbols, ["NEWT", "SOON", "MX1"]);

    let newt = &rows[0];
    // Earliest of Gate (3 d) and MEXC (5 d).
    assert_eq!(newt.listed_at, NOW - 5 * DAY);
    assert_eq!(newt.age, "5 d ago");
    assert_eq!(newt.volume_usd, 1_500_000.0);
    // From Gate, the busier venue.
    assert_eq!(newt.change_pct, Some(12.0));
    assert!(newt.binance_futures);
    let good: Vec<&str> = newt
        .signals
        .iter()
        .filter(|s| s.good)
        .map(|s| s.text.as_str())
        .collect();
    assert_eq!(
        good,
        [
            "Binance futures",
            "on Bybit",
            "Gate + MEXC",
            "vol $1.5M",
            "tight spread 12 bps (0.12 %)"
        ]
    );
    assert_eq!(
            newt.summary_line(),
            format!(
                "NEWT (Newt) | listed 5 d ago ({}) | Gate $800k, MEXC $400k, Bybit $300k | vol $1.5M | 24h +12.0 % \
                 | spread 12 bps (0.12 %) | edge -27 bps net Gate → MEXC | score 7 + Binance futures, on Bybit, Gate + MEXC, vol $1.5M, tight spread 12 bps (0.12 %)",
                newt.listed_date
            )
        );

    assert_eq!(rows[1].age, "in 2 h");
    let mx1 = &rows[2];
    assert!(mx1
        .signals
        .iter()
        .any(|s| !s.good && s.text.starts_with("pumped")));
    // 1.00 / 1.02 on MEXC: about 198 bps.
    assert!(mx1
        .signals
        .iter()
        .any(|s| !s.good && s.text == "wide spread 198 bps (1.98 %)"));
    assert!(mx1.score < 0);
}

#[test]
fn prices_the_best_edge_between_exchanges() {
    let t = |vol: f64, bid: f64, ask: f64| Ticker {
        volume_usd: Some(vol),
        change_pct: None,
        bid: Some(bid),
        ask: Some(ask),
    };
    let gate = |deposit_disabled: bool, withdraw_disabled: bool| GateCurrency {
        currency: "X".into(),
        market_cap: None,
        category: Vec::new(),
        deposit_disabled,
        withdraw_disabled,
    };
    let tickers = [
        (Exchange::Mexc, t(50_000.0, 1.000, 1.001)),
        (Exchange::Gate, t(80_000.0, 1.010, 1.011)),
        // Crossed far above everyone, but too thin to count.
        (Exchange::Okx, t(2_000.0, 1.200, 1.201)),
    ];
    let e = best_edge(&tickers, None).unwrap();
    // (1.010 - 1.001) / 1.0055 ≈ 89.5 bps, minus 5 (MEXC) and 10 (Gate).
    assert_eq!((e.buy, e.sell), (Exchange::Mexc, Exchange::Gate));
    assert!((e.gross_bps - 89.51).abs() < 0.01, "{e:?}");
    assert!((e.gross_bps - e.net_bps - 15.0).abs() < 1e-9);
    assert!(e.open());
    assert_eq!(e.route(), "MEXC → Gate");
    assert_eq!(fmt_edge(e.net_bps), "+75 bps");

    // Selling on Gate needs deposits there; withdrawals don't matter.
    assert!(best_edge(&tickers, Some(&gate(false, true)))
        .unwrap()
        .open());
    let blocked = best_edge(&tickers, Some(&gate(true, false))).unwrap();
    assert_eq!(blocked.blocked.as_deref(), Some("Gate deposits closed"));
    assert!(!blocked.open());

    // A 50 % gap is two tokens sharing a ticker, not money on the table.
    let twins = [
        (Exchange::Mexc, t(50_000.0, 1.0, 1.001)),
        (Exchange::Bybit, t(50_000.0, 1.6, 1.601)),
    ];
    let e = best_edge(&twins, None).unwrap();
    assert!(e.suspect() && !e.open());

    // One exchange alone has no edge.
    assert_eq!(best_edge(&tickers[..1], None), None);
}

#[test]
fn undated_mexc_tokens_wait() {
    let rows = build(&sources(), &MexcDates::new(), NOW);
    assert!(rows.iter().all(|r| r.symbol != "MX1"));
}

#[test]
fn parses_symbols_and_shapes() {
    assert_eq!(binance_base("PEPEUSDT"), Some("PEPE"));
    assert_eq!(binance_base("ETHBTC"), None);
    assert_eq!(futures_base("1000PEPEUSDT"), Some("PEPE"));
    assert_eq!(futures_base("1MBABYDOGEUSDT"), Some("BABYDOGE"));
    assert!(is_leveraged("SUI5S", "SUI5xShort"));
    assert!(!is_leveraged("B3S", "Base Three Social"));
    assert_eq!(fmt_usd(1_234_567.0), "$1.2M");
    assert_eq!(fmt_usd(340_000.0), "$340k");
    assert_eq!(age(NOW - 90 * 60, NOW), "1 h ago");
    assert_eq!(fmt_spread(0.28), "0.3 bps (0.003 %)");
    assert_eq!(fmt_spread(177.4), "177 bps (1.77 %)");
    assert_eq!(fmt_edge(-4.25), "-4.2 bps");
}

#[test]
fn parses_exchange_responses() {
    // Trimmed from real responses.
    let a: AlphaList = serde_json::from_str(
            r#"{"code":"000000","data":[{"symbol":"GSTOCK","name":"Gstock","marketCap":"22553254.66","holders":"10079","listingCex":false}]}"#,
        )
        .unwrap();
    assert_eq!(a.data[0].holders.as_deref(), Some("10079"));
    let m: MexcInfo = serde_json::from_str(
            r#"{"symbols":[{"symbol":"METALUSDT","status":"1","baseAsset":"METAL","quoteAsset":"USDT","fullName":"Metal Blockchain","isSpotTradingAllowed":true,"conceptPlates":["Innovation"]}]}"#,
        )
        .unwrap();
    assert_eq!(m.symbols[0].base_asset, "METAL");
    let t: Vec<MexcTicker> = serde_json::from_str(
            r#"[{"symbol":"METALUSDT","priceChangePercent":"-0.0473","quoteVolume":"7308.89","count":null}]"#,
        )
        .unwrap();
    assert_eq!(num(&t[0].price_change_percent), Some(-0.0473));
    let o: OkxResp =
        serde_json::from_str(r#"{"data":[{"instId":"XASTS-USDT","volCcy24h":"12856.8"}]}"#)
            .unwrap();
    assert_eq!(o.data[0].inst_id, "XASTS-USDT");
    let b: BybitResp = serde_json::from_str(
        r#"{"result":{"list":[{"symbol":"BTCUSDT","turnover24h":"56824.9"}]}}"#,
    )
    .unwrap();
    assert_eq!(num(&b.result.list[0].turnover24h), Some(56824.9));
}

#[tokio::test]
#[ignore = "hits every exchange's live API"]
async fn live_scan() {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();
    let mut src = Sources::default();
    let errors = src.update(&http).await;
    assert!(errors.is_empty(), "{errors:?}");
    let rows = build(&src, &MexcDates::new(), unix_now());
    for r in rows.iter().take(15) {
        let signals: Vec<&str> = r.signals.iter().map(|s| s.text.as_str()).collect();
        println!(
            "{:>10} {:>12} score {:>2} {:>7} {signals:?}",
            r.symbol,
            r.age,
            r.score,
            fmt_usd(r.volume_usd)
        );
    }
    let mut edged: Vec<&NewListing> = rows.iter().filter(|r| r.edge.is_some()).collect();
    edged.sort_by(|a, b| {
        let net = |r: &NewListing| r.edge.as_ref().map_or(f64::MIN, |e| e.net_bps);
        net(b).total_cmp(&net(a))
    });
    for r in edged.iter().take(15) {
        let e = r.edge.as_ref().unwrap();
        println!(
            "{:>10} {:>14} net {:>9} gross {:>9} open {} suspect {} {:?}",
            r.symbol,
            e.route(),
            fmt_edge(e.net_bps),
            fmt_edge(e.gross_bps),
            e.open(),
            e.suspect(),
            e.blocked
        );
    }
    assert!(!rows.is_empty());
}
