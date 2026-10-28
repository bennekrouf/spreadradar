//! What SpreadRadar Pro adds. The window, the badge and the licence checks
//! are desktop-kit's, the same in every mayorana app.

use dioxus::prelude::*;

use crate::licence::FREE_ASSETS;

/// The badge's tooltip while SpreadRadar isn't Pro.
pub fn hint() -> String {
    format!("The free version follows {FREE_ASSETS} assets; Pro follows any number")
}

/// What the free version does and what Pro adds, in the Pro window.
pub fn pitch() -> Element {
    rsx! {
        p {
            "The free version follows {FREE_ASSETS} assets at once, with every tab, every venue and trading. "
            "Pro follows any number of assets, and is the licence for commercial use. "
            "It comes with a monthly plan on mayorana.ch."
        }
    }
}
