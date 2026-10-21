//! The SpreadRadar Pro window: what the free version does, the licence on this
//! computer, and where to paste a key.

use dioxus::prelude::*;

use crate::licence::{self, FreeError, Status, FREE_ASSETS};

/// Shared by every part of the UI that can open the window or needs to know
/// whether the app is licensed. Provided once, by `App`.
#[derive(Clone, Copy)]
pub struct Pro {
    pub open: Signal<bool>,
    pub status: Signal<Status>,
}

/// "Get Pro…" while free, "✦ PRO" once licensed; opens the window either way.
#[component]
pub fn ProButton() -> Element {
    let mut pro = use_context::<Pro>();
    let status = pro.status.read().clone();
    let (label, class, title) = match &status {
        Status::Pro(a) => (
            "✦ PRO",
            "pro-btn active",
            format!("SpreadRadar Pro, {} plan for {}", a.plan.name, a.email),
        ),
        Status::NotPro(a) if a.is_free() => (
            "Get Pro…",
            "pro-btn",
            format!("Free: {} AI credits left this month. The free version follows {FREE_ASSETS} assets; Pro follows any number", a.credits.left),
        ),
        Status::NotPro(_) | Status::Offline(_) | Status::Unknown(_) => (
            "✦ RENEW PRO",
            "pro-btn renew",
            "Your licence doesn't make this SpreadRadar Pro right now".to_owned(),
        ),
        Status::Free => (
            "Get Pro…",
            "pro-btn",
            format!("The free version follows {FREE_ASSETS} assets; Pro follows any number"),
        ),
    };
    rsx! {
        button { class, title, onclick: move |_| pro.open.set(true), "{label}" }
    }
}

#[component]
pub fn ProWindow() -> Element {
    let mut pro = use_context::<Pro>();
    let mut key = use_signal(String::new);
    let mut error = use_signal(|| Option::<String>::None);
    let mut busy = use_signal(|| false);
    let mut free_email = use_signal(String::new);
    let mut free_note = use_signal(|| Option::<String>::None);
    let mut free_hidden = use_signal(|| false);
    if !(pro.open)() {
        return rsx! {};
    }
    let status = pro.status.read().clone();
    let asks_number =
        matches!(status, Status::Free | Status::Unknown(_)) || status.free_credits().is_some();
    let offers_free = matches!(status, Status::Free | Status::Unknown(_)) && !free_hidden();
    let mut close = move || {
        pro.open.set(false);
        error.set(None);
    };
    let mut activate = move || {
        let pasted = key.read().clone();
        busy.set(true);
        spawn(async move {
            match licence::activate(&pasted).await {
                Ok(s) => {
                    pro.status.set(s);
                    key.set(String::new());
                    error.set(None);
                }
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };
    let mut check_again = move || {
        busy.set(true);
        spawn(async move {
            pro.status.set(licence::refresh(true).await);
            busy.set(false);
        });
    };
    let mut ask_free = move || {
        let email = free_email.read().clone();
        busy.set(true);
        spawn(async move {
            match licence::request_free_credits(&email).await {
                Ok(message) => {
                    free_note.set(Some(format!(
                        "{message} Paste the licence number above when it arrives."
                    )));
                    error.set(None);
                }
                Err(FreeError::Unavailable) => free_hidden.set(true),
                Err(FreeError::Refused(e)) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };
    let mut manage = move |number: String| {
        busy.set(true);
        spawn(async move {
            match licence::portal_url(&number).await {
                Ok(url) => {
                    let _ = open::that(url);
                    error.set(None);
                }
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };

    rsx! {
        div { class: "modal-backdrop", onclick: move |_| close(),
            div {
                class: "modal card",
                // Clicks inside the window don't close it.
                onclick: move |e| e.stop_propagation(),
                h2 { "SpreadRadar Pro" }
                match &status {
                    Status::Free => rsx! {
                        p {
                            "The free version follows {FREE_ASSETS} assets at once, with every tab, every venue and trading. "
                            "Pro follows any number of assets, and is the licence for commercial use. "
                            "It comes with a monthly plan on mayorana.ch."
                        }
                    },
                    Status::Pro(a) => rsx! {
                        p { class: "pro-ok",
                            "Pro is active on this computer: "
                            strong { "{a.plan.name}" }
                            " plan for "
                            strong { "{a.email}" }
                            "."
                        }
                        p { class: "small",
                            "{a.credits.left} AI credits left this month. {a.renewal()}."
                        }
                    },
                    Status::NotPro(a) if a.is_free() => rsx! {
                        p {
                            "Free — "
                            strong { "{a.credits.left} AI credits" }
                            " left this month, for "
                            strong { "{a.email}" }
                            ". {a.renewal()}."
                        }
                        p {
                            "The free version follows {FREE_ASSETS} assets at once. Pro follows any number of assets, and is the licence for commercial use. "
                            "It comes with a monthly plan on mayorana.ch."
                        }
                    },
                    Status::NotPro(a) if !a.active => rsx! {
                        p {
                            "The subscription for "
                            strong { "{a.email}" }
                            " isn't active, so SpreadRadar runs as the free version. Renew it to have Pro again."
                        }
                    },
                    Status::NotPro(a) => rsx! {
                        p {
                            "The "
                            strong { "{a.plan.name}" }
                            " plan doesn't include SpreadRadar, so it runs as the free version. A plan with SpreadRadar makes it Pro."
                        }
                    },
                    Status::Offline(a) => rsx! {
                        p {
                            "SpreadRadar hasn't been able to check the licence for "
                            strong { "{a.email}" }
                            " for a week, so it runs as the free version until it can. Check the connection, then check again."
                        }
                    },
                    Status::Unknown(_) => rsx! {
                        p { "The licence number on this computer isn't known. Check it against the email, or paste it again." }
                    },
                }
                if !matches!(status, Status::Pro(_)) {
                    button {
                        class: "btn buy",
                        onclick: move |_| {
                            let _ = open::that(licence::BUY_URL);
                        },
                        if asks_number { "Buy SpreadRadar Pro" } else { "Renew SpreadRadar Pro" }
                    }
                }
                if asks_number {
                    p { class: "muted small", "Already subscribed? Paste the licence number from your email:" }
                    input {
                        class: "licence-key",
                        placeholder: "ABCDE-FGHJK-LMNPQ-RSTUV",
                        value: "{key}",
                        oninput: move |e| key.set(e.value()),
                    }
                    p { class: "muted small",
                        "The number is checked with api0 now and once a day. Pro keeps working for a week without a connection."
                    }
                }
                if offers_free {
                    div { class: "free-credits",
                        p { class: "muted small", "No plan? Get free AI credits every month, without Pro. The licence number comes by email." }
                        div { class: "free-row",
                            input {
                                class: "licence-key",
                                r#type: "email",
                                placeholder: "you@example.com",
                                value: "{free_email}",
                                oninput: move |e| free_email.set(e.value()),
                            }
                            button {
                                class: "btn",
                                disabled: free_email.read().trim().is_empty() || busy(),
                                onclick: move |_| ask_free(),
                                "Get free AI credits"
                            }
                        }
                        if let Some(n) = free_note.read().as_ref() {
                            p { class: "pro-ok small", "{n}" }
                        }
                    }
                }
                if let Some(e) = error.read().as_ref() {
                    p { class: "error small", "{e}" }
                }
                div { class: "modal-actions",
                    if asks_number {
                        button {
                            class: "btn",
                            disabled: key.read().trim().is_empty() || busy(),
                            onclick: move |_| activate(),
                            "Activate"
                        }
                    }
                    if let (Some(number), false) = (status.licence().map(str::to_owned), asks_number) {
                        button {
                            class: "btn",
                            disabled: busy(),
                            title: "Change or cancel the plan on mayorana.ch",
                            onclick: move |_| manage(number.clone()),
                            "Manage subscription"
                        }
                        button {
                            class: "btn ghost",
                            disabled: busy(),
                            onclick: move |_| check_again(),
                            "Check again"
                        }
                    }
                    if status.licence().is_some() {
                        button {
                            class: "btn ghost",
                            title: "To move the licence to another computer",
                            onclick: move |_| {
                                licence::deactivate();
                                pro.status.set(licence::current());
                            },
                            "Remove the licence from this computer"
                        }
                    }
                    button { class: "btn ghost", onclick: move |_| close(), "Close" }
                }
            }
        }
    }
}
