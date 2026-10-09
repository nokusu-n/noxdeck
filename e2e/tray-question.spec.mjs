import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import { A, B, record, startApp } from "./lib.mjs";
let app;
afterEach(async () => { await app?.stop(); app = null; });

test("stage 3 US1.7: a tray question is rendered from core state and both answers work", async () => {
  app = await startApp({ lang: "en", accounts: [record(A), record(B)], settings: { dismissed: { remember_login: true, logout_warning: true } }, scenario: { accounts: [{ ...A }, { ...B }], readiness: "playable", game_launch: "stays" } });
  await app.h.waitFor(`account-${A.id}`);
  const game = app.startGame();
  const state = () => app.browser.execute(() => window.__TAURI_INTERNALS__.invoke("get_state"));
  await app.h.tick(async () => (await state()).gameRunning, "the stand-in game was not seen");
  const clickTray = () => app.browser.execute((id) => window.__TAURI_INTERNALS__.invoke("test_tray_click", { id }), `acc-${B.id}`);
  await clickTray();
  await app.h.waitFor("game-dialog", { timeout: 2000 });
  assert.match(await app.h.text("game-dialog"), /Overwatch is running right now\. Close the game\?/);
  await app.h.click("game-no");
  await app.h.waitGone("game-dialog");
  assert.equal((await state()).ask, null);
  assert.equal(game.exitCode, null);
  await clickTray();
  await app.h.click("game-yes");
  await app.h.waitGone("game-dialog");
  await app.h.waitGone("operation");
  await app.h.waitFor(`ingame-${B.id}`, { timeout: 5000 });
  assert.equal((await state()).ask, null);
  assert.notEqual(game.exitCode, null);
  assert.equal(app.list(), B.email);
});
