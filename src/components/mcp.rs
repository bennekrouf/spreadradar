//! How to use SpreadRadar from Claude: what to paste, and where.
//!
//! The command is this app's own executable, read at runtime rather than
//! written down, so the snippet is right wherever it was installed — and in a
//! development build, which is what someone trying it from a checkout runs.

use dioxus::prelude::*;

/// Whether the window is open. Provided once, by `App`.
#[derive(Clone, Copy)]
pub struct McpOpen(pub Signal<bool>);

/// This executable, as a client should start it.
fn command() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "spreadradar".into())
}

/// The `mcpServers` entry for Claude Desktop's config file. Built with serde
/// so a Windows path's backslashes arrive escaped.
pub fn desktop_config(command: &str) -> String {
    let config = serde_json::json!({
        "mcpServers": { "spreadradar": { "command": command, "args": ["mcp"] } }
    });
    serde_json::to_string_pretty(&config).unwrap_or_default()
}

/// The one line that adds it to Claude Code, for every project.
pub fn code_command(command: &str) -> String {
    format!("claude mcp add spreadradar --scope user -- \"{command}\" mcp")
}

/// Where Claude Desktop keeps its config.
fn desktop_config_path() -> &'static str {
    if cfg!(target_os = "windows") {
        "%APPDATA%\\Claude\\claude_desktop_config.json"
    } else if cfg!(target_os = "macos") {
        "~/Library/Application Support/Claude/claude_desktop_config.json"
    } else {
        "~/.config/Claude/claude_desktop_config.json"
    }
}

#[component]
pub fn McpButton() -> Element {
    let mut open = use_context::<McpOpen>().0;
    rsx! {
        button {
            class: "btn ghost mcp-btn",
            title: "Use SpreadRadar from Claude Desktop or Claude Code",
            onclick: move |_| open.set(true),
            "MCP"
        }
    }
}

#[component]
pub fn McpWindow() -> Element {
    let mut open = use_context::<McpOpen>().0;
    if !open() {
        return rsx! {};
    }
    let command = command();
    let desktop = desktop_config(&command);
    let code = code_command(&command);
    rsx! {
        div { class: "modal-backdrop", onclick: move |_| open.set(false),
            div {
                class: "modal card mcp-modal",
                onclick: move |e| e.stop_propagation(),
                div { class: "modal-head",
                    h2 { "Use SpreadRadar from Claude" }
                    button { class: "modal-close", title: "Close", onclick: move |_| open.set(false), "×" }
                }
                div { class: "modal-body",
                    p { class: "muted small",
                        "Ask Claude about the market: where the widest edges are right now, which venue lags, \
                         what Gate and MEXC just listed that Binance doesn't have. It can add and remove assets \
                         in your watchlist too, and this window follows. Trading stays here, in the app."
                    }
                    h3 { "1 · Claude Desktop" }
                    p { class: "muted small",
                        "Settings \u{2192} Developer \u{2192} Edit Config, paste this in and save, then quit Claude \
                         and open it again. If the file already has an mcpServers section, add just the \
                         spreadradar entry to it."
                    }
                    Snippet { text: desktop.clone() }
                    p { class: "muted small mcp-path", "The file: {desktop_config_path()}" }
                    h3 { "2 · Claude Code" }
                    p { class: "muted small", "Run this once in a terminal:" }
                    Snippet { text: code.clone() }
                    h3 { "3 · Try it" }
                    p { class: "muted small",
                        "Ask: \u{201c}Where is SOL cheapest to buy right now, after fees?\u{201d}, or \u{201c}What did \
                         Gate list this week that looks serious?\u{201d}"
                    }
                    p { class: "muted small mcp-note",
                        "Claude in the browser (claude.ai) can\u{2019}t use this: it only connects to servers on the \
                         internet, and SpreadRadar runs on your computer. Any other app that speaks MCP can, with \
                         the same command: "
                        code { "{command} mcp" }
                    }
                    div { class: "modal-actions",
                        button { class: "btn ghost", onclick: move |_| open.set(false), "Close" }
                    }
                }
            }
        }
    }
}

/// A block of text to paste somewhere else, with its own Copy button.
#[component]
fn Snippet(text: String) -> Element {
    let mut copied = use_signal(|| false);
    let to_copy = text.clone();
    rsx! {
        div { class: "mcp-snippet-wrap",
            pre { class: "mcp-snippet", "{text}" }
            button {
                class: "btn ghost mcp-copy",
                onclick: move |_| {
                    // The system clipboard, as the scanner's Copy does.
                    let ok = arboard::Clipboard::new().and_then(|mut c| c.set_text(to_copy.clone()));
                    if let Err(e) = &ok {
                        tracing::warn!("copy failed: {e}");
                    }
                    copied.set(ok.is_ok());
                },
                if copied() { "Copied ✓" } else { "Copy" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_desktop_entry_starts_this_app_in_mcp_mode() {
        let config: serde_json::Value = serde_json::from_str(&desktop_config(
            "/Applications/SpreadRadar.app/Contents/MacOS/spreadradar",
        ))
        .unwrap();
        let entry = &config["mcpServers"]["spreadradar"];
        assert_eq!(
            entry["command"],
            "/Applications/SpreadRadar.app/Contents/MacOS/spreadradar"
        );
        assert_eq!(entry["args"], serde_json::json!(["mcp"]));
        let path = r"C:\Program Files\SpreadRadar\spreadradar.exe";
        let config: serde_json::Value = serde_json::from_str(&desktop_config(path)).unwrap();
        assert_eq!(config["mcpServers"]["spreadradar"]["command"], path);
        assert_eq!(
            code_command("/A B/spreadradar"),
            "claude mcp add spreadradar --scope user -- \"/A B/spreadradar\" mcp"
        );
    }
}
