import type { ReactNode } from "react";
import { core, Result, State } from "../core";
import { t } from "../i18n";
import { Icon } from "../ui/Icon";

type Api = Pick<typeof core, "setAutostart" | "setAfterStart" | "setShowGameStart" | "setCloseLauncher" | "setShowRatings" | "setAutoRatings" | "setLanguage" | "openRepository">;

function Switch({ testid, label, note, noteTestid, checked, onChange }: {
  testid: string; label: string; note?: string; noteTestid?: string; checked: boolean; onChange: (value: boolean) => void;
}) {
  const noteId = noteTestid ?? `${testid}-note`;
  return (
    <label className="set-row">
      <span className="set-text">
        <span className="set-label">{label}</span>
        {note && <span id={noteId} data-testid={noteTestid} className="set-note">{note}</span>}
      </span>
      <input data-testid={testid} className="switch" type="checkbox" role="switch" checked={checked} aria-describedby={note ? noteId : undefined} onChange={(e) => onChange(e.target.checked)} />
    </label>
  );
}

/** One of a few choices, drawn as a row of options with a radio dot each. */
function Options<T extends string>({ testidPrefix, name, value, choices, onChange }: {
  testidPrefix: string; name: string; value: T; choices: [T, string][]; onChange: (value: T) => void;
}) {
  return (
    <div className="opts" role="radiogroup">
      {choices.map(([v, label]) => (
        <label key={v} className={`opt${value === v ? " is-on" : ""}`}>
          <input data-testid={`${testidPrefix}-${v}`} type="radio" name={name} value={v} checked={value === v} onChange={() => onChange(v)} />
          {label}
        </label>
      ))}
    </div>
  );
}

function Group({ title, children }: { title: string; children: ReactNode }) {
  return <section className="set-group"><h3>{title}</h3>{children}</section>;
}

export function SettingsScreen({ state, api, run }: {
  state: State; api: Api; run: (action: () => Promise<Result<unknown>>) => Promise<void>;
}) {
  const s = state.settings;
  return (
    <section data-testid="settings" className="set">
      <Group title={t("sectionProgram")}>
        <Switch testid="set-autostart" label={t("autostart")} checked={s.autostart} onChange={(v) => run(() => api.setAutostart(v))} />
        <div className="set-row set-row--stack" role="group" aria-labelledby="after-title">
          <span className="set-text">
            <span id="after-title" data-testid="settings-after-title" className="set-label">{t("afterStart")}</span>
            <span className="set-note">{t("afterStartNote")}</span>
          </span>
          <Options testidPrefix="set-after" name="after-start" value={s.afterStart}
            choices={[["stay", t("afterStay")], ["tray", t("afterTray")], ["close", t("afterClose")]]}
            onChange={(v) => run(() => api.setAfterStart(v))} />
        </div>
        <div className="set-row">
          <span className="set-text"><span data-testid="settings-language-title" className="set-label">{t("language")}</span></span>
          {/* Each language is named in itself, so a player can find theirs from any state of the window. */}
          <div className="seg" role="radiogroup" aria-label={t("language")}>
            {([["ru", "languageRu"], ["en", "languageEn"], ["it", "languageIt"]] as const).map(([v, key]) => (
              <label key={v} className={`seg-b${state.lang === v ? " is-on" : ""}`}>
                <input data-testid={`set-lang-${v}`} type="radio" name="language" value={v} checked={state.lang === v} onChange={() => run(() => api.setLanguage(v))} />
                {t(key)}
              </label>
            ))}
          </div>
        </div>
      </Group>
      <Group title={t("sectionOverwatch")}>
        <Switch testid="set-show-game" label={t("showGame")} checked={s.showGameStart} onChange={(v) => run(() => api.setShowGameStart(v))} />
        <Switch testid="set-close-launcher" label={t("closeLauncher")} checked={s.closeLauncher} onChange={(v) => run(() => api.setCloseLauncher(v))} />
        <Switch testid="set-show-ratings" label={t("showRatings")} checked={s.showRatings} onChange={(v) => run(() => api.setShowRatings(v))} />
        <Switch testid="set-auto-ratings" label={t("autoRatings")} note={t("ratingsSourceNote")} noteTestid="ratings-source-note" checked={s.autoRatings} onChange={(v) => run(() => api.setAutoRatings(v))} />
      </Group>
      <footer className="set-foot">
        <p data-testid="developed-by">{t("developedBy")}</p>
        <button data-testid="open-repository" className="btn btn--icon" aria-label={t("openRepository")} title={t("openRepository")} onClick={() => run(api.openRepository)}>
          <Icon name="i-github" size={20} />
        </button>
        <span className="set-ver">{t("version")} {state.version}</span>
      </footer>
    </section>
  );
}
