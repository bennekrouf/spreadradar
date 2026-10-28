//! SpreadRadar Pro: what the free version is limited to.
//!
//! The licence itself (the number, api0's answer, the daily check, the shared
//! file) is `desktop_kit::licence`, the same in every mayorana app; SpreadRadar
//! names its product id and where it keeps api0's last answer in `kit`. The
//! MCP server reads that answer too, so it applies the same limit.
//!
//! Without Pro, SpreadRadar follows [`FREE_ASSETS`] assets at once. The engine
//! applies the limit ([`spread_core::MarketCmd::SetLimit`]); this module only
//! says which status the app is in.

pub use desktop_kit::licence::{buy_url, current, title_badge, Status};

/// The product id api0 knows SpreadRadar by: a plan lists it to make
/// SpreadRadar Pro. The app was called Spreadwatch when plans were first set
/// up, so the id stays.
pub const PRODUCT: &str = "spreadwatch";

/// How many assets the free version follows at once.
pub const FREE_ASSETS: usize = 3;

/// What SpreadRadar makes of a licence status.
pub trait StatusExt {
    /// The watchlist limit to hand the engine.
    fn asset_limit(&self) -> Option<usize>;
}

impl StatusExt for Status {
    fn asset_limit(&self) -> Option<usize> {
        (!self.is_pro()).then_some(FREE_ASSETS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desktop_kit::licence::{status_at, Answer, Saved};

    const DAY: u64 = 24 * 3600;

    fn saved(products: &[&str], active: bool, checked: u64) -> Saved {
        let answer = Answer {
            licence: "ABCDE-FGHJK-LMNPQ-RSTUV".into(),
            products: products.iter().map(|p| p.to_string()).collect(),
            active,
            ..Answer::default()
        };
        Saved {
            licence: answer.licence.clone(),
            answer: Some(answer),
            checked,
        }
    }

    #[test]
    fn only_the_free_version_is_limited() {
        assert_eq!(Status::Free.asset_limit(), Some(FREE_ASSETS));
        let now = 1_000 * DAY;
        let pro = status_at(Some(&saved(&[PRODUCT], true, now)), now, PRODUCT);
        assert_eq!(pro.asset_limit(), None);
        let lapsed = status_at(Some(&saved(&[PRODUCT], false, now)), now, PRODUCT);
        assert_eq!(lapsed.asset_limit(), Some(FREE_ASSETS));
    }
}
