import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import { join } from "node:path";
import { A, record, sleep, startApp } from "./lib.mjs";

let app;
afterEach(async () => { await app?.stop(); app = null; });
test("a start ignores a double click, then Cancel clears the wait and allows another start", async () => {
  app = await startApp({ accounts: [record(A)], settings: { dismissed: { remember_login: true, logout_warning: true } },
    scenario: { accounts: [A], login_delay_ms: 2800 }, env: { NOXDECK_START_TIMEOUT_MS: "7000" } });
  const { h } = app;
  await h.click(`game-${A.id}`);
  await h.tick(async () => (await h.text(`game-${A.id}`)) === "Cancel", "Cancel label");
  await h.click(`game-${A.id}`);
  await h.waitFor("operation");
  await sleep(1100);
  if (process.env.NOXDECK_CAPTURE_DIR) await app.browser.saveScreenshot(join(process.env.NOXDECK_CAPTURE_DIR, "cancel.png"));
  await h.click(`game-${A.id}`);
  await h.waitGone("operation");
  assert.equal(await h.text(`game-${A.id}`), "Overwatch");
  assert.equal(await h.text(`state-${A.id}`), "Ready");
  app.setScenario({ accounts: [A], game_launch: "stays" });
  await h.click(`start-${A.id}`);
  await h.waitFor(`current-${A.id}`);
});

test("once the game process appears the card and tray stop offering Cancel", async () => {
  app = await startApp({ accounts: [record(A)],
    settings: { close_launcher: false, dismissed: { remember_login: true, logout_warning: true } },
    scenario: { accounts: [A], game_launch: "stays" }, env: { NOXDECK_GAME_SETTLE_MS: "3000" } });
  const { h } = app;
  const invoke = (command) => app.browser.execute((name) => window.__TAURI_INTERNALS__.invoke(name), command);
  await h.click(`game-${A.id}`);
  await h.tick(async () => {
    const s = await invoke("get_state");
    return s.gameRunning && s.operation?.cancellable === false;
  }, "the new game must disable cancellation");
  await h.waitFor(`ingame-${A.id}`);
  assert.equal(await h.text(`ingame-${A.id}`), "Overwatch");
  assert.equal(await h.el(`ingame-${A.id}`).isEnabled(), false);
  const menu = await invoke("test_tray_menu");
  const item = menu.find(i => i.id === `acc-${A.id}`);
  assert.equal(item.enabled, false);
  assert.notEqual(item.label, "Cancel");
  assert.equal(menu.some(i => i.id === "cancel"), false);
  await invoke("cancel_operation");
  await h.waitGone("operation");
  const s = await invoke("get_state");
  assert.equal(s.accounts[0].lastOutcome, "game_started");
  assert.equal(s.launcher, "logged_in");
  assert.equal(s.gameRunning, true);
  assert.equal(await h.el(`ingame-${A.id}`).isEnabled(), true);
});
