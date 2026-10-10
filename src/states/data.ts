// Fixed data for the states page. Invented accounts only: no real e-mail or BattleTag appears here.
import type { Account, Rank, Rating, State } from "../core";

const svc = (rank: Rank, division: number): Rating => ({ rank, division, source: "service" });
const man = (rank: Rank, division: number): Rating => ({ rank, division, source: "manual" });
const placement: Rating = { rank: "placement", source: "manual" };

const account = (id: string, battletag: string, email: string | null, over: Partial<Account>, ratings: Account["ratings"]): Account => ({
  game: "overwatch", id, battletag, email, loginState: "ready", lastLoginAt: "2026-10-05T12:00:00Z", isCurrent: false, isInGame: false, lastOutcome: null, ratings, ...over,
});

/** The nine accounts the main window shows: every kind of state and every shape of rating. */
export const accounts: Account[] = [
  account("a1", "Kestrel#2417", "kestrel.mail@example.com", { isCurrent: true, isInGame: true }, { tank: svc("diamond", 2), damage: svc("platinum", 4), support: svc("gold", 3) }),
  account("a2", "Tamsin#6205", "tamsin.k@example.com", { lastOutcome: "game_needs_update" }, { tank: svc("gold", 1), damage: null, support: svc("silver", 5) }),
  account("a3", "Marlowe#1093", "marlowe.r@example.net", { loginState: "password_wanted" }, { tank: man("bronze", 2), damage: null, support: null }),
  account("a4", "Quill#8841", "quill.games@example.org", {}, { tank: null, damage: svc("emerald", 3), support: svc("emerald", 5) }),
  account("a5", "Haldor#4059", "haldor.s@example.com", {}, { tank: svc("master", 2), damage: svc("grandmaster", 4), support: svc("champion", 5) }),
  account("a6", "Corvus#3372", "corvus.alt@example.com", {}, { tank: placement, damage: null, support: man("platinum", 2) }),
  account("a7", "Isolde#7716", null, {}, { tank: null, damage: svc("diamond", 5), support: null }),
  account("a8", "Pip#9120", "pip.9120@example.com", {}, { tank: man("bronze", 5), damage: man("silver", 1), support: null }),
  account("a9", "Fenwick#5588", "fenwick.t@example.com", { lastOutcome: "no_answer" }, { tank: svc("platinum", 5), damage: svc("gold", 2), support: svc("gold", 4) }),
];

const none: Account["ratings"] = { tank: null, damage: null, support: null };

/** Eleven accounts of every game, newest login first: what the tabs sort and filter. */
export const mixedAccounts: Account[] = [
  accounts[0],
  account("b1", "Brindle#2204", "brindle.d@example.com", { game: "call_of_duty" }, none),
  accounts[3],
  account("b2", "Sable#6631", "sable.w@example.net", { game: "world_of_warcraft" }, none),
  account("b3", "Wren#1480", "wren.p@example.org", { game: "none" }, none),
  accounts[8],
  account("b4", "Linden#7302", "linden.o@example.com", { game: "world_of_warcraft_forever", isCurrent: true }, none),
  accounts[1],
  accounts[2],
  accounts[4],
  accounts[7],
];

export const base: State = {
  tab: "all", visibleAccountIds: accounts.map(a => a.id),
  syncing: false,
  version: "0.7.0",
  launcher: "logged_in",
  gameRunning: true,
  operation: null,
  accounts,
  sort: "last_login",
  dismissed: { rememberLogin: true, logoutWarning: true },
  notice: null,
  lang: "ru",
  settings: { hideEmails: false, closeLauncher: true, autoRatings: true, autostart: true, afterStart: "tray", language: null },
  ask: null,
};

/** The record the "one record" states are drawn on: a plain account with a full set of ratings. */
export const sample: Account = accounts[4];
