// User Story 1: switch the launcher to another account (acceptance scenarios 1-9).
import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import { A, B, record, sleep, startApp } from "./lib.mjs";

let app;
afterEach(async () => {
  await app?.stop();
  app = null;
});

const quiet = { dismissed: { remember_login: true, logout_warning: true } };
const both = [record(A), record(B)];

test("1. closed launcher, saved login: logging in, then ready with the mark", async () => {
  app = await startApp({
    accounts: both,
    settings: quiet,
    scenario: { accounts: [{ ...A }, { ...B }], login_delay_ms: 1500 },
  });
  const { h } = app;
  await h.waitFor(`account-${A.id}`);
  await h.click(`start-${A.id}`);
  await h.waitState(A.id, "logging_in");
  assert.ok(await h.exists("operation"), "the interface shows which action runs");
  await h.waitState(A.id, "ready");
  await h.waitFor(`current-${A.id}`);
  assert.equal(await h.exists(`start-${A.id}`), false);
  assert.ok(await h.exists(`start-${B.id}`));
  assert.equal(await h.exists(`current-${B.id}`), false);
  assert.equal(await h.exists("message"), false, "no error is shown");
  assert.equal(app.list(), A.email, "the launcher was given a one-entry list");
});

test("2. open on A, start B: the launcher is reopened on B and the mark moves", async () => {
  app = await startApp({ accounts: both, settings: quiet, scenario: { accounts: [{ ...A }, { ...B }] } });
  const { h } = app;
  await h.click(`start-${A.id}`);
  await h.waitFor(`current-${A.id}`);
  await h.tick(async () => !(await h.exists("operation")), "the first start never ended");
  await h.click(`start-${B.id}`);
  await h.waitFor(`current-${B.id}`);
  await h.waitGone(`current-${A.id}`);
  assert.equal(await h.exists(`start-${B.id}`), false);
  assert.ok(await h.exists(`start-${A.id}`));
  assert.equal(app.list(), B.email);
  await h.waitState(B.id, "ready");
});

test("3. a saved login that no longer works ends as password wanted, without an error", async () => {
  app = await startApp({
    accounts: both,
    settings: quiet,
    scenario: { accounts: [{ ...A, mode: "login_screen" }, { ...B }] },
  });
  const { h } = app;
  await h.click(`start-${A.id}`);
  await h.waitState(A.id, "password_wanted");
  assert.equal(await h.exists("message"), false);
  assert.equal(await h.exists(`current-${A.id}`), false);
});

test("4. a code screen ends as code wanted, without an error", async () => {
  app = await startApp({ accounts: both, settings: quiet, scenario: { accounts: [{ ...A, mode: "code" }, { ...B }] } });
  const { h } = app;
  await h.click(`start-${A.id}`);
  await h.waitState(A.id, "code_wanted");
  assert.equal(await h.exists("message"), false);
});

test("5. a login completed on the login screen turns the record ready with the mark", async () => {
  app = await startApp({
    accounts: both,
    settings: quiet,
    scenario: {
      accounts: [{ ...A, mode: "login_screen" }, { ...B }],
      login_screen: [{ at_ms: 3500, login: A.email }],
    },
  });
  const { h } = app;
  await h.click(`start-${A.id}`);
  await h.waitState(A.id, "password_wanted");
  await h.waitState(A.id, "ready");
  await h.waitFor(`current-${A.id}`);
});

test("6. a running game does not ask, block or stop when switching accounts", async () => {
  app = await startApp({ accounts: both, settings: quiet, scenario: { accounts: [{ ...A }, { ...B }] } });
  const { h } = app;
  await h.click(`start-${A.id}`);
  await h.waitFor(`current-${A.id}`);
  await h.waitGone("operation");
  const game = app.startGame();
  await sleep(1000);
  assert.ok(game.pid, "the stand-in game started");
  assert.equal(game.exitCode, null);

  await h.click(`start-${B.id}`);
  assert.equal(await h.exists("game-dialog"), false);
  await h.waitFor(`current-${B.id}`);
  await h.waitGone("operation");
  assert.equal(app.list(), B.email);
  assert.equal(game.exitCode, null, "the same game process is still running");
  assert.equal(game.signalCode, null);
});

test("9. the current record replaces Start launcher; closing or logging out restores it", async () => {
  app = await startApp({ accounts: both, settings: quiet, scenario: { accounts: [{ ...A }, { ...B }] } });
  const { h } = app;
  await h.click(`start-${A.id}`);
  await h.waitFor(`current-${A.id}`);
  await h.waitGone("operation");
  assert.equal(await h.exists(`start-${A.id}`), false);
  assert.ok(await h.exists(`start-${B.id}`));

  app.killLauncher();
  await h.waitGone(`current-${A.id}`);
  await h.waitFor(`start-${A.id}`);
  assert.equal(await h.el(`start-${A.id}`).isEnabled(), true);

  app.setScenario({ accounts: [{ ...A }, { ...B }], after_login: [{ at_ms: 3000, logout: true }] });
  await h.click(`start-${A.id}`);
  await h.waitFor(`current-${A.id}`);
  assert.equal(await h.exists(`start-${A.id}`), false);
  await h.waitState(A.id, "password_wanted");
  await h.waitGone(`current-${A.id}`);
  await h.waitFor(`start-${A.id}`);
  assert.equal(await h.el(`start-${A.id}`).isEnabled(), true);
});

test("7. the logout warning appears until it is dismissed, and the choice is kept", async () => {
  app = await startApp({
    accounts: both,
    settings: { dismissed: { remember_login: true, logout_warning: false } },
    scenario: { accounts: [{ ...A }, { ...B }] },
  });
  const { h } = app;
  await h.click(`start-${A.id}`);
  await h.waitFor("logout-dialog");
  assert.equal(await h.exists("operation"), false, "nothing starts before the user continues");
  await h.click("logout-dont-show");
  await h.click("logout-continue");
  await h.waitFor(`current-${A.id}`);
  assert.equal(app.settingsFile().dismissed.logout_warning, true);

  await h.tick(async () => !(await h.exists("operation")), "the start never ended");
  await h.click(`start-${B.id}`);
  await sleep(500);
  assert.equal(await h.exists("logout-dialog"), false, "a dismissed warning does not come back");
  await h.waitFor(`current-${B.id}`);
});

test("8. no answer within the waiting time: the record says so and keeps its state", async () => {
  app = await startApp({ accounts: both, settings: quiet, scenario: { accounts: [{ ...A, mode: "silent" }, { ...B }] } });
  const { h } = app;
  await h.click(`start-${A.id}`);
  await h.waitState(A.id, "logging_in");
  await h.waitState(A.id, "no_answer", { timeout: 15000 });
  assert.equal(await h.exists("message"), false);
  assert.equal(app.accountsFile().accounts.find((a) => a.id === A.id).login_state, "ready", "the stored state is unchanged");
});
