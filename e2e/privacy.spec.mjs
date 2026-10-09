// Constitution III / FR-030: after the main flows, nothing the app printed holds a
// launcher log line, an e-mail or a BattleTag.
import assert from "node:assert/strict";
import { after, test } from "node:test";
import { A, B, C, record, startApp } from "./lib.mjs";

let app;
after(() => app?.stop());

test("the app prints no launcher line, e-mail or BattleTag", async () => {
  app = await startApp({
    accounts: [record(A)],
    settings: { dismissed: { remember_login: true, logout_warning: true } },
    scenario: { accounts: [{ ...A }, { ...B }, { ...C }], login_screen: [{ at_ms: 2500, login: C.email }] },
  });
  const { h } = app;
  await h.click(`start-${A.id}`);
  await h.waitFor(`current-${A.id}`);
  await h.tick(async () => !(await h.exists("operation")), "the start never ended");
  await h.click("add-account");
  await h.waitFor(`account-${C.id}`);
  await h.tick(async () => !(await h.exists("operation")), "the add never ended");
  await h.click("save-current");

  const printed = app.output();
  for (const secret of ["@example.com", "#1111", "#3333", "Logged into", "bnet=1:0", "UAuth", "cached auth token", "browser state"]) {
    assert.equal(printed.includes(secret), false, `the output contains "${secret}"`);
  }
});
