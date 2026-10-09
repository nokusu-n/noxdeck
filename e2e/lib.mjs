// Harness for the interface tests: the running app, driven through tauri-driver,
// against the fake-launcher executable in a temporary directory. Nothing here
// touches the real Battle.net launcher: every location the app uses is moved into
// the temporary directory through the NOXDECK_* variables, and the harness refuses
// to start the app if one of them points elsewhere.

import { spawn, execFileSync } from "node:child_process";
import { closeSync, copyFileSync, existsSync, mkdirSync, openSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { remote } from "webdriverio";
import { startOverfast } from "./overfast.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const VERSION = readFileSync(join(root, "Cargo.toml"), "utf8").match(/^version = "([^"]+)"/m)[1];
// E2E_APP points the tests at another build of the app, e.g. an installed one.
export const APP = process.env.E2E_APP ?? join(root, "target", "e2e", "debug", "noxdeck.exe");
export const FAKE = join(root, "target", "e2e", "debug", "fake-launcher.exe");
const FIXTURE_CONFIG = join(root, "src-tauri", "tests", "fixtures", "Battle.net.config");
export const ORIGINAL_LIST =
  "user1@example.com,user2@example.com,user3@example.com,user4@example.com,user5@example.com";

/** The stand-in accounts the scenarios know. Placeholders only. */
export const A = { email: "userA@example.com", id: "aaaa0001", tag: "Alpha#1111" };
export const B = { email: "userB@example.com", id: "aaaa0002", tag: "Bravo#2222" };
export const C = { email: "userC@example.com", id: "aaaa0003", tag: "Charlie#3333" };

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export function record(a, over = {}) {
  return {
    id: a.id,
    email: a.email,
    battletag: a.tag,
    login_state: "ready",
    last_login_at: null,
    ...over,
  };
}

let nextPort = 4600 + Math.floor(Math.random() * 200) * 2;

async function waitForPort(port) {
  for (let i = 0; i < 100; i++) {
    try {
      const r = await fetch(`http://127.0.0.1:${port}/status`);
      if (r.ok) return;
    } catch {}
    await sleep(100);
  }
  throw new Error("tauri-driver did not start");
}

function killTree(pid) {
  try {
    execFileSync("taskkill", ["/PID", String(pid), "/T", "/F"], { stdio: "ignore" });
  } catch {}
}

/** Stop every process whose executable lies inside `dir` (the stand-ins we started). */
function killInside(dir) {
  const ps = `Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith('${dir.replace(/'/g, "''")}', 'OrdinalIgnoreCase') } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }`;
  try {
    execFileSync("powershell", ["-NoProfile", "-Command", ps], { stdio: "ignore" });
  } catch {}
}

/**
 * Start the app on a fresh fake launcher directory.
 * opts: scenario (object for scenario.json), list (initial SavedAccountNames),
 *       accounts (records for accounts.json), settings (object), lang ("ru" | "en"),
 *       env (extra environment), noInstall (leave the launcher uninstalled).
 */
export async function startApp(opts = {}) {
  const overfast = await startOverfast();
  for (const [tag, reply] of Object.entries(opts.overfast ?? {})) overfast.set(tag, reply);
  const temporaryRoot = join(root, "e2e", ".tmp");
  mkdirSync(temporaryRoot, { recursive: true });
  const tmp = mkdtempSync(join(temporaryRoot, "noxdeck-e2e-"));
  const d = { install: join(tmp, "install"), roaming: join(tmp, "roaming"), local: join(tmp, "local"), data: join(tmp, "data"), game: join(tmp, "game") };
  for (const p of Object.values(d)) mkdirSync(p, { recursive: true });

  const jsonPath = d.install.replaceAll("\\", "\\\\");
  let config = readFileSync(FIXTURE_CONFIG, "utf8").replace("C:\\\\Fake\\\\Battle.net", jsonPath);
  if (opts.list !== undefined) config = config.replace(ORIGINAL_LIST, opts.list);
  writeFileSync(join(d.roaming, "Battle.net.config"), config);

  if (!opts.noInstall) {
    copyFileSync(FAKE, join(d.install, "Battle.net Launcher.exe"));
    copyFileSync(FAKE, join(d.install, "Battle.net.exe"));
  }
  copyFileSync(FAKE, join(d.game, "FakeGame.exe"));
  if (opts.scenario) writeFileSync(join(d.install, "scenario.json"), JSON.stringify(opts.scenario));
  if (opts.accounts) writeFileSync(join(d.data, "accounts.json"), JSON.stringify({ schema: 1, accounts: opts.accounts }));
  writeFileSync(join(d.data, "settings.json"), JSON.stringify({ schema: 1, sort: "last_login", dismissed: { remember_login: false, logout_warning: false }, after_start: "stay", ...opts.settings }));
  if (opts.rawAccounts) writeFileSync(join(d.data, "accounts.json"), opts.rawAccounts);

  const env = {
    ...process.env,
    PATH: `${process.env.PATH};${join(homedir(), ".cargo", "bin")}`,
    NOXDECK_BNET_ROAMING: d.roaming,
    NOXDECK_BNET_LOCAL: d.local,
    NOXDECK_DATA_DIR: d.data,
    NOXDECK_GAME_EXE: "FakeGame.exe",
    NOXDECK_POLL_MS: "100",
    NOXDECK_STAND_MS: "700",
    NOXDECK_START_TIMEOUT_MS: "4000",
    NOXDECK_GONE_GRACE_MS: "1500",
    NOXDECK_GAME_TIMEOUT_MS: "1800",
    NOXDECK_GAME_SETTLE_MS: "900",
    NOXDECK_INSTANCE: `e2e-${process.pid}-${nextPort}`,
    NOXDECK_AUTOSTART_NAME: `NoxDeck-Test-e2e-${process.pid}-${nextPort}`,
    NOXDECK_LOCALE: opts.lang === "ru" ? "ru-RU" : "en-US",
    TEMP: tmp,
    TMP: tmp,
    ...opts.env,
    NOXDECK_OVERFAST_URL: overfast.url,
  };
  for (const k of ["NOXDECK_BNET_ROAMING", "NOXDECK_BNET_LOCAL", "NOXDECK_DATA_DIR"]) {
    if (!resolve(env[k]).startsWith(tmp)) throw new Error(`${k} must point inside the temporary directory`);
  }
  if (!env.NOXDECK_INSTANCE || !env.NOXDECK_AUTOSTART_NAME.startsWith("NoxDeck-Test-")) throw new Error("The instance and startup entry must be isolated for tests");

  const port = nextPort;
  nextPort += 2;
  // Everything the driver and the app print goes to a file the tests can read.
  const outFile = join(tmp, "output.txt");
  const outFd = openSync(outFile, "w");
  const driver = spawn("tauri-driver", ["--port", String(port), "--native-port", String(port + 1)], { env, stdio: ["ignore", outFd, outFd] });
  await waitForPort(port);

  const open = async () => {
    const b = await remote({
      hostname: "127.0.0.1",
      port,
      path: "/",
      logLevel: "error",
      connectionRetryCount: 1,
      capabilities: {
        browserName: "wry",
        "wdio:enforceWebDriverClassic": true,
        "tauri:options": { application: APP },
      },
    });
    return b;
  };
  let browser = await open();
  const h = makeHelpers(() => browser);
  return {
    overfast,
    get browser() {
      return browser;
    },
    h,
    dirs: d,
    env,
    tmp,
    /** Start the stand-in game, as the player would. */
    startGame() {
      const game = spawn(join(d.game, "FakeGame.exe"), [], { stdio: "ignore", detached: true, windowsHide: true });
      game.unref();
      return game;
    },
    /** Whatever the driver and the app printed so far. */
    output() {
      return readFileSync(outFile, "utf8");
    },
    /** The player starts the (stand-in) launcher by hand, outside the app. */
    startLauncherOutside() {
      spawn(join(d.install, "Battle.net Launcher.exe"), [], { env, cwd: d.install, stdio: "ignore", detached: true, windowsHide: true }).unref();
    },
    /** The stand-in equivalent of the launcher's own Play button. */
    playInLauncher() {
      spawn(join(d.install, "Battle.net.exe"), ["--exec=launch Pro"], { env, cwd: d.install, stdio: "ignore", detached: true, windowsHide: true }).unref();
    },
    killGame() {
      killInside(d.game);
    },
    /** The player closes the (stand-in) launcher. */
    killLauncher() {
      killInside(d.install);
    },
    list() {
      const m = readFileSync(join(d.roaming, "Battle.net.config"), "utf8").match(/"SavedAccountNames": "([^"]*)"/);
      return m ? m[1] : null;
    },
    accountsFile() {
      return JSON.parse(readFileSync(join(d.data, "accounts.json"), "utf8"));
    },
    settingsFile() {
      return JSON.parse(readFileSync(join(d.data, "settings.json"), "utf8"));
    },
    /** Another launcher run changed the stand-in's saved login: the scenario file. */
    setScenario(s) {
      writeFileSync(join(d.install, "scenario.json"), JSON.stringify(s));
    },
    /** Close the app and open it again on the same directories. */
    async restart() {
      const processId = () => browser.execute(() => window.__TAURI_INTERNALS__.invoke("test_process_id"));
      const previous = await processId();
      await browser.deleteSession().catch(() => {});
      browser = await open();
      if (await processId() === previous) throw new Error("restart must create a fresh core process");
    },
    async stop() {
      await overfast.stop();
      await browser.deleteSession().catch(() => {});
      killTree(driver.pid);
      killInside(tmp);
      const startupName = env.NOXDECK_AUTOSTART_NAME.replace(/'/g, "''");
      execFileSync("powershell", ["-NoProfile", "-Command", `if ($null -ne (Get-ItemProperty -LiteralPath 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Run' -Name '${startupName}' -ErrorAction SilentlyContinue)) { Remove-ItemProperty -LiteralPath 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Run' -Name '${startupName}' -ErrorAction Stop }; exit 0`], { env, stdio: "ignore" });
      closeSync(outFd);
      if (!resolve(tmp).startsWith(temporaryRoot + "\\")) throw new Error("App cleanup escaped the workspace");
      for (let i = 0; i < 10; i++) {
        try {
          rmSync(tmp, { recursive: true, force: true });
          break;
        } catch {
          await sleep(300);
        }
      }
      if (existsSync(tmp)) throw new Error("The test directory was left behind");
    },
  };
}

export function makeHelpers(getBrowser) {
  const sel = (id) => `[data-testid="${id}"]`;
  const el = (id) => getBrowser().$(sel(id));
  const wait = { timeout: 15000, interval: 100 };
  return {
    el,
    sel,
    async click(id) {
      const e = await el(id);
      await e.waitForClickable(wait);
      await e.click();
    },
    async text(id) {
      const e = await el(id);
      await e.waitForExist(wait);
      return e.getText();
    },
    async exists(id) {
      return el(id).isExisting();
    },
    async waitFor(id, opts = {}) {
      await (await el(id)).waitForExist({ ...wait, ...opts });
    },
    async waitGone(id, opts = {}) {
      await (await el(id)).waitForExist({ ...wait, ...opts, reverse: true });
    },
    async waitState(id, state, opts = {}) {
      const e = await el(`state-${id}`);
      await getBrowser().waitUntil(async () => (await e.isExisting()) && (await e.getAttribute("data-state")) === state, {
        timeout: 20000,
        interval: 100,
        timeoutMsg: `record ${id} never reached state ${state}`,
        ...opts,
      });
    },
    async ids() {
      const rows = await getBrowser().$$('[data-testid^="account-"]');
      const out = [];
      for (const r of rows) out.push((await r.getAttribute("data-testid")).slice("account-".length));
      return out;
    },
    async tick(cond, msg, timeout = 20000) {
      await getBrowser().waitUntil(cond, { timeout, interval: 100, timeoutMsg: msg });
    },
  };
}
