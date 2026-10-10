import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import { A, B, record, startApp, sleep } from "./lib.mjs";

let app;
afterEach(async () => { await app?.stop(); app = null; });
const roles = ["tank", "damage", "support"];
const ranks = ["bronze", "silver", "gold", "platinum", "emerald", "diamond", "master", "grandmaster", "champion"];
async function start(opts = {}) {
  app = await startApp({ accounts: [record(A)], ...opts });
  await app.h.waitFor(`account-${A.id}`);
}
// The choice opens from the role's cell; its options are named by their value ("" is "none").
async function pick(role, value, id = A.id) {
  const cell = await app.h.el(`rating-pick-${id}-${role}`);
  assert.ok(await cell.isExisting(), "rating choice must exist");
  await cell.click();
  await app.h.click(`rating-option-${id}-${role}-${value || "none"}`);
}
const shown = (role, id = A.id) => app.h.text(`rating-${id}-${role}`);
async function settings() { await app.h.click("settings-open"); await app.h.waitFor("settings"); }

test("US1.1: every record shows three unset roles", async () => {
  await start();
  assert.ok(await app.h.exists(`ratings-${A.id}`));
  const block = await app.h.el(`ratings-${A.id}`);
  for (const role of roles) assert.ok(await app.h.exists(`rating-pick-${A.id}-${role}`));
  for (const role of roles) assert.equal(await shown(role), "—");
  for (const label of ["Tank", "DPS", "Support"]) assert.ok((await block.getText()).includes(label));
});
test("US1.2: a choice contains the two special values and nine groups of five divisions", async () => {
  await start();
  await (await app.h.el(`rating-pick-${A.id}-tank`)).click();
  await app.h.waitFor(`rating-picker-${A.id}-tank`);
  const groups = await app.browser.execute(id => {
    const picker = document.querySelector(`[data-testid="rating-picker-${id}-tank"]`);
    const prefix = `rating-option-${id}-tank-`;
    return { values: [...picker.querySelectorAll(`[data-testid^="${prefix}"]`)].map(o => o.dataset.testid.slice(prefix.length)), groups: [...picker.querySelectorAll('[role="group"]')].map(g => g.children.length) };
  }, A.id);
  assert.deepEqual(groups.values, ["none", "placement", ...ranks.flatMap(r => [5,4,3,2,1].map(d => `${r}:${d}`))]);
  assert.deepEqual(groups.groups, Array(9).fill(5));
});
test("US1.3: picking Gold 3 displays its name, division and badge", async () => {
  await start(); await pick("tank", "gold:3");
  await app.h.tick(async () => await shown("tank") === "Gold 3", "Gold 3 was not shown");
  assert.ok(await app.h.el(`rating-${A.id}-tank`).$('img[src*="Gold"]').isExisting());
});
test("US1.4: placement and unset display text without a badge", async () => {
  await start(); await pick("tank", "placement");
  await app.h.tick(async () => await shown("tank") === "Placement", "placement was not shown");
  assert.equal(await app.h.el(`rating-${A.id}-tank`).$("img").isExisting(), false);
  await pick("tank", "");
  await app.h.tick(async () => await shown("tank") === "—", "unset was not shown");
  assert.equal(await app.h.el(`rating-${A.id}-tank`).$("img").isExisting(), false);
});
test("US1.5: ratings survive an actual program restart", async () => {
  await start();
  await pick("tank", "gold:3"); await pick("damage", "placement"); await pick("support", "emerald:1");
  await app.restart();
  assert.equal(await shown("tank"), "Gold 3");
  assert.equal(await shown("damage"), "Placement");
  assert.equal(await shown("support"), "Emerald 1");
});
test("US1.6: missing BattleTag or email does not prevent a manual rating", async () => {
  await start({ accounts: [record(A, { battletag: null }), record(B, { email: null })] });
  await pick("tank", "gold:3"); await pick("support", "silver:2", B.id);
  assert.equal(await shown("tank"), "Gold 3"); assert.equal(await shown("support", B.id), "Silver 2");
});
test("US1.7: changing away from Overwatch keeps stored ratings for changing back", async () => {
  await start(); await pick("tank", "gold:3");
  await app.h.click(`game-settings-${A.id}`); await app.h.click(`game-option-${A.id}-none`);
  await app.h.waitGone(`ratings-${A.id}`);
  await app.restart(); await app.h.waitFor(`choose-game-${A.id}`);
  assert.equal(await app.h.exists(`ratings-${A.id}`), false);
  await app.h.click(`choose-game-${A.id}`); await app.h.click(`game-option-${A.id}-overwatch`);
  assert.equal(await shown("tank"), "Gold 3");
});

const body = (tank = { division: "gold", tier: 3 }) => ({ competitive: { pc: { tank, damage: null, support: null } } });
const manual = { tank: { rank: "gold", division: 2, source: "manual" }, damage: null, support: null };
const service = { tank: { rank: "gold", division: 3, source: "service" }, damage: null, support: null };
const invoke = (command, args) => app.browser.execute((command, args) => window.__TAURI_INTERNALS__.invoke(command, args), command, args);
async function autoStart(opts = {}) {
  await start({ overfast: { [A.tag]: { body: body() } }, ...opts });
  await app.h.click("tab-overwatch");
  await settings();
  assert.ok(await app.h.exists("set-auto-ratings"), "automatic refresh control must exist");
  await app.h.click("settings-back");
}
async function enable() { await settings(); await app.h.click("set-auto-ratings"); await app.h.click("settings-back"); }
async function refreshed(count = 1) {
  await app.h.tick(async () => app.overfast.requests.length >= count && !(await invoke("get_state")).syncing, "refresh did not finish");
}
async function sync() { const count = app.overfast.requests.length; await app.h.click("sync"); await refreshed(count + 1); }

test("US2.1: settings show the auto-refresh switch on and explain the service", async () => {
  await autoStart(); await settings();
  assert.equal(await app.h.exists("set-show-ratings"), false);
  assert.equal(await app.h.el("set-auto-ratings").isSelected(), true);
  assert.equal(await app.h.text("ratings-source-note"), "Ratings are taken from the OverFast service by the account's BattleTag");
});
test("US2.2: with automatic refresh off the Overwatch button refreshes only Overwatch accounts once", async () => {
  await autoStart({ settings: { auto_ratings:false }, env: { NOXDECK_RATINGS_MS: "100" },
    accounts: [record(A), record(B, { game: "call_of_duty" })] }); await sleep(250);
  assert.equal(app.overfast.requests.length, 0); assert.equal(await app.h.exists("sync"), true);
  await sync();
  assert.equal(app.overfast.requests.length, 1);
  assert.equal(app.overfast.requests[0].path, "/players/Alpha-1111/summary");
  assert.equal(await shown("tank"), "Gold 3");
  await sleep(250); assert.equal(app.overfast.requests.length, 1);
  await app.h.click("tab-all"); assert.equal(await app.h.exists("sync"), false);
});
test("US2.3: enabling automatic refresh starts it immediately and shows sync", async () => {
  await autoStart({ settings: { auto_ratings:false } }); await enable(); await refreshed();
  assert.ok(await app.h.exists("sync")); assert.equal(await shown("tank"), "Gold 3");
});
test("US2.4: startup, sync and interval refresh every account with a BattleTag", async () => {
  await autoStart({ settings: { auto_ratings: true }, accounts: [record(A),record(B)], env: { NOXDECK_RATINGS_MS: "3000" } });
  await refreshed(2); const before = app.overfast.requests.length;
  await sync(); await refreshed(before + 2);
  await app.h.tick(() => app.overfast.requests.length >= before + 4, "interval did not refresh");
  const paths = new Set(app.overfast.requests.map(r => r.path));
  assert.equal(paths.size, 2);
});
test("US2.5: service ranks show a sync mark", async () => {
  await autoStart({ settings: { auto_ratings:true } }); await refreshed();
  assert.equal(await shown("tank"), "Gold 3"); assert.ok(await app.h.exists(`rating-sync-${A.id}-tank`));
});
test("US2.6: a manual rating has no sync mark", async () => {
  await autoStart({ settings: { auto_ratings:true }, overfast: {} });
  await refreshed(); await pick("tank", "gold:2");
  assert.equal(await app.h.exists(`rating-sync-${A.id}-tank`), false);
});
test("US2.7: sync indicates work and disables a second refresh", async () => {
  await autoStart({ settings: { auto_ratings:false } }); app.overfast.set(A.tag,{ body:body(), delay:600 }); await enable();
  assert.equal(await app.h.el("sync").isEnabled(), false); assert.equal(await app.h.el("sync").getAttribute("aria-label"), "Refreshing ratings…");
  await refreshed(); assert.equal(await app.h.el("sync").isEnabled(), true);
});
test("US2.8: an unavailable or private account keeps its ratings without an error", async () => {
  await autoStart({ accounts:[record(A,{ratings:manual})], settings:{auto_ratings:true}, overfast:{[A.tag]:{status:403}} });
  await refreshed(); assert.equal(await shown("tank"),"Gold 2"); assert.equal(await app.h.exists("message"),false);
});
test("US2.9: a partial service answer changes only the reported role", async () => {
  await autoStart({ accounts:[record(A,{ratings:{...manual,support:{rank:"silver",division:1,source:"manual"}}})], settings:{auto_ratings:true} });
  await refreshed(); assert.equal(await shown("tank"),"Gold 3"); assert.equal(await shown("support"),"Silver 1");
});
test("US2.10: the timer refreshes with the window closed in the tray", async () => {
  await autoStart({ settings:{auto_ratings:true}, env:{NOXDECK_RATINGS_MS:"1200"} }); await refreshed();
  // The window's own cross, as the player uses it: it closes the window to the tray without a system message.
  await app.h.click("win-close");
  // The window is gone: its page no longer answers.
  await app.h.tick(async () => { try { await app.browser.getTitle(); return false; } catch { return true; } }, "the window did not close");
  const before=app.overfast.requests.length;
  for(let n=0;n<60 && app.overfast.requests.length===before;n++) await sleep(100);
  assert.ok(app.overfast.requests.length>before,"tray timer did not refresh");
});
test("US2.11: a non-Overwatch account never sends automatic rating requests", async () => {
  await autoStart({ accounts: [record(A, {game:"none"})], settings:{auto_ratings:true}, env:{NOXDECK_RATINGS_MS:"100"} }); await sleep(250);
  assert.equal(app.overfast.requests.length,0); assert.equal(await app.h.exists("sync"),true);
});
test("US3.1: the service can replace a manual rank with a lower rank", async () => {
  await autoStart({ settings: { auto_ratings:false }, accounts:[record(A,{ratings:manual})] }); await enable(); await refreshed();
  assert.equal(await shown("tank"),"Gold 3"); assert.ok(await app.h.exists(`rating-sync-${A.id}-tank`));
});
test("US3.2: manual editing removes the sync mark until the next service answer", async () => {
  await autoStart({ settings:{auto_ratings:true} }); await refreshed(); await pick("tank","silver:2");
  assert.equal(await shown("tank"),"Silver 2"); assert.equal(await app.h.exists(`rating-sync-${A.id}-tank`),false);
  await sync(); assert.equal(await shown("tank"),"Gold 3"); assert.ok(await app.h.exists(`rating-sync-${A.id}-tank`));
});
test("US3.3: repeated missing reports keep a manual rating", async () => {
  await autoStart({ settings:{auto_ratings:true},accounts:[record(A,{ratings:manual})],overfast:{[A.tag]:{body:body(null)}} });
  await refreshed(); await sync(); await sync(); assert.equal(await shown("tank"),"Gold 2");
});
test("US3.4: disabling auto preserves service values and removes all sync marks", async () => {
  await autoStart({settings:{auto_ratings:true}}); await refreshed();
  await settings(); await app.h.click("set-auto-ratings"); await app.h.click("settings-back");
  assert.equal(await shown("tank"),"Gold 3"); assert.equal(await app.h.exists(`rating-sync-${A.id}-tank`),false);
  await app.restart(); assert.equal(await shown("tank"),"Gold 3"); assert.equal(await app.h.exists("sync"),true);
});
test("US3.5: with auto off, editing a previously synced value stays manual", async () => {
  await autoStart({settings:{auto_ratings:false},accounts:[record(A,{ratings:service})],env:{NOXDECK_RATINGS_MS:"100"}});
  await pick("tank","placement"); await sleep(250);
  assert.equal(await shown("tank"),"Placement"); assert.equal(app.overfast.requests.length,0);
});
