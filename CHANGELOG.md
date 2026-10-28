# Changelog

What changed in each release of **SpreadRadar**, the desktop app that watches
crypto spreads across exchanges and Solana on-chain.

The public version of this page — with the download for each release — lives at
<https://mayorana.ch/en/apps/spreadradar/releases>. It is generated from this
file by `scripts/changelog_to_json.py`, so this file is the only place a
release note is written.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Each heading is dated on the day its tag was pushed.

## [Unreleased]

### Changed

- The update banner and the occasional message from mayorana.ch now sit just
  under the bar at the top of the window instead of covering it, and look the
  same in every mayorana app. Download opens the new build in your browser and
  shows its SHA-256 when one is published, to compare with the file you
  downloaded.
- The Pro window looks and reads the same in every mayorana app. Once you're Pro
  it names your plan and email, and pressing Enter in the licence number field
  activates it.

## [0.1.28] - 2026-10-28

### Changed

- The bar across the top of the window now looks and works the same in every
  mayorana app: the name and version on the left, and on the right MCP,
  Feedback, the light/dark switch and the Pro badge in the corner.
- The market and New listings tabs are in the bar, and the bar and the watchlist
  stay in place while the market view scrolls.

## [0.1.27] - 2026-10-28

### Changed

- Packaging only — no user-visible change.

## [0.1.26] - 2026-10-27

### Added

- Added support for Codex: the MCP setup window now shows instructions for Codex
  alongside Claude Desktop and Claude Code.

## [0.1.25] - 2026-10-27

### Changed

- Packaging only — no user-visible change.

## [0.1.24] - 2026-10-25

### Fixed

- Installing silently, as IT does for an unattended update with `/VERYSILENT`,
  no longer hangs when SpreadRadar is already installed: setup waited on a
  question nobody could see. It now replaces the installed version without
  asking, and stops with an error, instead of waiting, when the Microsoft
  Edge WebView2 Runtime is missing.

## [0.1.23] - 2026-10-25

### Changed

- Activating a licence now shows a small spinner while it is checked, then
  a window that says you're Pro, with the AI credits left and the renewal
  date in small type below. Removing the licence and checking it again no
  longer sit beside it; Manage subscription is a small link. Press Esc or
  the × to close it.
- The new-version banner and the usage-statistics notice now look the same
  as in GitAgent: a blue bar across the top, or the bottom for the notice,
  with a Download link and a × to hide it.

## [0.1.22] - 2026-10-23

### Changed

- Dialogs now have a title bar with a × to close them, and text fields have
  the same size and typeface as in GitAgent.
- An open dialog now dims the light/dark switch in the corner along with the
  rest of the window, instead of leaving it lit on top.
- The buttons along the top of the window now all have the same height,
  borders included, and the light/dark switch matches them.

## [0.1.21] - 2026-10-23

### Changed

- Buttons now have the same size, rounded corners and typeface as in
  GitAgent, so they look alike across the Mayorana apps.

### Fixed

- The PRO button in the top-right corner is no longer hidden behind the
  light/dark switch.

## [0.1.20] - 2026-10-22

### Changed

- Packaging only — no user-visible change.

## [0.1.19] - 2026-10-22

### Changed

- The window title now shows your licence state next to the app name and
  version: a star and Pro when Pro is active, and Pro inactive when a Pro
  licence has lapsed or could not be checked. It updates as soon as the licence
  changes, so you can see your status without opening the Pro window.

## [0.1.18] - 2026-10-22

### Changed

- One licence number for every Mayorana app on this computer. Paste it in any
  of them and the others are Pro too the next time they start, with nothing to
  paste again; a number already pasted in one app is shared the same way.
  Removing the licence in the Pro window removes it from all of them.

## [0.1.17] - 2026-10-21

### Added

- Free AI credits every month, without Pro. Enter your email under Get free
  AI credits in the Pro window and the licence number arrives by email; paste
  it in the same window. The window then shows how many credits are left this
  month. The free version's limits stay as they are.

## [0.1.16] - 2026-10-21

### Changed

- SpreadRadar Pro now comes with a monthly plan on mayorana.ch instead of a
  one-time licence key. Paste the licence number from the plan's email (like
  ABCDE-FGHJK-LMNPQ-RSTUV) in the Pro window. SpreadRadar checks it when it
  starts and once a day, and Pro keeps working for a week without a
  connection.
- The Pro window shows your plan, the AI credits left this month and when the
  plan renews. Manage subscription opens the page where you change or cancel
  it.

## [0.1.15] - 2026-10-20

### Added

- A Feedback button next to MCP opens the contact form with SpreadRadar and
  your version already filled in, so an idea or a problem reaches us in two
  clicks.

## [0.1.13] - 2026-10-19

### Changed

- SpreadRadar now includes the necessary Windows C runtime libraries, ensuring
  it runs on any PC without requiring additional installations. This makes the
  app more portable and easier to use for new users.

## [0.1.12] - 2026-10-18

### Added

- Use SpreadRadar from Claude. The new **MCP** button next to Get Pro… shows
  what to paste into Claude Desktop, or the one line to run for Claude Code,
  already pointing at where SpreadRadar is installed. Claude can then read
  your watchlist's prices and best edges, look at one asset venue by venue
  (routes before and after fees, which venue lags), list what Gate and MEXC
  just listed that Binance doesn't have, and add or remove assets. An asset
  Claude adds or removes shows up in the open app within a couple of
  seconds. Trading stays in the app.

## [0.1.11] - 2026-10-16

### Added

- The New listings tab now shows each token's best edge between exchanges:
  buy on one, sell on another, after the taker fee on both legs. **By edge**
  puts the ones worth a look first, and the Detections log notes when an
  edge of 30 bps or more opens or closes. Exchanges with under $10k of
  volume are left out, since their best price is often a stale order.
- An edge that needs Gate is marked when Gate has closed deposits or
  withdrawals for the coin, so you don't chase a gap you can't move coins
  across. The other exchanges don't publish this, so check them yourself.
- Gaps over 20 % are marked "same token?": they almost always mean two
  different tokens sharing a ticker. Prices come from the scan every 5
  minutes; click **Watch** to follow a token's edge live on the Market tab.

## [0.1.10] - 2026-10-16

### Added

- Light mode. SpreadRadar now matches your computer's light or dark setting
  and follows it when it changes. The sun/moon button at the top right switches
  between the two.

### Fixed

- On Windows and Linux, the SpreadRadar window now shows its icon in the
  title bar, the taskbar and the app switcher, instead of a blank or generic
  one.

## [0.1.9] - 2026-10-14

### Changed

- Spreadwatch is now called SpreadRadar. Your Pro licence, hot wallet,
  watchlist and settings carry over: they stay in the same folder,
  `~/.config/spreadwatch` (`%LOCALAPPDATA%\Spreadwatch` on Windows). On macOS
  the update installs as SpreadRadar.app; once it runs, you can delete
  Spreadwatch.app from Applications.

## [0.1.7] - 2026-10-08

### Added

- SpreadRadar Pro. The free version follows 3 assets at once, with every tab,
  every venue and trading; Pro follows any number, and is the licence for
  commercial use. Buy it on [SpreadRadar's page](https://mayorana.ch/en/apps/spreadradar),
  then click **Get Pro…** at the right of the tab bar and paste the key. The
  key is checked on your computer: no account, nothing sent anywhere.
- If your watchlist already holds more than 3 assets, none is removed: the
  first 3 are followed and the others stay in the list marked 🔒 Pro. Remove
  one to make room, then click a locked one to follow it.

## [0.1.6] - 2026-10-07

### Changed

- The publisher information for SpreadRadar has been updated to 'Mayorana'. This
  change affects the metadata associated with the application, such as the
  installer and executable details.

## [0.1.5] - 2026-10-06

### Added

- SpreadRadar now shares anonymous usage statistics: whether it is installed and
  opened, its version and your operating system. It is on by default; a note at
  the bottom of the window tells you once, and nothing is sent before you have
  seen it. **Turn off** there stops it for good and deletes anything not yet
  sent. Your files, data and accounts are never part of it. It also stays off if
  `DO_NOT_TRACK`, `DISABLE_UPDATE_CHECK` or `MAYORANA_NO_TELEMETRY` is set.

## [0.1.4] - 2026-10-02

### Changed

- Packaging only — no user-visible change.

## [0.1.3] - 2026-09-27

### Added

- The version you are running now shows at the right end of the tab bar, so
  it is at hand when you report a problem or check whether an update
  installed.

## [0.1.2] - 2026-09-27

### Changed

- Packaging only — no user-visible change.

## [0.1.1] - 2026-09-27

### Added

- First public release, for macOS (Apple Silicon and Intel), Windows and Linux.
- Follow any asset by ticker and see it quoted against USDT on Binance, Bybit,
  OKX, MEXC and Gate, plus Jupiter for Solana tokens, with the best
  cross-venue edges net of fees and which venue's price trails the others.
- A New listings tab: tokens Gate or MEXC just listed that Binance spot does
  not, scored by volume, other listings, market cap and Binance's interest.
- SOL/USDT swaps through Jupiter from a hot wallet the app creates, with a dry
  run at every start and hard per-trade and daily limits.
- The app tells you when a newer version is available, with a link to it.

### Changed

- The app was called SOL Spread. Its settings folder, with your hot wallet,
  watchlist and `trade.toml`, moves from `~/.config/sol-spread` to
  `~/.config/spreadwatch` the first time you start it; nothing to do by hand.
  A `keypair_path` in `trade.toml` that still points into the old folder keeps
  working, and the trade panel asks you to update it.
