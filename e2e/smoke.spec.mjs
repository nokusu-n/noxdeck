import assert from "node:assert/strict";
import { after, test } from "node:test";
import { startApp, VERSION } from "./lib.mjs";

let app;
after(() => app?.stop());

test("the window opens on an empty list and shows the version", async () => {
  app = await startApp();
  const { h } = app;
  await h.waitFor("empty-state");
  assert.equal(await h.text("version"), VERSION);
  assert.ok(await h.exists("add-account"));
  assert.ok(await h.exists("save-current"));
  assert.equal(await h.exists("launcher-not-found"), false);
});
