import { useEffect, useRef, useState } from "react";
import { AccountRecord } from "./AccountRecord";
import { SettingsScreen } from "./SettingsScreen";
import { DeleteDialog, GameDialog, ReminderDialog } from "./Dialogs";
import { Banner, EmptyState, OperationStatus, WaitPanel } from "./Notices";
import { RatingPicker } from "./RatingPicker";
import { SettingsHead, Toolbar } from "./Toolbar";
import { TitleBar } from "./TitleBar";
import type { core, RatingValue, Refusal, Result, Role, State } from "../core";
import { Key, phaseKey, t } from "../i18n";
import type { WindowControls } from "../window";

/** What the window asks of the core. The program passes `core`; the states page passes stand-ins. */
export type Api = typeof core;

type Dialog =
  | { kind: "remind" }
  | { kind: "logout"; id: string }
  | { kind: "delete"; id: string }
  | { kind: "game"; id: string };

type Picking = { id: string; role: Role; anchor: HTMLElement };

/** The whole window: title bar, toolbar, and below them the notices, the list or the settings. */
export function MainWindow({ state, api, controls, initial }: {
  state: State;
  api: Api;
  controls: WindowControls;
  /** For the states page only: start with a screen, a message or a picker already open. */
  initial?: { settingsOpen?: boolean; message?: string; picking?: { id: string; role: Role } };
}) {
  const [message, setMessage] = useState<string | null>(initial?.message ?? null);
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(initial?.settingsOpen ?? false);
  const [picking, setPicking] = useState<Picking | null>(null);
  const root = useRef<HTMLElement>(null);

  useEffect(() => {
    if (!initial?.picking) return;
    const { id, role } = initial.picking;
    const anchor = root.current?.querySelector<HTMLElement>(`[data-testid="rating-pick-${id}-${role}"]`);
    if (anchor) setPicking({ id, role, anchor });
  }, [initial?.picking]);

  const busy = state.operation !== null;
  const found = state.launcher !== "not_found";

  // An unexpected error (not a Refusal) is shown as it is rather than swallowed.
  const say = (r: Refusal) => setMessage(`r_${r}`);

  async function run(action: () => Promise<Result<unknown>>) {
    const r = await action();
    if (r.ok) return setMessage(null);
    say(r.refusal);
  }

  async function saveCurrent() {
    const r = await api.saveCurrent();
    if (r.ok) setMessage("saved");
    else say(r.refusal);
  }

  async function startGame(id: string, closeGame = false) {
    const r = await api.startGame(id, closeGame);
    if (r.ok) setMessage(null);
    else if (r.refusal === "game_running") setDialog({ kind: "game", id });
    else say(r.refusal);
  }

  async function remove(id: string) {
    setDialog(null);
    const r = await api.deleteAccount(id);
    if (!r.ok) say(r.refusal);
  }

  function openPicker(id: string, role: Role, anchor: HTMLElement) {
    setPicking((now) => (now?.id === id && now.role === role ? null : { id, role, anchor }));
  }

  function pick(value: RatingValue | null) {
    if (!picking) return;
    const { id, role } = picking;
    setPicking(null);
    run(() => api.setRating(id, role, value));
  }

  const add = state.operation?.kind === "add";
  const gameQuestionId = state.ask?.accountId ?? (dialog?.kind === "game" ? dialog.id : null);
  const pickedAccount = picking ? state.accounts.find((a) => a.id === picking.id) : undefined;
  const sync = state.settings.showRatings && state.settings.autoRatings
    ? { busy: state.syncing, onClick: () => run(api.syncRatings) }
    : null;

  return (
    <main ref={root} className="win">
      <TitleBar
        version={state.version}
        sync={sync}
        settingsOpen={settingsOpen}
        onSettings={() => { setPicking(null); setSettingsOpen(true); }}
        controls={controls}
      />
      {settingsOpen ? (
        <SettingsHead back={() => setSettingsOpen(false)} />
      ) : (
        <Toolbar
          sort={state.sort}
          onSort={(sort) => api.setSort(sort)}
          addDisabled={busy || !found}
          onAdd={() => (state.dismissed.rememberLogin ? run(api.addAccount) : setDialog({ kind: "remind" }))}
          onSave={saveCurrent}
        />
      )}

      <div className="body">
        <div className="stack">
          {state.notice && (
            <Banner tone="warn" testid="notice" action={<button data-testid="notice-ack" className="btn" onClick={() => api.acknowledgeNotice()}>{t("ok")}</button>}>
              {t("noticeReset")}
            </Banner>
          )}
          {!found && <Banner tone="bad" testid="launcher-not-found">{t("launcherNotFound")}</Banner>}
          {state.operation?.kind === "add" && (
            <WaitPanel
              testid="waiting"
              phase={state.operation.phase}
              title={t("waitingTitle")}
              body={state.operation.phase === "waiting_login" ? t("waitingBody") : t(phaseKey[state.operation.phase])}
              onCancel={state.operation.cancellable ? () => api.cancelOperation() : null}
            />
          )}
          {state.operation && state.operation.kind !== "add" && (
            <OperationStatus
              phase={state.operation.phase}
              title={t(state.operation.kind === "game" ? "operationGame" : "operationStart")}
              body={t(phaseKey[state.operation.phase])}
            />
          )}
          {message && <Banner tone={message === "saved" ? "ok" : "info"} testid="message">{t(message as Key) ?? message}</Banner>}
        </div>

        {settingsOpen ? (
          <SettingsScreen state={state} api={api} run={run} />
        ) : state.accounts.length === 0 ? (
          <EmptyState />
        ) : (
          <ul className="cards">
            {state.accounts.map((a) => (
              <AccountRecord
                key={a.id}
                account={a}
                loggingIn={state.operation?.kind !== "add" && state.operation?.accountId === a.id}
                phase={state.operation && state.operation.kind !== "add" && state.operation.accountId === a.id ? state.operation.phase : null}
                operationKind={state.operation?.kind !== "add" ? state.operation?.kind : null}
                onCancel={() => api.cancelOperation()}
                busy={busy}
                launcherFound={found}
                showGameStart={state.settings.showGameStart}
                showRatings={state.settings.showRatings}
                autoRatings={state.settings.autoRatings}
                pickingRole={picking?.id === a.id ? picking.role : null}
                onPickRating={(role, anchor) => openPicker(a.id, role, anchor)}
                onStart={() =>
                  state.dismissed.logoutWarning ? run(() => api.startLauncher(a.id)) : setDialog({ kind: "logout", id: a.id })
                }
                onDelete={() => setDialog({ kind: "delete", id: a.id })}
                onGame={() => startGame(a.id)}
              />
            ))}
          </ul>
        )}
      </div>

      {picking && pickedAccount && root.current && (
        <RatingPicker account={pickedAccount} role={picking.role} anchor={picking.anchor} container={root.current} onPick={pick} onClose={() => setPicking(null)} />
      )}

      {dialog?.kind === "remind" && (
        <ReminderDialog
          testid="remind-dialog"
          checkTestid="remind-dont-show"
          continueTestid="remind-continue"
          title={t("remindTitle")}
          body={t("remindBody")}
          onClose={() => setDialog(null)}
          onContinue={(dontShow) => {
            if (dontShow) api.dismiss("rememberLogin");
            setDialog(null);
            run(api.addAccount);
          }}
        />
      )}
      {dialog?.kind === "logout" && (
        <ReminderDialog
          testid="logout-dialog"
          checkTestid="logout-dont-show"
          continueTestid="logout-continue"
          title={t("logoutTitle")}
          body={t("logoutBody")}
          onClose={() => setDialog(null)}
          onContinue={(dontShow) => {
            if (dontShow) api.dismiss("logoutWarning");
            const id = dialog.id;
            setDialog(null);
            run(() => api.startLauncher(id));
          }}
        />
      )}
      {dialog?.kind === "delete" && (
        <DeleteDialog onConfirm={() => remove(dialog.id)} onCancel={() => setDialog(null)} />
      )}
      {gameQuestionId && (
        <GameDialog
          onCancel={() => {
            if (state.ask) api.clearAsk();
            setDialog(null);
          }}
          onConfirm={() => {
            const id = gameQuestionId;
            setDialog(null);
            startGame(id, true);
          }}
        />
      )}
    </main>
  );
}
