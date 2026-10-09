// User Story 3: save the account the launcher is on now (acceptance scenarios 1-4).
// The launcher is made to be "open on an account" by starting it from the app on a
// listed account, then the list is emptied or left to show what Save current does.
import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import { A, B, record, startApp } from "./lib.mjs";

let app;
afterEach(async () => {
  await app?.stop();
  app = null;
});

const quiet = { dismissed: { remember_login: true, logout_warning: true } };
const known = [{ ...A }, { ...B }];

/** Open the launcher on A through the app, then forget A from the list. */
async function openOnAThenForget() {
  const { h } = app;
  await h.click(`start-${A.id}`);
  await h.waitFor(`current-${A.id}`);
  await h.tick(async () => !(await h.exists("operation")), "the start never ended");
  await h.click(`delete-${A.id}`);
  await h.click("delete-confirm");
  await h.waitFor("empty-state");
}

test("1. open and logged in: the account is saved without restarting the launcher", async () => {
  app = await startApp({ accounts: [record(A, { email: A.email })], settings: quiet, scenario: { accounts: known } });
  await openOnAThenForget();
  const { h } = app;
  const listBefore = app.list();
  await h.click("save-current");
  await h.waitFor(`account-${A.id}`);
  assert.equal(await h.text(`tag-${A.id}`), A.tag);
  assert.equal(await h.text(`email-${A.id}`), A.email);
  await h.waitState(A.id, "ready");
  await h.waitFor(`current-${A.id}`);
  assert.equal(app.list(), listBefore, "nothing was written to the launcher");
  assert.equal(await h.exists("operation"), false, "the launcher was not restarted");
});

test("2. closed after a session without log-out: the account it was on is saved", async () => {
  app = await startApp({ accounts: [record(A)], settings: quiet, scenario: { accounts: known } });
  await openOnAThenForget();
  const { h } = app;
  // Close the launcher the way the player would: the stand-in exits by itself.
  app.setScenario({ accounts: known });
  const { execFileSync } = await import("node:child_process");
  execFileSync("powershell", ["-NoProfile", "-Command", `Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith('${app.dirs.install}', 'OrdinalIgnoreCase') } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }`]);
  await h.waitGone(`current-${A.id}`);
  await h.click("save-current");
  await h.waitFor(`account-${A.id}`);
  assert.deepEqual(await h.ids(), [A.id], "only that one");
});

test("3. on the login screen, after a log-out, or never logged in: no account with a saved login", async () => {
  // Never logged in: no launcher log exists at all.
  app = await startApp({ settings: quiet, scenario: { accounts: known } });
  const { h } = app;
  await h.click("save-current");
  await h.waitFor("message");
  assert.match(await h.text("message"), /saved login|сохранённым входом/);
  assert.deepEqual(await h.ids(), []);
});

test("3b. the launcher open on its login screen", async () => {
  app = await startApp({
    accounts: [record(A)],
    settings: quiet,
    scenario: { accounts: [{ ...A, mode: "login_screen" }, { ...B }] },
  });
  const { h } = app;
  await h.click(`start-${A.id}`);
  await h.waitState(A.id, "password_wanted");
  await h.tick(async () => !(await h.exists("operation")), "the start never ended");
  await h.click(`delete-${A.id}`);
  await h.click("delete-confirm");
  await h.waitFor("empty-state");
  await h.click("save-current");
  await h.waitFor("message");
  assert.deepEqual(await h.ids(), []);
});

test("4. an account already in the list: the message, and the list is unchanged", async () => {
  app = await startApp({ accounts: [record(A)], settings: quiet, scenario: { accounts: known } });
  const { h } = app;
  await h.click(`start-${A.id}`);
  await h.waitFor(`current-${A.id}`);
  await h.tick(async () => !(await h.exists("operation")), "the start never ended");
  await h.click("save-current");
  await h.waitFor("message");
  assert.match(await h.text("message"), /already in the list|уже в списке/);
  assert.deepEqual(await h.ids(), [A.id]);
});
