//! SpreadRadar Pro: the licence on this computer.
//!
//! Pro comes with a monthly plan bought on mayorana.ch. The plan's licence
//! number (`ABCDE-FGHJK-LMNPQ-RSTUV`, emailed when it is bought) is saved here
//! and checked with api0 when SpreadRadar starts and then once a day:
//!
//! ```text
//! GET https://gateway.api0.ai/v1/licence
//! Authorization: Bearer <licence>
//! X-Client-App: spreadwatch
//! ```
//!
//! The answer names the plan, the apps it makes Pro, whether it is paid up and
//! the AI credits left. It is kept on disk, so the MCP server and a SpreadRadar
//! started offline read the last one: Pro lasts a week
//! (`desktop_kit::licence::GRACE`) after the last check that got through, then
//! waits for the next.
//!
//! Without Pro, SpreadRadar follows [`FREE_ASSETS`] assets at once. The engine
//! applies the limit ([`spread_core::MarketCmd::SetLimit`]); this module only
//! says which status the app is in.
//!
//! What every app agrees on — the number's format, api0's answer, the grace
//! period, the shared file — is `desktop_kit::licence`. Here: the requests,
//! where SpreadRadar keeps its own answer, and its free version's limit.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use desktop_kit::licence::{
    check_due, free_reply, load_shared, normalise, plausible_email, reconcile, Shared, GATEWAY,
};
pub use desktop_kit::licence::{Answer, FreeError, Saved, Status};
use serde::Deserialize;

/// The product id api0 knows SpreadRadar by: a plan lists it to make
/// SpreadRadar Pro. The app was called Spreadwatch when plans were first set
/// up, so the id stays.
pub const PRODUCT: &str = "spreadwatch";

/// Where "Buy SpreadRadar Pro" leads.
pub const BUY_URL: &str = "https://mayorana.ch/en/apps/spreadradar";

const LICENCE_FILE: &str = "licence.json";

/// How many assets the free version follows at once.
pub const FREE_ASSETS: usize = 3;

/// What `saved` amounts to for SpreadRadar at `now` (seconds since 1970).
pub fn status_at(saved: Option<&Saved>, now: u64) -> Status {
    desktop_kit::licence::status_at(saved, now, PRODUCT)
}

/// What SpreadRadar makes of a licence status.
pub trait StatusExt {
    /// What the window title adds after "SpreadRadar <version>".
    fn title_badge(&self) -> &'static str;
    /// The watchlist limit to hand the engine.
    fn asset_limit(&self) -> Option<usize>;
}

impl StatusExt for Status {
    fn title_badge(&self) -> &'static str {
        self.title_badge_for(PRODUCT)
    }

    fn asset_limit(&self) -> Option<usize> {
        (!self.is_pro()).then_some(FREE_ASSETS)
    }
}

/// Writes the shared licence file.
fn save_shared(text: &str) {
    desktop_kit::licence::save_shared(text, PRODUCT);
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn dir() -> PathBuf {
    spread_core::trade::config_dir()
}

fn licence_path() -> PathBuf {
    dir().join(LICENCE_FILE)
}

fn load_own() -> Option<Saved> {
    let text = std::fs::read_to_string(licence_path()).ok()?;
    serde_json::from_str(&text).ok()
}

fn load() -> Option<Saved> {
    let shared = load_shared();
    if shared == Shared::Removed {
        let _ = std::fs::remove_file(licence_path());
    }
    let (saved, share) = reconcile(shared, load_own());
    if share {
        if let Some(saved) = &saved {
            save_shared(&saved.licence);
        }
    }
    saved
}

fn save(saved: &Saved) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(saved).map_err(|e| e.to_string())?;
    let path = licence_path();
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(dir())?;
        // Written beside the target and renamed over it, so a crash can't leave
        // half a file behind.
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, &json)?;
        std::fs::rename(&tmp, &path)
    };
    write().map_err(|e| format!("The licence couldn't be saved: {e}"))
}

/// The licence as of the last check, without asking api0.
pub fn current() -> Status {
    status_at(load().as_ref(), now())
}

/// Why a check didn't give an answer.
#[derive(Debug)]
enum CheckError {
    /// api0 doesn't know this licence number.
    Unknown,
    /// Offline, api0 down, or an answer that made no sense.
    Unreachable(String),
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap_or_default()
}

async fn ask(licence: &str) -> Result<Answer, CheckError> {
    let resp = client()
        .get(format!("{GATEWAY}/v1/licence"))
        .bearer_auth(licence)
        .header("X-Client-App", PRODUCT)
        .send()
        .await
        .map_err(|e| CheckError::Unreachable(e.to_string()))?;
    match resp.status().as_u16() {
        200 => resp
            .json::<Answer>()
            .await
            .map_err(|e| CheckError::Unreachable(e.to_string())),
        401 => Err(CheckError::Unknown),
        code => Err(CheckError::Unreachable(format!("api0 answered {code}"))),
    }
}

/// Checks `typed` with api0 and, if api0 knows it, saves it.
pub async fn activate(typed: &str) -> Result<Status, String> {
    let licence = normalise(typed).ok_or(
        "This isn't a licence number. It has 20 letters and digits, like ABCDE-FGHJK-LMNPQ-RSTUV.",
    )?;
    let answer = match ask(&licence).await {
        Ok(a) => a,
        Err(CheckError::Unknown) => {
            return Err("This licence number isn't known. Check it against the email.".into())
        }
        Err(CheckError::Unreachable(e)) => {
            return Err(format!(
                "The licence couldn't be checked ({e}). Check the connection and try again."
            ))
        }
    };
    let saved = Saved {
        licence,
        answer: Some(answer),
        checked: now(),
    };
    save(&saved)?;
    save_shared(&saved.licence);
    Ok(status_at(Some(&saved), now()))
}

/// Checks the saved licence with api0 again, when `force`d or a day after the
/// last check, and keeps the answer. Offline, the last answer stands.
pub async fn refresh(force: bool) -> Status {
    let Some(mut saved) = load() else {
        return Status::Free;
    };
    if !force && !check_due(&saved, now()) {
        return status_at(Some(&saved), now());
    }
    match ask(&saved.licence).await {
        Ok(answer) => {
            saved.answer = Some(answer);
            saved.checked = now();
            let _ = save(&saved);
        }
        Err(CheckError::Unknown) => {
            saved.answer = None;
            let _ = save(&saved);
        }
        Err(CheckError::Unreachable(_)) => {}
    }
    status_at(Some(&saved), now())
}

/// The address of the page where the subscription is changed or cancelled.
pub async fn portal_url(licence: &str) -> Result<String, String> {
    #[derive(Deserialize)]
    struct Portal {
        url: String,
    }
    let resp = client()
        .post(format!("{GATEWAY}/v1/licence/portal"))
        .bearer_auth(licence)
        .header("X-Client-App", PRODUCT)
        .send()
        .await
        .map_err(|e| format!("The subscription page couldn't be opened ({e})."))?;
    if !resp.status().is_success() {
        return Err(format!(
            "The subscription page couldn't be opened (api0 answered {}).",
            resp.status().as_u16()
        ));
    }
    let portal: Portal = resp
        .json()
        .await
        .map_err(|e| format!("The subscription page couldn't be opened ({e})."))?;
    Ok(portal.url)
}

/// Asks api0 to email `email` a licence for the free allowance of AI credits.
/// The licence number itself only goes by email.
pub async fn request_free_credits(email: &str) -> Result<String, FreeError> {
    if !plausible_email(email) {
        return Err(FreeError::Refused(
            "Enter the email the licence should go to.".into(),
        ));
    }
    let resp = client()
        .post(format!("{GATEWAY}/api/plans/free"))
        .header("X-Client-App", PRODUCT)
        .json(&serde_json::json!({ "email": email.trim() }))
        .send()
        .await
        .map_err(|e| {
            FreeError::Refused(format!("The free AI credits couldn't be requested ({e})."))
        })?;
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();
    free_reply(status, &body)
}

/// Removes the licence from this computer (to move it to another one), for
/// every Mayorana app on it.
pub fn deactivate() {
    let _ = std::fs::remove_file(licence_path());
    save_shared("");
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let pro = status_at(Some(&saved(&["spreadwatch"], true, now)), now);
        assert_eq!(pro.asset_limit(), None);
        let lapsed = status_at(Some(&saved(&["spreadwatch"], false, now)), now);
        assert_eq!(lapsed.asset_limit(), Some(FREE_ASSETS));
    }
}
