import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { once } from "node:events";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, test } from "node:test";
import { APP, sleep, startApp } from "./lib.mjs";

let app;
afterEach(async () => { await app?.stop(); app = null; });

test("window size and place survive the tray and a program restart while hidden", async () => {
  app = await startApp();
  await app.h.waitFor("empty-state");
  const pid = await app.browser.execute(() => window.__TAURI_INTERNALS__.invoke("test_process_id"));
  const native = (processId, resize = false) => JSON.parse(execFileSync("powershell", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "e2e/window-state.ps1", "-ProcessId", String(processId), "-AppPath", APP, ...(resize ? ["-Resize"] : [])], { windowsHide: true }).toString());
  const initial = native(pid, true);
  const before = initial.normal;
  assert.equal(initial.visible, false, "the test window stays hidden");
  assert.equal(before.width, 800);
  assert.equal(before.height, 600);
  await app.h.click("win-close");
  const path = join(app.dirs.data, "window.json");
  for (let i = 0; i < 100 && !existsSync(path); i++) await sleep(50);
  assert.ok(existsSync(path), "the window is saved when it goes to the tray");
  const saved = JSON.parse(readFileSync(path, "utf8"));
  assert.deepEqual(saved.normal, before);
  assert.equal(saved.maximized, false);

  // Another isolated start signals the same core, even while its webview is destroyed.
  const reopen = spawn(APP, [], { env: app.env, stdio: "ignore", windowsHide: true });
  assert.equal((await once(reopen, "exit"))[0], 0);
  let handles = [];
  for (let i = 0; i < 100 && !handles.length; i++) {
    handles = await app.browser.getWindowHandles();
    if (!handles.length) await sleep(50);
  }
  assert.equal(handles.length, 1);
  await app.browser.switchToWindow(handles[0]);
  await app.h.waitFor("empty-state");
  const returned = native(pid);
  assert.equal(returned.visible, false);
  assert.deepEqual(returned.normal, before);
  assert.equal(await app.browser.execute(() => window.__TAURI_INTERNALS__.invoke("test_process_id")), pid);
  await app.restart();
  await app.h.waitFor("empty-state");
  const restarted = native(await app.browser.execute(() => window.__TAURI_INTERNALS__.invoke("test_process_id")));
  assert.equal(restarted.visible, false);
  assert.deepEqual(restarted.normal, before);
});
