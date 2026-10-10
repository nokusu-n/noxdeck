# Developing Noxdeck

How to build, test and find your way around the source. What the program does
for its user is in the [README](../README.md).

## Prerequisites

Windows 10 or 11 (x64), WebView2, Node 24, Rust stable (MSVC) with the Visual Studio
C++ Build Tools. The interface tests also need `tauri-driver`
(`cargo install tauri-driver --locked`) and a `msedgedriver.exe` on `PATH` whose
version matches the installed WebView2 runtime.

```
npm install
```

## Commands

| What | Command |
|---|---|
| Run in development | `npm run tauri dev` |
| Full automated suite (core, interface, program) | `npm test` |
| Core tests (flows against a test launcher directory) | `npm run test:core` |
| Interface tests (the running app against the `test-launcher` executable) | `npm run e2e` |
| Program checks (window lifecycle, instance, memory, time to list) | `npm run test:program` |
| Build the installer, without installing it | `npm run build:installer` → `target/release/bundle/nsis/Noxdeck_0.6.0_x64-setup.exe` |
| Type-check the interface | `npm run typecheck` |

`npm run e2e` builds its own copy of the app and of `test-launcher` into
`target/e2e`, then runs every `e2e/*.spec.mjs`. `E2E_SKIP_BUILD=1` reuses the last
build; `E2E_ONLY=<part of a file name>` runs matching specs, and
`E2E_TEST_NAME=<regular expression>` selects test names within them.
`PROGRAM_ONLY=<regular expression>` selects program checks. The foreground-focus
check runs last; leave the desktop untouched for that check.

No automated command starts, stops or edits the real Battle.net launcher.
The rating service is also replaced: the harness starts a local HTTP stub and
sets `NOXDECK_OVERFAST_URL`. No automated test opens the repository in a browser;
the core test records the URL handed to the shell.
The test commands set isolated `NOXDECK_INSTANCE`, `NOXDECK_AUTOSTART_NAME` and
`NOXDECK_LOCALE`, use directories inside `e2e/.tmp`, and delete their test startup
values and files. `npm test` excludes installation/uninstallation: the legacy
`e2e:installed` entry point is disabled because the uninstall hook affects the
real startup value. Installation and sign-in checks are manual.

The publisher is Nokusu.dev. The installer remembers a custom install folder
under `Software\Nokusu.dev\Noxdeck`, so changing the publisher or the product name
makes a new version forget where the old one was installed. Do not change either.

The interface suite can run without a window on screen: build the test app with
`TAURI_CONFIG` set to the window block of `tauri.conf.json` plus
`"visible": false, "focus": false, "skipTaskbar": true`, then `npm run e2e`.

In a development build, `npm run dev -- --host 127.0.0.1` and
`http://127.0.0.1:1420/#states` show every state of the window at once from fixed
data, without the core.

For a direct cargo check, set the same isolation explicitly:

```powershell
$env:NOXDECK_INSTANCE = "core-$([guid]::NewGuid().ToString('N'))"
$env:NOXDECK_AUTOSTART_NAME = "NoxDeck-Test-$([guid]::NewGuid().ToString('N'))"
$env:NOXDECK_LOCALE = "en-US"
$env:NOXDECK_OVERFAST_URL = "http://127.0.0.1:9" # local closed port; service tests own a local stub
cargo test --workspace
```

## Where things are

- `src-tauri/src/launcher/` — everything known about Battle.net (files, log lines,
  process names). The rest of the code speaks in typed events.
- `src-tauri/src/` — the core: `watcher.rs` (derives launcher state), `operations.rs`
  (add, save current, start launcher, start game), `registry.rs` and `settings.rs` (Noxdeck's own files).
- `src-tauri/src/game_state.rs` — the observed account and game process ID,
  saved in Noxdeck's own `state.json`; no login credentials or session data.
- `src/` — the React interface; `src/core.ts` is the typed boundary to the core.
- `src-tauri/src/ratings/` — stored rating values and background refresh;
  `overfast.rs` alone knows the service URL, request path and response format.
- `e2e/test-launcher/` — a test-only executable that plays the launcher from a
  `scenario.json`.
- `e2e/` — WebDriver tests.
- `src-tauri/src/shell.rs`, `shell/` — desktop tray/window, Windows instance guard
  and startup registration; `tray.rs` derives the menu and calls core actions.
- `src-tauri/windows/hooks.nsh` — current-user startup removal during uninstall.
- `src-tauri/icons/noxdeck-app.png` — source for executable/shortcut/installer icons;
  regenerate with `npm run tauri icon -- src-tauri/icons/noxdeck-app.png --output src-tauri/icons`.
  `noxdeck-tray.ico` is the separate supplied tray icon.
- The version lives in one place: `[workspace.package]` in `Cargo.toml`. The app,
  the window and the installer read it from there.

## Environment variables

The program reads these to find things; the tests set them to point at stand-ins.

| Variable | Meaning | Default |
|---|---|---|
| `NOXDECK_BNET_ROAMING` | the launcher's config directory | `%APPDATA%\Battle.net` |
| `NOXDECK_BNET_LOCAL` | the launcher's logs and cache directory | `%LOCALAPPDATA%\Battle.net` |
| `NOXDECK_DATA_DIR` | Noxdeck's own data | `%APPDATA%\Noxdeck` |
| `NOXDECK_GAME_EXE` | file name of the game executable | `Overwatch.exe` |
| `NOXDECK_INSTANCE` | extra scope for the Windows user instance guard; set in every automated run | `main` |
| `NOXDECK_AUTOSTART_NAME` | replacement Run value name; automated runs use `NoxDeck-Test-…` | `Noxdeck` |
| `NOXDECK_LOCALE` | replacement Windows locale; a saved language still wins | Windows user locale |
| `NOXDECK_OVERFAST_URL` | rating service base URL; automated runs use a local stub | `https://overfast-api.tekrop.fr` |
| `NOXDECK_RATINGS_MS` | interval between refresh starts, including sleep, for tests | 1800000 |
| `NOXDECK_STOP_MS` | bound for stopping all launcher processes | 30000 |
| `NOXDECK_POLL_MS`, `NOXDECK_STAND_MS`, `NOXDECK_START_TIMEOUT_MS`, `NOXDECK_GONE_GRACE_MS` | timings, for the tests | 500, 5000, 60000, 15000 |
| `NOXDECK_GAME_TIMEOUT_MS`, `NOXDECK_GAME_SETTLE_MS`, `NOXDECK_HIDE_INTERVAL_MS` | game wait, settling wait, window hiding interval, for the tests | 120000, 5000, 10 |
| `NOXDECK_GAME_HANDOVER_MS`, `NOXDECK_HANDOVER_PAUSE_MS` | bound for visible game window + pause before closing the launcher (rule B), pause after the window appears | 120000, 15000 |

The core resolves the language. Tests set `NOXDECK_LOCALE` or `settings.json`
`language`; neither the window nor the tray uses localStorage for language.
