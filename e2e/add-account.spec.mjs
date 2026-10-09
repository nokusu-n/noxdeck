// User Story 2: add an account by logging in (acceptance scenarios 1-7).
import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import { A, B, C, ORIGINAL_LIST, record, sleep, startApp } from "./lib.mjs";

let app;
afterEach(async () => {
  await app?.stop();
  app = null;
});

const remembered = { dismissed: { remember_login: true, logout_warning: true } };
const known = [{ ...A }, { ...B }, { ...C }];

test("1. a first start: an empty list that offers Add account and Save current", async () => {
  app = await startApp({ scenario: { accounts: known } });
  const { h } = app;
  await h.waitFor("empty-state");
  assert.ok(await h.exists("add-account"));
  assert.ok(await h.exists("save-current"));
  assert.deepEqual(await h.ids(), [], "nothing found on the computer is suggested");
});

test("2. the reminder comes first, can be dismissed for good, and adding proceeds after it", async () => {
  app = await startApp({ scenario: { accounts: known, login_screen: [] } });
  const { h } = app;
  await h.click("add-account");
  await h.waitFor("remind-dialog");
  assert.equal(await h.exists("waiting"), false, "nothing starts before the user continues");
  assert.equal(app.list(), ORIGINAL_LIST, "the launcher's list is untouched meanwhile");
  await h.click("remind-dont-show");
  await h.click("remind-continue");
  await h.waitFor("waiting");
  assert.equal(app.settingsFile().dismissed.remember_login, true);

  await h.click("cancel");
  await h.waitGone("waiting");
  await h.click("add-account");
  await h.waitFor("waiting");
  assert.equal(await h.exists("remind-dialog"), false, "a dismissed reminder does not come back");
  await h.click("cancel");
});

test("3 and 4. the launcher opens on its login screen with nobody saved; a new account appears", async () => {
  app = await startApp({
    settings: remembered,
    scenario: { accounts: known, login_screen: [{ at_ms: 3500, login: C.email }] },
  });
  const { h } = app;
  await h.click("add-account");
  await h.waitFor("waiting");
  await h.waitFor("cancel");
  assert.equal(app.list(), "", "nobody is saved while the launcher waits");

  await h.waitFor(`account-${C.id}`);
  assert.equal(await h.text(`tag-${C.id}`), C.tag);
  assert.equal(await h.text(`email-${C.id}`), C.email);
  await h.waitState(C.id, "ready");
  await h.waitFor(`current-${C.id}`);
  assert.equal(await h.exists(`start-${C.id}`), false);
  await h.waitGone("waiting");
  assert.deepEqual(await h.ids(), [C.id]);
});

test("5. logging into a listed account makes no second record", async () => {
  app = await startApp({
    settings: remembered,
    accounts: [record(A, { login_state: "password_wanted" })],
    scenario: { accounts: known, login_screen: [{ at_ms: 3000, login: A.email }] },
  });
  const { h } = app;
  await h.click("add-account");
  await h.waitFor(`current-${A.id}`);
  await h.waitGone("waiting");
  await h.waitState(A.id, "ready");
  assert.equal(await h.exists(`start-${A.id}`), false);
  assert.deepEqual(await h.ids(), [A.id]);
});

test("6a. closing the launcher without logging in leaves the list and the launcher as they were", async () => {
  app = await startApp({
    settings: remembered,
    scenario: { accounts: known, login_screen: [{ at_ms: 1500, exit: true }] },
  });
  const { h } = app;
  await h.click("add-account");
  await h.waitFor("waiting");
  await h.waitGone("waiting", { timeout: 20000 });
  assert.deepEqual(await h.ids(), []);
  assert.equal(app.list(), ORIGINAL_LIST, "the launcher will log in whoever it would have before");
});

test("6b. Cancel does the same", async () => {
  app = await startApp({ settings: remembered, scenario: { accounts: known } });
  const { h } = app;
  await h.click("add-account");
  await h.waitFor("cancel");
  await sleep(500);
  assert.equal(app.list(), "");
  await h.click("cancel");
  await h.waitGone("waiting", { timeout: 20000 });
  assert.deepEqual(await h.ids(), []);
  assert.equal(app.list(), ORIGINAL_LIST);
});

test("7. an account added earlier still starts after another is added", async () => {
  app = await startApp({
    settings: remembered,
    accounts: [record(A)],
    scenario: { accounts: known, login_screen: [{ at_ms: 2500, login: C.email }] },
  });
  const { h } = app;
  await h.click("add-account");
  await h.waitFor(`account-${C.id}`);
  await h.waitGone("waiting");
  await h.click(`start-${A.id}`);
  await h.waitFor(`current-${A.id}`);
  await h.waitGone(`current-${C.id}`);
  await h.waitState(A.id, "ready");
});

test("a running game does not ask, block or stop when adding an account", async () => {
  app = await startApp({
    settings: remembered,
    accounts: [record(A)],
    scenario: { accounts: known, login_screen: [{ at_ms: 3000, login: C.email }] },
  });
  const { h } = app;
  await h.click(`start-${A.id}`);
  await h.waitFor(`current-${A.id}`);
  await h.waitGone("operation");
  const game = app.startGame();
  await sleep(1000);
  assert.ok(game.pid, "the stand-in game started");
  assert.equal(game.exitCode, null);

  await h.click("add-account");
  assert.equal(await h.exists("game-dialog"), false);
  await h.waitFor("cancel");
  assert.equal(app.list(), "");
  await h.waitFor(`current-${C.id}`);
  await h.waitGone("waiting");
  assert.deepEqual((await h.ids()).sort(), [A.id, C.id]);
  assert.equal(await h.exists(`start-${C.id}`), false);
  assert.ok(await h.exists(`start-${A.id}`));
  assert.equal(game.exitCode, null, "the same game process is still running");
  assert.equal(game.signalCode, null);
});
