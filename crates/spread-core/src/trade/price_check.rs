//! Jupiter's quote, held to what the exchanges pay for SOL right now.
//!
//! The guard bounds a swap to its quote, but the quote is Jupiter's word:
//! a route through a drained pool, a stale answer or a hostile server would
//! set every floor from a bad price, and the swap would pass them all. Here
//! the quoted price is compared with the median top of book of the exchanges
//! the market follows, before anything is built or signed.
//!
//! Only a quote worse for the wallet is refused. A better one costs nothing:
//! the floors hold the swap to it, and the simulation fails if it can't be had.

use super::Side;
use crate::model::ExchangeQuote;

/// One exchange off on its own must not decide the reference.
pub const MIN_EXCHANGES: usize = 2;

/// `Ok` when the quote is no more than `max_gap_bps` worse than the
/// exchanges: below their median bid when selling, above their median ask
/// when buying. `exchanges` is `None` while SOL is not followed.
pub fn check_quote(
    side: Side,
    quoted_px: f64,
    exchanges: Option<&[ExchangeQuote]>,
    max_gap_bps: u16,
) -> Result<(), String> {
    let Some(exchanges) = exchanges else {
        return Err(
            "SOL is not followed in the watchlist, and its exchange prices check Jupiter's quote"
                .into(),
        );
    };
    let (book_side, prices): (&str, Vec<f64>) = match side {
        Side::SellSol => ("bid", exchanges.iter().map(|q| q.bid).collect()),
        Side::BuySol => ("ask", exchanges.iter().map(|q| q.ask).collect()),
    };
    let Some(reference) = median(prices.into_iter().filter(|p| p.is_finite() && *p > 0.0)) else {
        return Err(format!(
            "no price to check Jupiter's quote against: {} of the exchanges quote SOL right now, \
             {MIN_EXCHANGES} are needed",
            exchanges.len()
        ));
    };
    if !quoted_px.is_finite() || quoted_px <= 0.0 {
        return Err(format!("Jupiter quoted {quoted_px} USDT per SOL"));
    }
    // Positive when the quote is worse for the wallet.
    let gap_bps = match side {
        Side::SellSol => (reference - quoted_px) / reference * 1e4,
        Side::BuySol => (quoted_px - reference) / reference * 1e4,
    };
    if gap_bps > f64::from(max_gap_bps) {
        let dir = match side {
            Side::SellSol => "below",
            Side::BuySol => "above",
        };
        return Err(format!(
            "Jupiter quoted {quoted_px:.4} USDT per SOL, {gap_bps:.0} bps {dir} the exchanges' \
             median {book_side} of {reference:.4} (limit {max_gap_bps} bps)"
        ));
    }
    Ok(())
}

/// `None` below [`MIN_EXCHANGES`] values.
fn median(values: impl Iterator<Item = f64>) -> Option<f64> {
    let mut v: Vec<f64> = values.collect();
    if v.len() < MIN_EXCHANGES {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Venue;

    fn book(quotes: &[(f64, f64)]) -> Vec<ExchangeQuote> {
        quotes
            .iter()
            .zip(Venue::ALL)
            .map(|(&(bid, ask), venue)| ExchangeQuote { venue, bid, ask })
            .collect()
    }

    #[test]
    fn a_quote_close_to_the_exchanges_passes() {
        let b = book(&[(150.00, 150.02), (149.98, 150.03), (150.01, 150.04)]);
        assert!(check_quote(Side::SellSol, 149.90, Some(&b), 100).is_ok());
        assert!(check_quote(Side::BuySol, 150.20, Some(&b), 100).is_ok());
    }

    #[test]
    fn a_quote_worse_than_the_limit_is_refused() {
        let b = book(&[(150.00, 150.02), (149.98, 150.03), (150.01, 150.04)]);
        // 2% under the median bid of 150.00.
        let why = check_quote(Side::SellSol, 147.00, Some(&b), 100).unwrap_err();
        assert!(why.contains("200 bps below"), "{why}");
        assert!(why.contains("median bid of 150.0000"), "{why}");
        // 2% over the median ask of 150.03.
        let why = check_quote(Side::BuySol, 153.03, Some(&b), 100).unwrap_err();
        assert!(why.contains("above"), "{why}");
    }

    #[test]
    fn a_quote_better_for_the_wallet_passes() {
        let b = book(&[(150.00, 150.02), (149.98, 150.03)]);
        assert!(check_quote(Side::SellSol, 160.0, Some(&b), 100).is_ok());
        assert!(check_quote(Side::BuySol, 140.0, Some(&b), 100).is_ok());
    }

    #[test]
    fn one_exchange_off_on_its_own_does_not_move_the_reference() {
        // A venue printing 10% low would otherwise wave a bad sale through.
        let b = book(&[(150.00, 150.02), (135.00, 135.02), (150.01, 150.04)]);
        assert!(check_quote(Side::SellSol, 136.0, Some(&b), 100).is_err());
    }

    #[test]
    fn without_enough_exchanges_nothing_is_quoted() {
        let one = book(&[(150.00, 150.02)]);
        let why = check_quote(Side::SellSol, 150.0, Some(&one), 100).unwrap_err();
        assert!(why.contains("1 of the exchanges"), "{why}");
        assert!(check_quote(Side::SellSol, 150.0, Some(&[]), 100).is_err());
        // Zero or NaN prices don't count toward the two.
        let bad = book(&[(150.00, 150.02), (0.0, f64::NAN)]);
        assert!(check_quote(Side::SellSol, 150.0, Some(&bad), 100).is_err());
        let why = check_quote(Side::BuySol, 150.0, None, 100).unwrap_err();
        assert!(why.contains("not followed"), "{why}");
    }

    #[test]
    fn a_nonsense_quote_is_refused() {
        let b = book(&[(150.00, 150.02), (149.98, 150.03)]);
        assert!(check_quote(Side::BuySol, f64::NAN, Some(&b), 100).is_err());
        assert!(check_quote(Side::BuySol, 0.0, Some(&b), 100).is_err());
    }
}
