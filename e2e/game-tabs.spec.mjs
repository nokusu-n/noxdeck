import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import { join } from "node:path";
import { A, B, C, record, startApp } from "./lib.mjs";

let app;
afterEach(async () => { await app?.stop(); app = null; });
const quiet = { auto_ratings: false, dismissed: { remember_login: true, logout_warning: true } };
const capture = async name => { if (process.env.NOXDECK_CAPTURE_DIR) await app.browser.saveScreenshot(join(process.env.NOXDECK_CAPTURE_DIR, name)); };

test("a role sorts highest first, flips on the second press and remembers each tab's sort", async () => {
  const rating = (rank, division) => ({ tank: { rank, division, source: "manual" } });
  app = await startApp({ accounts: [record(A, { ratings: rating("gold", 3) }), record(B, { ratings: rating("champion", 1) }), record(C)], settings: quiet });
  const { h } = app;
  await h.click("sort-battletag");
  await h.click("tab-overwatch");
  await h.click("sort-role-tank");
  await h.tick(async () => (await h.ids()).join() === [B.id, A.id, C.id].join(), "descending role order");
  assert.equal(await h.el("sort-role-tank").getAttribute("data-direction"), "desc");
  assert.equal(await h.el("sort-last-login").getAttribute("aria-pressed"), "false");
  await capture("overwatch-role-sort.png");
  await h.click("sort-role-tank");
  await h.tick(async () => (await h.ids()).join() === [A.id, B.id, C.id].join(), "ascending role order");
  assert.equal(await h.el("sort-role-tank").getAttribute("data-direction"), "asc");
  await h.click("tab-all");
  assert.equal(await h.el("sort-battletag").getAttribute("aria-pressed"), "true");
  assert.equal(await h.exists("sort-role-tank"), false);
  await h.click("tab-overwatch");
  assert.equal(await h.el("sort-role-tank").getAttribute("data-direction"), "asc");
});

test("the gear changes a game, moves the card from its tab and keeps the now empty tab selected", async () => {
  app = await startApp({ accounts: [record(A), record(B, { game: "call_of_duty" }), record(C, { game: "none" })], settings: quiet });
  const { h } = app;
  await h.waitFor(`choose-game-${C.id}`);
  await capture("all-mixed.png");
  const heights = await app.browser.execute(() => [...document.querySelectorAll('[data-testid^="account-"]')].map(e => e.getBoundingClientRect().height));
  assert.ok(heights.every(h => h === heights[0]), "cards have equal heights");
  await h.click("tab-overwatch");
  await h.waitGone(`account-${B.id}`);
  await h.click(`game-settings-${A.id}`);
  await h.waitFor(`game-picker-${A.id}`);
  assert.equal(await h.el(`game-option-${A.id}-overwatch`).getAttribute("aria-checked"), "true");
  await capture("gear-popup.png");
  await h.click(`game-option-${A.id}-call_of_duty`);
  await h.waitGone(`account-${A.id}`);
  assert.equal(await h.el("tab-overwatch").getAttribute("aria-selected"), "true");
  assert.equal(await h.text("game-moved"), "Moved to Call of Duty");
  await h.waitFor("empty-game-tab");
  await capture("empty-tab.png");
  await h.click("tab-call_of_duty");
  await h.waitFor(`account-${A.id}`);
  assert.equal(app.accountsFile().accounts.find(a => a.id === A.id).game, "call_of_duty");
  await h.click("tab-all");
  await h.click(`choose-game-${C.id}`);
  await h.waitFor(`game-picker-${C.id}`);
});

test("an account added from an empty game's tab gets that game without a choice during login", async () => {
  app = await startApp({ accounts: [record(B)], settings: quiet, scenario: { accounts: [A, B], login_screen: [{ at_ms: 300, login: A.email }] } });
  const { h } = app;
  await h.click("tab-world_of_warcraft");
  await h.click("empty-game-add");
  await h.waitFor(`account-${A.id}`);
  assert.equal(app.accountsFile().accounts.find(a => a.id === A.id).game, "world_of_warcraft");
  assert.deepEqual(await h.ids(), [A.id]);
  assert.equal(await h.exists(`game-picker-${A.id}`), false);
});

test("the chosen tab survives the tray and a real program restart", async () => {
  app = await startApp({ accounts: [record(A)], settings: quiet });
  const { h } = app;
  await h.click("tab-call_of_duty");
  await h.waitFor("empty-game-tab");
  await h.click("win-close");
  await app.browser.execute(() => window.__TAURI_INTERNALS__.invoke("test_tray_click", { id: "open" }));
  await h.waitFor("tab-call_of_duty");
  assert.equal(await h.el("tab-call_of_duty").getAttribute("aria-selected"), "true");
  await app.restart();
  await h.waitFor("empty-game-tab");
  assert.equal(await h.el("tab-call_of_duty").getAttribute("aria-selected"), "true");
});
