import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import { A, B, record, startApp } from "./lib.mjs";

let app;
afterEach(async () => { await app?.stop(); app = null; });
const quiet = { dismissed: { remember_login: true, logout_warning: true } };
const invoke = (command, args) => app.browser.execute((command, args) => window.__TAURI_INTERNALS__.invoke(command, args), command, args);
const menu = () => invoke("test_tray_menu", {});
const checked = id => app.h.el(id).isSelected();
async function settings() { await app.h.click("settings-open"); await app.h.waitFor("settings"); }

test("settings 1: all controls show their defaults", async () => {
  app = await startApp({ settings: { after_start: undefined } });
  await settings();
  assert.equal(await checked("set-autostart"), false);
  assert.equal(await checked("set-after-tray"), true);
  assert.equal(await app.h.exists("set-show-game"), false);
  assert.equal(await app.h.exists("set-show-ratings"), false);
  assert.equal(await checked("set-lang-en"), true);
  for (const id of ["set-after-stay", "set-after-close", "set-lang-ru"]) assert.equal(await checked(id), false);
  await app.h.click("settings-back"); await app.h.waitGone("settings");
});

test("settings 2: another game has only Launcher and its tray start preserves the playing account", async () => {
  app = await startApp({ accounts: [record(A), record(B, { game: "call_of_duty" })], settings: quiet, scenario: { accounts: [A, B], readiness: "playable", game_launch: "stays" } });
  const { h } = app;
  await h.click(`game-${A.id}`); await h.waitFor(`ingame-${A.id}`);
  await h.waitGone("operation");
  assert.equal(await h.exists(`game-${B.id}`), false);
  assert.equal(await h.exists(`ingame-${A.id}`), true);
  await invoke("test_tray_click", { id: `acc-${B.id}` });
  await h.tick(async () => (await invoke("get_state", {})).operation === null, "tray operation did not end");
  assert.equal((await invoke("get_state", {})).accounts.find(a => a.id === B.id).isCurrent, true, JSON.stringify(await invoke("get_state", {})));
  await h.waitFor(`current-${B.id}`); await h.waitGone("operation");
  assert.equal(await h.exists("game-dialog"), false);
  assert.equal((await invoke("get_state", {})).accounts.find(a => a.id === A.id).isInGame, true, "the existing game stays on A");
});

test("settings 3: the first language follows the Windows locale", async () => {
  app = await startApp({ env: { NOXDECK_LOCALE: "ru-RU" } });
  await settings(); assert.equal(await checked("set-lang-ru"), true);
  assert.equal(await app.h.text("settings-back"), "Назад");
});

test("settings 4: selecting a language changes the window and tray immediately and keeps the choice", async () => {
  app = await startApp({ accounts: [record(A)], env: { NOXDECK_LOCALE: "ru-RU" } });
  await settings(); await app.h.click("set-lang-en");
  await app.h.tick(async () => await app.h.text("settings-back") === "Back", "the window language did not change");
  assert.equal((await menu())[0].label, "Open Noxdeck");
  assert.equal((await menu()).at(-1).label, "Exit");
  await app.restart(); await settings();
  assert.equal(await checked("set-lang-en"), true); assert.equal(await app.h.text("settings-back"), "Back");
  assert.equal((await menu())[0].label, "Open Noxdeck");
});

test("settings 5: changed settings survive an actual program restart", async () => {
  app = await startApp({ accounts: [record(A)], settings: quiet });
  await settings();
  await app.h.click("set-after-close"); await app.h.click("set-close-launcher"); await app.h.click("set-lang-ru"); await app.h.click("set-autostart");
  await app.h.tick(async () => await checked("set-autostart"), "startup was not enabled");
  await app.restart(); await settings();
  for (const id of ["set-after-close", "set-lang-ru", "set-autostart"]) assert.equal(await checked(id), true);
  assert.equal(await checked("set-close-launcher"), false);
  assert.equal(app.settingsFile().autostart, undefined, "startup is read from Windows, not settings.json");
  await app.h.click("settings-back"); assert.equal(await app.h.exists(`game-${A.id}`), true);
});
