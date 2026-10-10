import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { A, B, record, startApp } from "./lib.mjs";

let app;
afterEach(async () => { await app?.stop(); app = null; });
const quiet = { dismissed: { remember_login: true, logout_warning: true } };
const options = (lang = "en") => ({
  accounts: [record(A), record(B)], settings: quiet, lang,
  list: A.email, scenario: { accounts: [{ ...A }, { ...B }], game_launch: "stays" },
});
const state = () => app.browser.execute(async () => window.__TAURI_INTERNALS__.invoke("get_state"));

async function assertMarked(a = A, label = "Overwatch") {
  await app.h.waitFor(`ingame-${a.id}`, { timeout: 2000 });
  assert.equal(await app.h.text(`ingame-${a.id}`), label);
  await app.h.tick(async () => (await state()).operation === null, "game start is still finishing");
  assert.equal(await app.h.el(`ingame-${a.id}`).isEnabled(), true);
  assert.equal(await app.h.el(`ingame-${a.id}`).getCSSProperty("color").then(p => p.value), "rgba(142,228,51,1)");
  assert.equal(await app.h.exists(`game-${a.id}`), false);
  const s = await state();
  assert.equal(s.accounts.filter((item) => item.isInGame).length, 1);
  assert.equal(s.accounts.find((item) => item.id === a.id).isInGame, true);
}

async function startOnA(lang = "en") {
  app = await startApp(options(lang));
  await app.h.click(`game-${A.id}`);
  await app.h.tick(async () => (await state()).gameRunning, "the stand-in game did not run");
  await app.h.waitGone("operation");
}

async function manualPlay() {
  app = await startApp(options());
  app.startLauncherOutside();
  await app.h.waitFor(`current-${A.id}`);
  app.playInLauncher();
  await app.h.tick(async () => (await state()).gameRunning, "manual Play did not start the stand-in game");
}

test("US4.1 started account has a pressable running button; other accounts keep Start game", async () => {
  await startOnA();
  await assertMarked();
  assert.equal(await app.h.exists(`ingame-${B.id}`), false);
  assert.equal(await app.h.el(`game-${B.id}`).isEnabled(), true);
  assert.equal(await app.h.exists("game-dialog"), false);
  assert.equal(await app.h.exists("operation"), false);
});

test("US4 restarting the playing account asks and replaces its process on the same account", async () => {
  await startOnA();
  await assertMarked();
  const gameFile = join(app.dirs.install, "game-started.json");
  const old = JSON.parse(readFileSync(gameFile, "utf8"));
  await app.h.click(`ingame-${A.id}`);
  await app.h.waitFor("game-dialog");
  await app.h.click("game-no");
  await app.h.waitGone("game-dialog");
  assert.deepEqual(JSON.parse(readFileSync(gameFile, "utf8")), old);
  await app.h.click(`ingame-${A.id}`);
  await app.h.waitFor("game-dialog");
  await app.h.click("game-yes");
  await app.h.tick(() => JSON.parse(readFileSync(gameFile, "utf8")).pid !== old.pid, "the game process was not replaced");
  await app.h.waitGone("operation");
  await assertMarked();
  const restarted = JSON.parse(readFileSync(gameFile, "utf8"));
  assert.equal(restarted.accountId, A.id);
  assert.notEqual(restarted.pid, old.pid);
  assert.deepEqual(JSON.parse(readFileSync(join(app.dirs.data, "state.json"), "utf8")), {
    schema: 1, game: { account_id: A.id, pid: restarted.pid },
  });
});

test("US4.2 the launcher's own Play marks its logged-in account", async () => {
  await manualPlay();
  await assertMarked();
  assert.equal(await app.h.exists(`current-${A.id}`), true);
});

test("US4.3 closing and switching the launcher preserve the game's account", async () => {
  await manualPlay();
  await assertMarked();
  app.killLauncher();
  await app.h.waitGone(`current-${A.id}`);
  await assertMarked();
  await app.h.click(`start-${B.id}`);
  await app.h.waitFor(`current-${B.id}`);
  await app.h.waitGone("operation");
  await assertMarked();
  assert.equal(await app.h.exists(`ingame-${B.id}`), false);
});

test("US4.4 ending the game restores Start game within five seconds", async () => {
  await startOnA();
  await assertMarked();
  const ended = Date.now();
  app.killGame();
  await app.h.waitGone(`ingame-${A.id}`, { timeout: 5000 });
  await app.h.waitFor(`game-${A.id}`, { timeout: 5000 });
  assert.ok(Date.now() - ended < 5000);
  assert.equal(await app.h.el(`game-${A.id}`).isEnabled(), true);
});

test("US4.5 restart restores the mark from state.json while the same game runs", async () => {
  await startOnA();
  await assertMarked();
  const saved = JSON.parse(readFileSync(join(app.dirs.data, "state.json"), "utf8"));
  const game = JSON.parse(readFileSync(join(app.dirs.install, "game-started.json"), "utf8"));
  assert.deepEqual(saved, { schema: 1, game: { account_id: A.id, pid: game.pid } });
  assert.equal(existsSync(join(app.dirs.data, "state.json.tmp")), false);
  await app.restart();
  await assertMarked();
});

test("US4.6 an unknown game's account is not guessed; both buttons retain the close question", async () => {
  app = await startApp(options());
  app.startGame();
  await app.h.tick(async () => (await state()).gameRunning, "stand-in game not detected");
  assert.equal(await app.h.exists(`ingame-${A.id}`), false);
  assert.equal(await app.h.exists(`ingame-${B.id}`), false);
  for (const a of [A, B]) {
    assert.equal(await app.h.el(`game-${a.id}`).isEnabled(), true);
    await app.h.click(`game-${a.id}`);
    await app.h.waitFor("game-dialog");
    await app.h.click("game-no");
    await app.h.waitGone("game-dialog");
  }
  app.killGame();
  await app.h.tick(async () => !(await state()).gameRunning, "stand-in game did not end");
  await app.h.click(`game-${A.id}`);
  await app.h.waitGone("operation");
  await assertMarked();
});

test("US4.7 consenting to start on B moves the mark only after B's game runs", async () => {
  await startOnA();
  await assertMarked();
  const old = JSON.parse(readFileSync(join(app.dirs.install, "game-started.json"), "utf8"));
  app.setScenario({ accounts: [{ ...A }, { ...B }], game_launch: "stays", login_delay_ms: 900 });
  await app.h.click(`game-${B.id}`);
  await app.h.waitFor("game-dialog");
  await app.h.click("game-yes");
  await app.h.waitState(B.id, "logging_in");
  await app.h.waitGone(`ingame-${A.id}`, { timeout: 3000 });
  assert.equal(await app.h.exists(`ingame-${A.id}`), false);
  assert.equal(await app.h.exists(`ingame-${B.id}`), false);
  await app.h.waitGone("operation");
  await assertMarked(B);
  assert.equal(await app.h.exists(`game-${A.id}`), true);
  const newGame = JSON.parse(readFileSync(join(app.dirs.install, "game-started.json"), "utf8"));
  assert.notEqual(newGame.pid, old.pid);
  assert.equal(newGame.accountId, B.id);
});
