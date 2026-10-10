import { useState } from "react";
import type { Account, Role, State } from "../core";
import { lang, phaseKey, t } from "../i18n";
import { Icon, OverwatchMark } from "../ui/Icon";
import { Ratings } from "./Ratings";

type Phase = NonNullable<State["operation"]>["phase"];

type Props = {
  account: Account;
  /** This record's start is running. */
  loggingIn: boolean;
  /** The step of the action running on this record, if one is. */
  phase: Phase | null;
  /** Some launcher action runs, so no other starts. */
  busy: boolean;
  launcherFound: boolean;
  showGameStart: boolean;
  showRatings: boolean;
  autoRatings: boolean;
  /** The role whose picker is open on this record. */
  pickingRole: Role | null;
  onPickRating: (role: Role, anchor: HTMLElement) => void;
  onStart: () => void;
  onDelete: () => void;
  onGame: () => void;
  /** The states page keeps a note on screen that fades by itself in the program. */
  preview?: "copied" | "started";
};

type Tone = "ok" | "warn" | "bad" | "busy";
/** What the record looks like right now: its state id (for the tests), the mark, and the chip that says it in words. */
type Look = { id: string; kind: Tone | "idle"; mark: string; chip: { tone: Tone; text: string; title?: string } | null; spin?: boolean };

function lookOf(a: Account, loggingIn: boolean, phase: Phase | null, started: boolean): Look {
  if (loggingIn) {
    return { id: phase === "launching_game" || phase === "closing_launcher" ? phase : "logging_in", kind: "busy", mark: "m-busy", spin: true, chip: { tone: "busy", text: phase ? t(phaseKey[phase]) : t("stateLoggingIn") } };
  }
  if (a.lastOutcome === "no_answer") return { id: "no_answer", kind: "warn", mark: "m-warn", chip: { tone: "warn", text: t("stateNoAnswer") } };
  if (a.lastOutcome === "game_needs_update") return { id: "game_needs_update", kind: "warn", mark: "m-update", chip: { tone: "warn", text: t("stateGameNeedsUpdate"), title: t("stateGameNeedsUpdateTip") } };
  if (a.lastOutcome === "game_not_started") return { id: "game_not_started", kind: "warn", mark: "m-warn", chip: { tone: "warn", text: t("stateGameNotStarted"), title: t("stateGameNotStartedTip") } };
  if (a.loginState === "password_wanted") return { id: "password_wanted", kind: "bad", mark: "m-pw", chip: { tone: "bad", text: t("statePassword") } };
  if (a.loginState === "code_wanted") return { id: "code_wanted", kind: "bad", mark: "m-code", chip: { tone: "bad", text: t("stateCode") } };
  if (started) return { id: "ready", kind: "ok", mark: "m-started", chip: { tone: "ok", text: t("stateGameStarted") } };
  if (a.isCurrent || a.isInGame) return { id: "ready", kind: "ok", mark: "m-live", chip: null };
  return { id: "ready", kind: "idle", mark: "m-idle", chip: null };
}

export function AccountRecord({ account: a, loggingIn, phase, busy, launcherFound, showGameStart, showRatings, autoRatings, pickingRole, onPickRating, onStart, onDelete, onGame, preview }: Props) {
  const [copied, setCopied] = useState(preview === "copied");
  const started = a.lastOutcome === "game_started";
  const look = lookOf(a, loggingIn, phase, started);

  async function copyTag() {
    if (!a.battletag) return;
    try {
      await navigator.clipboard.writeText(a.battletag);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // The clipboard refused (no focus); nothing was copied, so nothing is confirmed.
    }
  }

  const startBlocked = busy || !launcherFound || !a.email;
  const reason = !a.email ? t("startNoEmail") : busy ? t("startBusy") : undefined;
  const lastLogin = a.lastLoginAt ? `${t("lastLogin")}: ${new Date(a.lastLoginAt).toLocaleString(lang)}` : undefined;
  const wide = showGameStart ? "" : " btn--wide";
  return (
    <li data-testid={`account-${a.id}`} className={`card card--${look.kind}${look.kind === "ok" ? " is-live" : ""}`}>
      <div className="c-top">
        <span className="mark" aria-hidden="true"><svg className={`mk${look.spin ? " spin" : ""}`} width="16" height="16"><use href={`#${look.mark}`} /></svg></span>
        <div className="who" title={lastLogin}>
          <div className="l1">
            {a.battletag ? (
              <button data-testid={`tag-${a.id}`} className="tag" title={t("copyTag")} onClick={copyTag}>{a.battletag}</button>
            ) : (
              <span data-testid={`tag-${a.id}`} className="tag tag--none">{t("none")}</span>
            )}
            {copied && <span data-testid={`copied-${a.id}`} className="copied"><Icon name="i-check" size={12} />{t("copied")}</span>}
          </div>
          <div className="l2">
            {/* The state in words: a chip when it needs attention, otherwise a label for screen readers only. */}
            <span data-testid={`state-${a.id}`} data-state={look.id} className={look.chip ? `chip chip--${look.chip.tone}` : "sr-only"} title={look.chip?.title}>
              {look.chip ? look.chip.text : t("stateReady")}
            </span>
            <span data-testid={`email-${a.id}`} className={a.email ? "mail" : "mail mail--missing"} title={a.email ?? t("noEmail")}>{a.email ?? t("noEmail")}</span>
          </div>
        </div>
        <button data-testid={`delete-${a.id}`} className="btn btn--icon btn-del" disabled={busy} title={t("delete")} onClick={onDelete}>
          <Icon name="i-trash" size={16} />
          <span className="sr-only">{t("delete")}</span>
        </button>
      </div>
      <Ratings account={a} shown={showRatings} autoRatings={autoRatings} openRole={pickingRole} onOpen={onPickRating} />
      <div className="c-acts">
        {a.isCurrent ? (
          <button data-testid={`current-${a.id}`} className={`btn btn--act is-on${wide}`} disabled aria-current="true" title={t("openInLauncherTip")}>{t("start")}</button>
        ) : (
          <button data-testid={`start-${a.id}`} className={`btn btn--act${wide}`} disabled={startBlocked} title={reason ?? t("startTip")} onClick={onStart}>{t("start")}</button>
        )}
        {showGameStart && (a.isInGame ? (
          <button data-testid={`ingame-${a.id}`} className="btn btn--act is-on" disabled aria-current="true" title={t("inGameTip")}><OverwatchMark />{t("startGame")}</button>
        ) : (
          <button data-testid={`game-${a.id}`} className="btn btn--act" disabled={startBlocked} title={reason ?? t("startGameTip")} onClick={onGame}><OverwatchMark />{t("startGame")}</button>
        ))}
      </div>
    </li>
  );
}
