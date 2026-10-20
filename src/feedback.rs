//! The way back to mayorana.ch from inside the app.
//!
//! People get SpreadRadar as a download and may never see the site again: the
//! update banner and the notice only show when there is something to say. The
//! Feedback button is always in the tab bar, so anyone with an idea or a
//! problem has somewhere to send it.

/// The contact form with the topic set and the app and version filled in, so a
/// message says what it is about without the sender having to.
pub fn contact_url() -> String {
    format!(
        "https://mayorana.ch/en/contact?service=tools&app=SpreadRadar&v={}",
        env!("CARGO_PKG_VERSION")
    )
}
