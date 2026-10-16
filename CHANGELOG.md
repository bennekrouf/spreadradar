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
