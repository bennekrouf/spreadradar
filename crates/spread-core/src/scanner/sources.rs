//! Where listings come from: each exchange's list of markets.

use super::*;

impl Exchange {
    pub fn name(self) -> &'static str {
        match self {
            Exchange::Gate => "Gate",
            Exchange::Mexc => "MEXC",
            Exchange::Bybit => "Bybit",
            Exchange::Okx => "OKX",
        }
    }

    pub(super) fn venue(self) -> Venue {
        match self {
            Exchange::Gate => Venue::Gate,
            Exchange::Mexc => Venue::Mexc,
            Exchange::Bybit => Venue::Bybit,
            Exchange::Okx => Venue::Okx,
        }
    }

    /// The same regular-tier taker fee the Market tab uses.
    pub fn taker_fee_bps(self) -> f64 {
        EngineConfig::default().taker_fee_bps[self.venue().idx()]
    }
}

impl Sources {
    /// Fetches everything at once; returns one line per failed source.
    pub(super) async fn update(&mut self, http: &reqwest::Client) -> Vec<String> {
        let (bn, bnf, alpha, gp, gt, gc, mi, mt, by, okx) = tokio::join!(
            get::<Vec<BinancePrice>>(http, "https://api.binance.com/api/v3/ticker/price"),
            get::<Vec<BinancePrice>>(http, "https://fapi.binance.com/fapi/v1/ticker/price"),
            get::<AlphaList>(
                http,
                "https://www.binance.com/bapi/defi/v1/public/wallet-direct/buw/wallet/cex/alpha/all/token/list"
            ),
            get::<Vec<GatePair>>(http, "https://api.gateio.ws/api/v4/spot/currency_pairs"),
            get::<Vec<GateTicker>>(http, "https://api.gateio.ws/api/v4/spot/tickers"),
            get::<Vec<GateCurrency>>(http, "https://api.gateio.ws/api/v4/spot/currencies"),
            get::<MexcInfo>(http, "https://api.mexc.com/api/v3/exchangeInfo"),
            get::<Vec<MexcTicker>>(http, "https://api.mexc.com/api/v3/ticker/24hr"),
            get::<BybitResp>(http, "https://api.bybit.com/v5/market/tickers?category=spot"),
            get::<OkxResp>(http, "https://www.okx.com/api/v5/market/tickers?instType=SPOT"),
        );
        let mut errors = Vec::new();
        let mut keep = |name: &str, r: Result<()>| {
            if let Err(e) = r {
                tracing::warn!("scanner: {name} failed: {e:#}");
                errors.push(format!("{name}: {e:#}"));
            }
        };
        keep(
            "Binance spot",
            bn.map(|v| {
                self.binance_spot = v
                    .iter()
                    .filter_map(|p| binance_base(&p.symbol))
                    .map(str::to_owned)
                    .collect();
            }),
        );
        keep(
            "Binance futures",
            bnf.map(|v| {
                self.binance_futures = v
                    .iter()
                    .filter_map(|p| futures_base(&p.symbol))
                    .map(str::to_owned)
                    .collect();
            }),
        );
        keep(
            "Binance Alpha",
            alpha.map(|v| {
                self.alpha = v
                    .data
                    .into_iter()
                    .map(|t| (t.symbol.to_uppercase(), t))
                    .collect();
            }),
        );
        keep(
            "Gate pairs",
            gp.map(|v| self.gate_pairs = v.into_iter().filter(|p| p.quote == "USDT").collect()),
        );
        keep(
            "Gate tickers",
            gt.map(|v| {
                self.gate_tickers = v
                    .into_iter()
                    .filter_map(|t| {
                        let base = t.currency_pair.strip_suffix("_USDT")?.to_owned();
                        let ticker = Ticker {
                            volume_usd: num(&t.quote_volume),
                            change_pct: num(&t.change_percentage),
                            bid: num(&t.highest_bid),
                            ask: num(&t.lowest_ask),
                        };
                        Some((base, ticker))
                    })
                    .collect();
            }),
        );
        keep(
            "Gate currencies",
            gc.map(|v| {
                self.gate_currencies = v.into_iter().map(|c| (c.currency.clone(), c)).collect();
            }),
        );
        keep(
            "MEXC symbols",
            mi.map(|v| {
                self.mexc_symbols = v
                    .symbols
                    .into_iter()
                    .filter(|s| {
                        s.quote_asset == "USDT" && s.status == "1" && s.is_spot_trading_allowed
                    })
                    .collect();
            }),
        );
        keep(
            "MEXC tickers",
            mt.map(|v| {
                self.mexc_tickers = v
                    .into_iter()
                    .filter_map(|t| {
                        let base = t.symbol.strip_suffix("USDT")?.to_owned();
                        let ticker = Ticker {
                            volume_usd: num(&t.quote_volume),
                            change_pct: num(&t.price_change_percent).map(|f| f * 100.0),
                            bid: num(&t.bid_price),
                            ask: num(&t.ask_price),
                        };
                        Some((base, ticker))
                    })
                    .collect();
            }),
        );
        keep(
            "Bybit",
            by.map(|v| {
                self.bybit = v
                    .result
                    .list
                    .into_iter()
                    .filter_map(|t| {
                        let ticker = Ticker {
                            volume_usd: num(&t.turnover24h),
                            change_pct: num(&t.price24h_pcnt).map(|f| f * 100.0),
                            bid: num(&t.bid1_price),
                            ask: num(&t.ask1_price),
                        };
                        Some((t.symbol.strip_suffix("USDT")?.to_owned(), ticker))
                    })
                    .collect();
            }),
        );
        keep(
            "OKX",
            okx.map(|v| {
                self.okx = v
                    .data
                    .into_iter()
                    .filter_map(|t| {
                        let (last, open) = (num(&t.last), num(&t.open24h).filter(|o| *o > 0.0));
                        let ticker = Ticker {
                            volume_usd: num(&t.vol_ccy24h),
                            change_pct: last.zip(open).map(|(l, o)| (l / o - 1.0) * 100.0),
                            bid: num(&t.bid_px),
                            ask: num(&t.ask_px),
                        };
                        Some((t.inst_id.strip_suffix("-USDT")?.to_owned(), ticker))
                    })
                    .collect();
            }),
        );
        errors
    }

    /// MEXC tokens Binance doesn't list, as candidates for dating.
    pub(super) fn mexc_candidates(&self) -> impl Iterator<Item = &MexcSymbol> {
        self.mexc_symbols
            .iter()
            .filter(|s| !self.binance_spot.contains(&s.base_asset) && !mexc_excluded(s))
    }
}
