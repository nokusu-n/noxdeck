// Typed wrappers over the core's commands and its `state` event (contracts/ui-core.md).
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type Refusal =
  | "repository_open_failed"
  | "invalid_rating"
  | "autostart_failed"
  | "busy"
  | "launcher_not_found"
  | "launcher_not_closable"
  | "no_saved_login"
  | "already_listed"
  | "no_email"
  | "already_open"
  | "already_in_game"
  | "game_running"
  | "game_not_closable"
  | "unknown_account";

export type LoginState = "ready" | "password_wanted" | "code_wanted";
export type Sort = "last_login" | "battletag";
export const ranks = ["bronze", "silver", "gold", "platinum", "emerald", "diamond", "master", "grandmaster", "champion"] as const;
export type Rank = typeof ranks[number];
export type Role = "tank" | "damage" | "support";
export type RatingValue = { rank: Rank | "placement"; division?: number };
export type Rating = RatingValue & { source: "manual" | "service" };

export type Account = {
  ratings: Record<Role, Rating | null>;
  id: string;
  battletag: string | null;
  email: string | null;
  loginState: LoginState;
  lastLoginAt: string | null;
  isCurrent: boolean;
  isInGame: boolean;
  lastOutcome: null | "no_answer" | "game_started" | "game_needs_update" | "game_not_started";
};

export type State = {
  syncing: boolean;
  version: string;
  launcher: "not_found" | "closed" | "login_screen" | "logged_in";
  gameRunning: boolean;
  operation: null | {
    kind: "add" | "start" | "game";
    accountId: string | null;
    phase: "stopping" | "starting" | "waiting_login" | "finishing" | "launching_game" | "closing_launcher";
    cancellable: boolean;
  };
  accounts: Account[];
  sort: Sort;
  dismissed: { rememberLogin: boolean; logoutWarning: boolean };
  notice: null | "data_file_reset";
  lang: "ru" | "en" | "it";
  settings: {
    closeLauncher: boolean;
    showRatings: boolean;
    autoRatings: boolean;
    autostart: boolean;
    afterStart: "stay" | "tray" | "close";
    showGameStart: boolean;
    language: null | "ru" | "en" | "it";
  };
  ask: null | { kind: "close_game"; accountId: string };
};

export type Result<T> = { ok: true; value: T } | { ok: false; refusal: Refusal };

async function call<T = void>(cmd: string, args?: Record<string, unknown>): Promise<Result<T>> {
  try {
    return { ok: true, value: await invoke<T>(cmd, args) };
  } catch (e) {
    return { ok: false, refusal: e as Refusal };
  }
}

export const core = {
  saveDiagnostics: () => call<"saved" | "cancelled" | "failed">("save_diagnostics"),
  setCloseLauncher: (value: boolean) => call("set_close_launcher", { value }),
  openRepository: () => call("open_repository"),
  setRating: (id: string, role: Role, value: RatingValue | null) => call("set_rating", { id, role, value }),
  setShowRatings: (value: boolean) => call("set_show_ratings", { value }),
  setAutoRatings: (value: boolean) => call("set_auto_ratings", { value }),
  syncRatings: () => call("sync_ratings"),
  getState: () => invoke<State>("get_state"),
  addAccount: () => call("add_account"),
  cancelOperation: () => call("cancel_operation"),
  saveCurrent: () => call<{ id: string }>("save_current"),
  startLauncher: (id: string) => call("start_launcher", { id }),
  startGame: (id: string, closeGame: boolean) => call("start_game", { id, closeGame }),
  deleteAccount: (id: string) => call("delete_account", { id }),
  setSort: (sort: Sort) => call("set_sort", { sort }),
  dismiss: (what: "rememberLogin" | "logoutWarning") => call("dismiss", { what }),
  acknowledgeNotice: () => call("acknowledge_notice"),
  clearAsk: () => call("clear_ask"),
  setAutostart: (value: boolean) => call("set_autostart", { value }),
  setAfterStart: (value: State["settings"]["afterStart"]) => call("set_after_start", { value }),
  setShowGameStart: (value: boolean) => call("set_show_game_start", { value }),
  setLanguage: (value: State["settings"]["language"]) => call("set_language", { value }),
  /** Calls `fn` with every state the core sends. Returns the function that stops it. */
  subscribe: async (fn: (s: State) => void) => listen<State>("state", (e) => fn(e.payload)),
};
