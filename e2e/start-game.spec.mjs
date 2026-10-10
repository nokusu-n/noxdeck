import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import { appendFileSync, existsSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { A, B, record, sleep, startApp } from "./lib.mjs";

let app;
afterEach(async () => { await app?.stop(); app = null; });
const quiet = { dismissed: { remember_login: true, logout_warning: true } };
const both = [record(A), record(B)];
const scenario = (extra = {}) => ({ accounts: [{ ...A }, { ...B }], game_launch: "stays", ...extra });
const config = () => readFileSync(join(app.dirs.roaming, "Battle.net.config"), "utf8");
const launchFile = () => join(app.dirs.install, "game-started.json");
const gameStarted = () => JSON.parse(readFileSync(launchFile(), "utf8"));
const mainCount = () => readdirSync(join(app.dirs.local, "Logs"))
  .filter((file) => readFileSync(join(app.dirs.local, "Logs", file), "utf8").includes("--from-launcher")).length;
const state = () => app.browser.execute(async () => window.__TAURI_INTERNALS__.invoke("get_state"));

async function assertFinished(a = A) {
  await app.h.tick(async () => (await app.h.text(`state-${a.id}`)).includes("Game started"), "no game-started message");
  await app.h.waitGone("operation");
  const s = await state();
  assert.equal(s.launcher, "closed");
  assert.equal(s.gameRunning, true);
  assert.equal(s.accounts.find((item) => item.id === a.id).loginState, "ready");
  assert.equal(gameStarted().accountId, a.id, "the stand-in game received the chosen account");
  assert.ok(gameStarted().pid > 0);
  assert.equal(await app.h.exists(`current-${a.id}`), false);
}

test("US1.1 closed launcher: progress, game on the chosen account, launcher closed", async () => {
  app = await startApp({ accounts: both, settings: quiet, lang: "en", scenario: scenario({ login_delay_ms: 1200 }) });
  const { h } = app;
  await h.waitFor(`game-${A.id}`, { timeout: 2000 });
  assert.match(await h.text(`game-${A.id}`), /Overwatch/);
  await h.click(`game-${A.id}`);
  await h.waitState(A.id, "logging_in");
  assert.equal(await h.el(`game-${B.id}`).isEnabled(), false);
  assert.equal(await h.el("add-account").isEnabled(), false);
  await h.tick(async () => (await h.el("operation").getAttribute("data-phase")) === "closing_launcher", "no closing phase");
  assert.match(await h.text(`state-${A.id}`), /Closing Battle.net/);
  const duringHandover = await state();
  assert.equal(duringHandover.gameRunning, true);
  assert.equal(duringHandover.launcher, "logged_in", "launcher remains alive during the handover pause");
  await assertFinished();
});

test("US1.2 open on another account: restart on B and start B's game", async () => {
  app = await startApp({ accounts: both, settings: quiet, lang: "en", scenario: scenario() });
  await app.h.waitFor(`game-${B.id}`, { timeout: 2000 });
  await app.h.click(`start-${A.id}`);
  await app.h.waitFor(`current-${A.id}`);
  await app.h.waitGone("operation");
  await app.h.click(`game-${B.id}`);
  await assertFinished(B);
  assert.equal(mainCount(), 2);
  assert.equal(app.list(), B.email);
});

test("US1.3 current account: start without restarting the launcher", async () => {
  app = await startApp({ accounts: both, settings: quiet, lang: "en", scenario: scenario() });
  await app.h.waitFor(`game-${A.id}`, { timeout: 2000 });
  await app.h.click(`start-${A.id}`);
  await app.h.waitFor(`current-${A.id}`);
  await app.h.waitGone("operation");
  const before = config();
  const loginTime = app.accountsFile().accounts.find((a) => a.id === A.id).last_login_at;
  await app.h.click(`game-${A.id}`);
  await assertFinished();
  assert.equal(mainCount(), 1);
  assert.equal(config(), before);
  assert.equal(app.accountsFile().accounts.find((a) => a.id === A.id).last_login_at, loginTime);
});

test("US1.4 ready, a brief started message, and the new login time", async () => {
  app = await startApp({ accounts: both, settings: quiet, lang: "en", scenario: scenario() });
  await app.h.waitFor(`game-${A.id}`, { timeout: 2000 });
  await app.h.click(`game-${A.id}`);
  await assertFinished();
  await app.h.waitState(A.id, "ready");
  assert.ok(app.accountsFile().accounts.find((a) => a.id === A.id).last_login_at);
  await app.h.tick(async () => (await app.h.text(`state-${A.id}`)) === "Ready", "the started message never cleared", 7000);
});

test("US1.5 every record has the game button, including the current record", async () => {
  app = await startApp({ accounts: both, settings: quiet, lang: "en", scenario: scenario() });
  await app.h.waitFor(`game-${A.id}`, { timeout: 2000 });
  assert.match(await app.h.text(`game-${B.id}`), /Overwatch/);
  await app.h.click(`start-${A.id}`);
  await app.h.waitFor(`current-${A.id}`);
  await app.h.waitGone("operation");
  assert.equal(await app.h.exists(`start-${A.id}`), false);
  assert.equal(await app.h.el(`game-${A.id}`).isEnabled(), true);
});

test("US1.6 launcher may exit itself; its window setting and other config bytes stay unchanged", async () => {
  app = await startApp({ accounts: both, settings: quiet, lang: "en", scenario: scenario({ game_launch: "exits" }) });
  await app.h.waitFor(`game-${A.id}`, { timeout: 2000 });
  const before = config();
  await app.h.click(`game-${A.id}`);
  await assertFinished();
  assert.equal(config().replace(/"SavedAccountNames": "[^"]*"/, '"SavedAccountNames": ""'),
    before.replace(/"SavedAccountNames": "[^"]*"/, '"SavedAccountNames": ""'));
  assert.equal(JSON.parse(config()).Client.GameLaunchWindowBehavior, "2");
});

test("US1 game is unavailable without an e-mail or launcher", async () => {
  app = await startApp({ accounts: [record(A, { email: null })], settings: quiet, lang: "en", scenario: scenario() });
  await app.h.waitFor(`game-${A.id}`, { timeout: 2000 });
  assert.equal(await app.h.el(`game-${A.id}`).isEnabled(), false);
  assert.match(await app.h.el(`game-${A.id}`).getAttribute("title"), /e-mail/);
  await app.stop(); app = null;
  app = await startApp({ accounts: both, noInstall: true, lang: "en" });
  await app.h.waitFor(`game-${A.id}`);
  assert.equal(await app.h.el(`game-${A.id}`).isEnabled(), false);
});

const senderCount = () => readdirSync(join(app.dirs.local, "Logs"))
  .filter((file) => readFileSync(join(app.dirs.local, "Logs", file), "utf8").includes("--exec=launch Pro")).length;
function manualLogin() {
  const file = readdirSync(join(app.dirs.local, "Logs"))
    .find((file) => readFileSync(join(app.dirs.local, "Logs", file), "utf8").includes("--from-launcher"));
  appendFileSync(join(app.dirs.local, "Logs", file),
    `I 2026-10-09 01:00:10.000000 [BNLogin] {Main} Logged into Battle.net successfully. |bnet=1:0:${A.id}|game=2:0:00000000\n`);
}
async function assertNoGame() {
  assert.equal(existsSync(launchFile()), false);
  assert.equal((await state()).gameRunning, false);
}

test("US2.1 needs update: no command and no automatic game after the update", async () => {
  app = await startApp({ accounts: both, settings: quiet, lang: "en", scenario: scenario({
    readiness: "not_playable", after_login: [{ at_ms: 3000, playable: true }],
  }) });
  await app.h.click(`game-${A.id}`);
  await app.h.waitState(A.id, "game_needs_update", { timeout: 5000 });
  assert.equal(await app.h.text(`state-${A.id}`), "Update the game");
  await app.h.waitGone("operation");
  assert.equal((await state()).launcher, "logged_in");
  await sleep(3200);
  await assertNoGame();
  assert.equal(senderCount(), 0);
});

test("US2.2 password wanted: later manual login turns ready without starting the game", async () => {
  app = await startApp({ accounts: both, settings: quiet, lang: "en", scenario: scenario({
    accounts: [{ ...A, mode: "login_screen" }, { ...B }], login_screen: [{ at_ms: 3000, login: A.email }],
  }) });
  await app.h.click(`game-${A.id}`);
  await app.h.waitState(A.id, "password_wanted", { timeout: 5000 });
  await app.h.waitGone("operation");
  assert.equal((await state()).launcher, "login_screen");
  await app.h.waitState(A.id, "ready");
  await app.h.waitFor(`current-${A.id}`);
  await assertNoGame();
  assert.equal(senderCount(), 0);
});

test("US2.3 code wanted: leave the launcher open, without a game command", async () => {
  app = await startApp({ accounts: both, settings: quiet, lang: "en", scenario: scenario({ accounts: [{ ...A, mode: "code" }, { ...B }] }) });
  await app.h.click(`game-${A.id}`);
  await app.h.waitState(A.id, "code_wanted", { timeout: 5000 });
  assert.equal(await app.h.text(`state-${A.id}`), "Code needed");
  await app.h.waitGone("operation");
  assert.equal((await state()).launcher, "login_screen");
  await assertNoGame();
  assert.equal(senderCount(), 0);
});

test("US2.4 no answer: bounded wait, launcher left open, later manual login clears the message", async () => {
  app = await startApp({ accounts: both, settings: quiet, lang: "en", scenario: scenario({ accounts: [{ ...A, mode: "silent" }, { ...B }] }) });
  await app.h.click(`game-${A.id}`);
  await app.h.waitState(A.id, "no_answer", { timeout: 7000 });
  await app.h.waitGone("operation");
  assert.equal((await state()).launcher, "login_screen");
  manualLogin();
  await app.h.waitState(A.id, "ready", { timeout: 3000 });
  await assertNoGame();
  assert.equal(senderCount(), 0);
});

test("US2.5 ignored launch: bounded game wait and the game-not-started message", async () => {
  app = await startApp({ accounts: both, settings: quiet, lang: "en", scenario: scenario({ game_launch: "ignores" }) });
  await app.h.click(`game-${A.id}`);
  await app.h.waitState(A.id, "game_not_started", { timeout: 5000 });
  assert.equal(await app.h.text(`state-${A.id}`), "Game didn’t start");
  await app.h.waitGone("operation");
  assert.equal((await state()).launcher, "logged_in");
  assert.equal(senderCount(), 1);
  await assertNoGame();
});

test("US2 unknown readiness still requests the game", async () => {
  app = await startApp({ accounts: both, settings: quiet, lang: "en", scenario: scenario({ readiness: "unknown" }) });
  await app.h.click(`game-${A.id}`);
  await app.h.waitState(A.id, "closing_launcher", { timeout: 5000 });
  await assertFinished();
  assert.equal(senderCount(), 1);
});

async function withRunningGame(lang = "en") {
  app = await startApp({ accounts: both, list: A.email, settings: quiet, lang, scenario: scenario() });
  app.startLauncherOutside();
  await app.h.waitFor(`current-${A.id}`);
  const game = app.startGame();
  await app.h.tick(async () => (await state()).gameRunning, "stand-in game not detected");
  return game;
}

test("US3.1 a running game asks before touching the launcher or game", async () => {
  const game = await withRunningGame();
  const before = config();
  await app.h.click(`game-${B.id}`);
  await app.h.waitFor("game-dialog", { timeout: 2000 });
  assert.match(await app.h.text("game-dialog"), /Overwatch is running right now\. Close the game\?/);
  assert.equal(await app.h.exists("operation"), false);
  assert.equal(config(), before);
  assert.equal(game.exitCode, null);
  assert.equal(mainCount(), 1);
  assert.equal(senderCount(), 0);
});

test("US3.2 No leaves the same game and launcher unchanged", async () => {
  const game = await withRunningGame();
  const before = config();
  await app.h.click(`game-${B.id}`);
  await app.h.waitFor("game-dialog", { timeout: 2000 });
  await app.h.click("game-no");
  await app.h.waitGone("game-dialog");
  await sleep(500);
  assert.equal(game.exitCode, null);
  assert.equal(game.signalCode, null);
  assert.equal(config(), before);
  assert.equal(mainCount(), 1);
  assert.equal(senderCount(), 0);
  assert.equal(await app.h.exists(`current-${A.id}`), true);
  assert.equal(await app.h.exists("operation"), false);
});

test("US3.3 Yes closes the existing game and starts a new one on the selected account", async () => {
  const game = await withRunningGame();
  await app.h.click(`game-${B.id}`);
  await app.h.waitFor("game-dialog", { timeout: 2000 });
  await app.h.click("game-yes");
  await app.h.waitGone("game-dialog");
  await assertFinished(B);
  assert.notEqual(game.exitCode, null, "the original game was stopped");
  assert.notEqual(gameStarted().pid, game.pid, "a new game process was started");
  assert.equal(app.list(), B.email);
});

test("US3.4 Start launcher and Add account still never ask about or stop the game", async () => {
  const game = await withRunningGame();
  await app.h.click(`game-${B.id}`);
  await app.h.waitFor("game-dialog", { timeout: 2000 });
  await app.h.click("game-no");
  await app.h.click(`start-${B.id}`);
  await app.h.waitFor(`current-${B.id}`);
  await app.h.waitGone("operation");
  assert.equal(await app.h.exists("game-dialog"), false);
  assert.equal(game.exitCode, null);
  app.setScenario(scenario({ login_screen: [{ at_ms: 1400, login: A.email }] }));
  await app.h.click("add-account");
  await app.h.waitFor("waiting");
  assert.equal(await app.h.exists("game-dialog"), false);
  await app.h.waitFor(`current-${A.id}`);
  await app.h.waitGone("waiting");
  assert.equal(game.exitCode, null);
  assert.equal(game.signalCode, null);
});
