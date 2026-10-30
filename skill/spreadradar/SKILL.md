---
name: spreadradar
description: Read crypto market data from SpreadRadar, the desktop app that follows the same asset on Binance, Bybit, OKX, MEXC, Gate and Jupiter (Solana) at once, through its MCP tools. Use this whenever the user mentions SpreadRadar, or asks how a token's price differs between exchanges, which venue quotes it best or moves first, what a cross-exchange spread looks like before and after fees, which tokens Gate or MEXC listed recently that Binance doesn't have, or wants to add or remove an asset from their watchlist. Also use it when SpreadRadar's tools are missing and the user wants to connect the app. Read-only market data; it does not trade and is not financial advice.
---

# SpreadRadar

SpreadRadar is a desktop app (macOS, Windows, Linux) that watches the same
crypto asset across five exchanges and Solana on-chain (Jupiter) at once, all
quoted in USDT. It shows where the price differs, which venue moves first,
and which tokens were listed recently on Gate or MEXC but not on Binance spot.
All prices come from public market data.

The person you are helping follows crypto markets and wants a clear,
factual read of what the venues show.

## Before you start

The tools come from SpreadRadar's own MCP server (`spreadradar mcp`, over
stdio). If no tool named `watchlist` from a `spreadradar` server is
available, the app is not connected:

- Tell the user SpreadRadar is a separate desktop app, from
  <https://mayorana.ch/en/apps/spreadradar>, and that this skill only works
  with it installed. Do not download or install it for them.
- Once installed, the **MCP** button next to *Get Pro…* shows the exact
  config for Claude Desktop and the command for Claude Code and Codex.
  `references/connect.md` has the steps per client.

## How to work with it

- **`watchlist`**: every followed asset with its price, its best cross-venue
  edge and how many venues quote it. Start here.
- **`asset`**: one asset in detail: each venue's bid and ask, every
  buy-here, sell-there route in basis points before and after taker fees,
  and which venue's price lags the others. Lead and lag need price moves to
  measure, so pass `watch_secs` (for example 30 to 120) to observe longer;
  on a quiet market, say the reading is weak rather than overstating it.
- **`new_listings`**: tokens Gate or MEXC listed recently that Binance spot
  doesn't, scored by the scanner.
- **`add_asset`** and **`remove_asset`** change the watchlist the app shows
  too. Ask before removing one. The free version follows 3 assets; past that
  the extra ones are kept but paused until there is room or a SpreadRadar Pro
  licence.

When you report, give numbers with their units (price in USDT, edges in
basis points), name the venues, and say how old the data is if the tool
reports it.

## How to talk about what you see

- **An edge on screen is not a profit.** Moving funds between venues takes
  time and has withdrawal and network fees; slippage on thin order books and
  the price moving in the meantime eat into it. Say so whenever you mention
  an edge, and put the after-fee figure first.
- A large gap on a single venue often means that venue is stale, halted for
  deposits or withdrawals, or quoting a different token with the same ticker.
  Point that out instead of presenting it as an opportunity.
- New listings are high-risk: low liquidity, wide spreads, sharp swings.
  The scanner's score ranks what it found; it is not a recommendation.
- **Never present any of this as financial advice**, and never tell the user
  to buy, sell or move money. Describe what the market data shows and let
  them decide.

## Not available here

Trading is not exposed through these tools. SpreadRadar's on-chain swaps stay
in the app, behind its own dry run and limits. If the user wants to trade,
tell them it is done in the SpreadRadar window, by them.
