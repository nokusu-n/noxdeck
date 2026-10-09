import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import { A, record, startApp } from "./lib.mjs";

let app;
afterEach(async () => { await app?.stop(); app = null; });

test("US5: disabling launcher closing persists and keeps it open after a game starts", async () => {
  app = await startApp({ accounts: [record(A)], settings: { dismissed: { remember_login: true, logout_warning: true } },
    scenario: { accounts: [A], readiness: "playable", game_launch: "stays" } });
  const { h } = app;
  await h.click("settings-open");
  assert.ok(await h.exists("set-close-launcher"), "launcher closing control must exist");
  assert.equal(await h.el("set-close-launcher").isSelected(), true);
  await h.click("set-close-launcher");
  await h.tick(() => app.settingsFile().close_launcher === false, "setting was not saved");
  await app.restart();
  await h.click("settings-open");
  assert.equal(await h.el("set-close-launcher").isSelected(), false);
  await h.click("settings-back");
  await h.click(`game-${A.id}`);
  await h.tick(async () => (await h.text(`state-${A.id}`)).includes("Game started"), "no game-started message");
  await h.waitGone("operation");
  const state = await app.browser.execute(() => window.__TAURI_INTERNALS__.invoke("get_state"));
  assert.equal(state.gameRunning, true);
  assert.equal(state.launcher, "logged_in");
  await h.waitFor(`current-${A.id}`);
});
