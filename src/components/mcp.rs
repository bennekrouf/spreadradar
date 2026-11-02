//! How to use SpreadRadar from Claude: what to paste, and where.
//!
//! The command is this app's own executable, read at runtime rather than
//! written down, so the snippet is right wherever it was installed — and in a
//! development build, which is what someone trying it from a checkout runs.

use dioxus::prelude::*;

/// Whether the window is open. Provided once, by `App`.
#[derive(Clone, Copy)]
pub struct McpOpen(pub Signal<bool>);

#[component]
pub fn McpWindow() -> Element {
    let mut open = use_context::<McpOpen>().0;
    if !open() {
        return rsx! {};
    }
    rsx! {
        desktop_kit::ui::ConnectWindow {
            lead: "Ask Claude about the market: where the widest edges are right now, which venue lags, what Gate and \
                   MEXC just listed that Binance doesn't have. It can add and remove assets in your watchlist too, \
                   and this window follows. Trading stays here, in the app.",
            try_it: "\u{201c}Where is SOL cheapest to buy right now, after fees?\u{201d}, or \u{201c}What did Gate list this \
                     week that looks serious?\u{201d}",
            on_close: move |_| open.set(false),
        }
    }
}
