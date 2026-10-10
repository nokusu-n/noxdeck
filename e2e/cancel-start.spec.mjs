import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
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
  await h.click(`game-${A.id}`);
  await h.waitGone("operation");
  assert.equal(await h.text(`game-${A.id}`), "Overwatch");
  assert.equal(await h.text(`state-${A.id}`), "Ready");
  app.setScenario({ accounts: [A], game_launch: "stays" });
  await h.click(`start-${A.id}`);
  await h.waitFor(`current-${A.id}`);
});
