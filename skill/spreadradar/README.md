# SpreadRadar skill

A skill for AI agents that use **SpreadRadar**, the desktop app that follows the same crypto asset on Binance, Bybit, OKX, MEXC, Gate and Jupiter (Solana) at once. The
skill tells the agent how to use SpreadRadar's tools well: which to call first, what
needs your yes, sensible defaults, and how to report back.

## What you need

- **SpreadRadar**, installed from <https://mayorana.ch/en/apps/spreadradar> (macOS,
  Windows or Linux). This skill does not include the app.
- A desktop agent connected to SpreadRadar's MCP server: Claude Desktop, Claude
  Code, Codex CLI or Cursor. `references/connect.md` has the steps. Web-only
  agents such as ChatGPT can't reach an app on your computer.

## What you can ask for

- Show your watchlist with each asset's best cross-venue edge
- Compare one asset's bid and ask on every venue, before and after fees
- Watch which venue leads and which lags
- List tokens Gate or MEXC listed that Binance doesn't have
- Add or remove assets on the watchlist

## What waits for your yes

- Removing an asset from the watchlist

## Limits

Read-only market data: trading stays in the SpreadRadar window. The free version follows 3 assets. Nothing here is financial advice.

## Install

- **Claude Code**: unzip into `~/.claude/skills/` so the folder is
  `~/.claude/skills/spreadradar/`.
- **claude.ai or Claude Desktop**: upload the zip under Skills in Settings.
- **Other agents**: put the `spreadradar` folder where the agent reads skills.

## Help

Questions, licences and bug reports: <https://mayorana.ch/en/contact>.
