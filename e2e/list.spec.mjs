// User Story 4: read and keep the list (acceptance scenarios 1-8), the
// launcher-not-found edge case, and both languages on every element.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { afterEach, test } from "node:test";
import { A, B, C, record, sleep, startApp } from "./lib.mjs";

let app;
afterEach(async () => {
  await app?.stop();
  app = null;
});

const quiet = { dismissed: { remember_login: true, logout_warning: true } };
const known = [{ ...A }, { ...B }, { ...C }];
const three = [
  record(A, { last_login_at: "2026-10-01T10:00:00Z" }),
  record(B, { last_login_at: "2026-10-05T10:00:00Z", battletag: "alpha#2" }),
  record(C, { last_login_at: null }),
];

const clipboard = () => execFileSync("powershell", ["-NoProfile", "-Command", "Get-Clipboard"]).toString().trim();

test("1. a record shows the BattleTag, the e-mail and the login state", async () => {
  app = await startApp({ accounts: [record(A), record(B, { login_state: "password_wanted" }), record(C, { login_state: "code_wanted" })], settings: quiet });
  const { h } = app;
  await h.waitFor(`account-${A.id}`);
  assert.equal(await h.text(`tag-${A.id}`), A.tag);
  assert.equal(await h.text(`email-${A.id}`), A.email);
  await h.waitState(A.id, "ready");
  await h.waitState(B.id, "password_wanted");
  await h.waitState(C.id, "code_wanted");
});

test("2. clicking the BattleTag copies it and confirms briefly", async () => {
  app = await startApp({ accounts: [record(A)], settings: quiet });
  const { h } = app;
  await h.click(`tag-${A.id}`);
  await h.waitFor(`copied-${A.id}`);
  assert.equal(clipboard(), A.tag);
  await h.waitGone(`copied-${A.id}`);
});

test("3. sorting by last login or by BattleTag, and the choice survives a restart", async () => {
  app = await startApp({ accounts: three, settings: quiet });
  const { h } = app;
  await h.waitFor(`account-${A.id}`);
  assert.deepEqual(await h.ids(), [B.id, A.id, C.id], "most recent first, never last");
  await h.click("sort-battletag");
  // Case-insensitive: "alpha#1111" < "alpha#2" < "charlie#3333".
  await h.tick(async () => (await h.ids()).join() === [A.id, B.id, C.id].join(), "BattleTag order");
  assert.equal(app.settingsFile().sort, "battletag");

  await app.restart();
  await h.waitFor(`account-${A.id}`);
  assert.equal(await h.el("sort-battletag").getAttribute("aria-pressed"), "true");
  assert.deepEqual(await h.ids(), [A.id, B.id, C.id]);
});

test("4. delete asks first; declining changes nothing, confirming removes only the record", async () => {
  app = await startApp({ accounts: three, settings: quiet, scenario: { accounts: known } });
  const { h } = app;
  const listBefore = app.list();
  await h.click(`delete-${A.id}`);
  await h.waitFor("delete-dialog");
  await h.click("delete-cancel");
  await h.waitGone("delete-dialog");
  assert.equal((await h.ids()).length, 3);

  await h.click(`delete-${A.id}`);
  await h.click("delete-confirm");
  await h.waitGone(`account-${A.id}`);
  assert.equal((await h.ids()).length, 2);
  assert.equal(app.accountsFile().accounts.some((a) => a.id === A.id), false);
  assert.equal(app.list(), listBefore, "the launcher's list is untouched");
  assert.equal(execFileSync("powershell", ["-NoProfile", "-Command", `(Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith('${app.dirs.install}', 'OrdinalIgnoreCase') } | Measure-Object).Count`]).toString().trim(), "0", "no launcher was started");
});

test("5. a launcher started outside the program: the mark follows it, and goes when it closes", async () => {
  app = await startApp({ accounts: [record(A), record(B)], settings: quiet, list: A.email, scenario: { accounts: known } });
  const { h } = app;
  app.startLauncherOutside();
  await h.waitFor(`current-${A.id}`, { timeout: 5000 });
  assert.equal(await h.exists(`start-${A.id}`), false);
  assert.equal(await h.exists(`current-${B.id}`), false);
  app.killLauncher();
  await h.waitGone(`current-${A.id}`, { timeout: 5000 });
  await h.waitFor(`start-${A.id}`);
});

test("6. a log-out inside the launcher turns the record to password wanted and drops the mark", async () => {
  app = await startApp({
    accounts: [record(A)],
    settings: quiet,
    list: A.email,
    scenario: { accounts: known, after_login: [{ at_ms: 3500, logout: true }] },
  });
  const { h } = app;
  app.startLauncherOutside();
  await h.waitFor(`current-${A.id}`);
  await h.waitState(A.id, "password_wanted", { timeout: 5000 });
  await h.waitGone(`current-${A.id}`);
  await h.waitFor(`start-${A.id}`);
});

test("7. a login of an unlisted account creates no record", async () => {
  app = await startApp({ accounts: [record(A)], settings: quiet, list: C.email, scenario: { accounts: known } });
  const { h } = app;
  app.startLauncherOutside();
  await sleep(3500);
  assert.deepEqual(await h.ids(), [A.id]);
  assert.equal(await h.exists(`current-${A.id}`), false);
  assert.equal(await h.exists("empty-state"), false);
});

test("8. after a restart the list, the states, the sort and the dismissed reminders are as they were", async () => {
  app = await startApp({
    accounts: [record(A), record(B, { login_state: "password_wanted" })],
    settings: { sort: "battletag", dismissed: { remember_login: true, logout_warning: false } },
  });
  const { h } = app;
  await h.waitFor(`account-${A.id}`);
  await h.click("sort-last-login");
  await h.click(`delete-${B.id}`);
  await h.click("delete-confirm");
  await h.waitGone(`account-${B.id}`);
  await app.restart();
  await h.waitFor(`account-${A.id}`);
  assert.deepEqual(await h.ids(), [A.id]);
  assert.equal(await h.el("sort-last-login").getAttribute("aria-pressed"), "true");
  assert.deepEqual(app.settingsFile().dismissed, { remember_login: true, logout_warning: false });
  await h.click("add-account");
  await h.waitFor("waiting");
  assert.equal(await h.exists("remind-dialog"), false, "the dismissed reminder stays dismissed");
  await h.click("cancel");
});

test("edge: the launcher is not found: the list is readable and the launcher actions are unavailable", async () => {
  app = await startApp({ accounts: [record(A)], settings: quiet, noInstall: true });
  const { h } = app;
  await h.waitFor("launcher-not-found");
  assert.equal(await h.text(`tag-${A.id}`), A.tag);
  assert.equal(await h.el("add-account").isEnabled(), false);
  assert.equal(await h.el("save-current").isEnabled(), false);
  assert.equal(await h.el(`start-${A.id}`).isEnabled(), false);
});

// ---- both languages ---------------------------------------------------------

const cyrillic = /[А-Яа-я]/;

async function checkLanguage(lang) {
  // The record's two buttons keep their names in both languages ("Launcher", "Overwatch"): their tooltips are what is translated.
  const named = /^(start|current|ingame)-|^game-(?!dialog$|yes$|no$)/;
  const check = async (h, ids) => {
    for (const id of ids) {
      const text = await h.text(id);
      assert.ok(text.length > 0, `${lang}: ${id} has text`);
      // The settings name each language in itself, so "Русский" is there in either window language.
      const words = (named.test(id) ? await h.el(id).getAttribute("title") : text).replace("Русский", "");
      assert.equal(cyrillic.test(words), lang === "ru", `${lang}: ${id} is in the right language: "${words}"`);
    }
  };
  app = await startApp({
    lang,
    accounts: [record(A), record(B)],
    scenario: { accounts: known, login_screen: [], readiness: "playable", game_launch: "stays" },
  });
  let { h } = app;
  assert.equal(await app.browser.getTitle(), "Noxdeck");
  const seen = new Set();
  const see = async (ids) => {
    for (const id of ids) {
      await h.waitFor(id);
      seen.add(id);
    }
    // A checkbox has no text of its own: its label is part of its dialog, which is checked.
    await check(h, ids.filter((i) => !i.startsWith("tag-") && !i.startsWith("email-") && i !== "version" && !i.endsWith("-dont-show")));
  };

  await see(["version", "add-account", "save-current", "sort-last-login", "sort-battletag", `account-${A.id}`, `tag-${A.id}`, `email-${A.id}`, `state-${A.id}`, `start-${A.id}`, `delete-${A.id}`]);
  await see(["settings-open"]);
  await h.click("settings-open");
  await see(["settings", "settings-back", "settings-after-title", "settings-language-title", "developed-by", "ratings-source-note"]);
  assert.ok((await h.text("developed-by")).includes("Nokusu.dev"));
  const repository = await h.el("open-repository");
  assert.ok(await repository.isExisting());
  assert.ok(await repository.$("svg").isExisting());
  assert.equal(await repository.getText(), "");
  assert.equal(cyrillic.test(await repository.getAttribute("aria-label")), lang === "ru");
  assert.equal(await repository.getAttribute("title"), await repository.getAttribute("aria-label"));
  for (const id of ["set-autostart", "set-after-stay", "set-after-tray", "set-after-close", "set-show-game", "set-show-ratings", "set-auto-ratings", "set-close-launcher"]) {
    await h.waitFor(id); seen.add(id);
    const label = await app.browser.execute(id => document.querySelector(`[data-testid="${id}"]`).closest("label").textContent, id);
    assert.equal(cyrillic.test(label), lang === "ru", `${lang}: ${id} label is translated`);
  }
  // Each language is named in itself, in either window language, so a player can always find theirs.
  for (const [id, name] of [["set-lang-ru", "Русский"], ["set-lang-en", "English"]]) {
    await h.waitFor(id); seen.add(id);
    assert.equal(await app.browser.execute(id => document.querySelector(`[data-testid="${id}"]`).closest("label").textContent.trim(), id), name);
  }
  if (process.env.NOXDECK_CAPTURE) {
    for (const [width, height] of [[960, 720], [640, 480]]) {
      await app.browser.setWindowSize(width, height);
      await app.browser.saveScreenshot(`.tmp-runs/003/ui-${lang}-${width}.png`);
    }
    await app.browser.setWindowSize(960, 720);
  }
  await h.click("settings-back");
  for (const role of ["tank", "damage", "support"]) {
    await (await h.el(`rating-pick-${A.id}-${role}`)).click();
    await h.waitFor(`rating-picker-${A.id}-${role}`);
    const names = await app.browser.execute((id, role) => [...document.querySelectorAll(`[data-testid="rating-picker-${id}-${role}"] [role="group"]`)].map(g => g.getAttribute("aria-label")), A.id, role);
    for (const name of names) assert.equal(cyrillic.test(name), lang === "ru");
    await h.click(`rating-option-${A.id}-${role}-placement`);
    await check(h,[`rating-${A.id}-${role}`]);
  }
  app.overfast.set(A.tag, { body: {competitive:{pc:{tank:{division:"gold",tier:3}}}}, delay:500 });
  await h.click("sync");
  await see(["sync"]);
  await h.waitFor(`rating-sync-${A.id}-tank`);
  assert.equal(cyrillic.test(await h.el(`rating-sync-${A.id}-tank`).getAttribute("aria-label")),lang === "ru");
  await check(h,[`rating-${A.id}-tank`]);
  await h.click(`tag-${A.id}`);
  await see([`copied-${A.id}`]);
  await h.click("save-current");
  await see(["message"]);

  await h.click("add-account");
  await see(["remind-dialog", "remind-dont-show", "remind-continue"]);
  await h.click("remind-continue");
  await see(["waiting", "cancel"]);
  await h.click("cancel");
  await h.waitGone("waiting", { timeout: 20000 });

  await h.click(`start-${A.id}`);
  await see(["logout-dialog", "logout-dont-show", "logout-continue"]);
  await h.click("logout-continue");
  await see([`current-${A.id}`]);
  assert.equal(await h.exists(`start-${A.id}`), false);
  await h.tick(async () => !(await h.exists("operation")), "the start never ended");

  await h.click(`delete-${A.id}`);
  await see(["delete-dialog", "delete-confirm", "delete-cancel"]);
  await h.click("delete-cancel");

  app.startGame();
  await sleep(1000);
  await h.click(`start-${B.id}`);
  await h.click("logout-continue");
  assert.equal(await h.exists("game-dialog"), false);
  await see([`current-${B.id}`, `start-${A.id}`]);
  assert.equal(await h.exists(`start-${B.id}`), false);
  await h.waitGone("operation");
  await see([`game-${B.id}`]);
  await h.click(`game-${B.id}`);
  await see(["game-dialog", "game-yes", "game-no"]);
  await h.click("game-yes");
  await h.waitState(B.id, "closing_launcher");
  await see(["operation", `state-${B.id}`]);
  await h.waitGone("operation");
  await see([`ingame-${B.id}`]);
  assert.match(await h.text(`state-${B.id}`), lang === "ru" ? /Игра запущена/ : /Game started/);
  app.killGame(); await h.waitFor(`game-${B.id}`);
  for (const [readiness, game_launch, outcome, text] of [
    ["not_playable", "stays", "game_needs_update", lang === "ru" ? "Обновите игру" : "Update the game"],
    ["playable", "ignores", "game_not_started", lang === "ru" ? "Игра не запустилась" : "Game didn’t start"],
  ]) {
    app.setScenario({ accounts: known, readiness, game_launch });
    await h.click(`game-${A.id}`); await h.waitState(A.id, outcome); await h.waitGone("operation");
    assert.equal(await h.text(`state-${A.id}`), text);
    app.killLauncher(); await h.waitGone(`current-${A.id}`);
  }
  await app.stop();

  // The empty list, the missing launcher and the data-file notice.
  app = await startApp({ lang, noInstall: true, rawAccounts: "garbage" });
  h = app.h;
  await see(["empty-state", "launcher-not-found", "notice", "notice-ack"]);
  return seen;
}

for (const lang of ["en", "ru"]) {
  test(`both languages: every element of the contract is there and in ${lang}`, async () => {
    const seen = await checkLanguage(lang);
    const contract = ["empty-state", "add-account", "save-current", "message", "remind-dialog", "remind-dont-show", "remind-continue", "waiting", "cancel", "sort-last-login", "sort-battletag", "delete-dialog", "delete-confirm", "delete-cancel", "logout-dialog", "logout-dont-show", "logout-continue", "launcher-not-found", "version", "settings-open", "settings", "settings-back", "set-autostart", "set-after-stay", "set-after-tray", "set-after-close", "set-show-game", "set-lang-ru", "set-lang-en"];
    for (const id of contract) assert.ok(seen.has(id), `${id} was seen`);
  });
}

