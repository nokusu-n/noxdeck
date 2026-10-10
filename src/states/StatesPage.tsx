// Every state of the window at once, from fixed data. Development builds only: while
// `npm run dev` runs, open the dev server's address with #states on the end. Nothing here touches the core.
// ponytail: the captions are Russian literals, outside i18n: they are for the people who review the
// design, not for players. Translate them if a reviewer who does not read Russian ever opens this page.
import { ReactNode, useState } from "react";
import { AccountRecord } from "../components/AccountRecord";
import { DeleteDialog, GameDialog, ReminderDialog } from "../components/Dialogs";
import { Api, MainWindow } from "../components/MainWindow";
import { Lang, setLang, t } from "../i18n";
import type { Account, Role, State } from "../core";
import type { WindowControls } from "../window";
import { base, sample } from "./data";
import "./states.css";

const ok = async () => ({ ok: true as const, value: undefined });
// Stand-ins for the core: every command answers "done" and changes nothing.
const api: Api = {
  setHideEmails: ok,
  setAccountGame: ok,
  setTab: ok,
  saveDiagnostics: async () => ({ ok: true as const, value: "cancelled" as const }),
  setCloseLauncher: ok, openRepository: ok, setRating: ok, setShowRatings: ok, setAutoRatings: ok, syncRatings: ok,
  getState: async () => base, addAccount: ok, cancelOperation: ok, saveCurrent: async () => ({ ok: true as const, value: { id: "a1" } }),
  startLauncher: ok, startGame: ok, deleteAccount: ok, setSort: ok, dismiss: ok, acknowledgeNotice: ok, clearAsk: ok,
  setAutostart: ok, setAfterStart: ok, setShowGameStart: ok, setLanguage: ok, subscribe: async () => () => {},
};
const controls = (maximized = false): WindowControls => ({ minimize() {}, toggleMaximize() {}, close() {}, watchMaximized: (fn) => (fn(maximized), () => {}) });

type Op = NonNullable<State["operation"]>;
const op = (phase: Op["phase"], kind: Op["kind"] = "start", accountId: string | null = "a5", cancellable = false): Op => ({ kind, accountId, phase, cancellable });

function Frame({ w, h, zoom, children }: { w: number; h: number; zoom?: number; children: ReactNode }) {
  return <div className="frame" style={{ width: w, height: h, zoom }}>{children}</div>;
}
function Section({ title, note, children }: { title: string; note?: string; children: ReactNode }) {
  return <section className="sec"><h2>{title}</h2>{note && <p className="note">{note}</p>}{children}</section>;
}
function Shot({ caption, children }: { caption: string; children: ReactNode }) {
  return <figure className="shot"><figcaption>{caption}</figcaption>{children}</figure>;
}

type Case = { caption: string; account?: Partial<Account>; busy?: boolean; loggingIn?: boolean; phase?: Op["phase"]; showGame?: boolean; showRatings?: boolean; preview?: "copied" | "started" };
const cases: Case[] = [
  { caption: "Готов, ничего не запущено" },
  { caption: "Battle.net открыт на этом аккаунте", account: { isCurrent: true } },
  { caption: "Игра запущена, Battle.net закрыт", account: { isInGame: true } },
  { caption: "Battle.net и игра", account: { isCurrent: true, isInGame: true } },
  { caption: "Нужен пароль", account: { loginState: "password_wanted" } },
  { caption: "Нужен код", account: { loginState: "code_wanted" } },
  { caption: "Шаг: закрываем Battle.net", busy: true, loggingIn: true, phase: "stopping" },
  { caption: "Шаг: запускаем Battle.net", busy: true, loggingIn: true, phase: "starting" },
  { caption: "Шаг: ждём Battle.net", busy: true, loggingIn: true, phase: "waiting_login" },
  { caption: "Шаг: ждём запуска игры", busy: true, loggingIn: true, phase: "launching_game" },
  { caption: "Шаг: закрываем Battle.net после игры", busy: true, loggingIn: true, phase: "closing_launcher" },
  { caption: "Шаг: завершаем", busy: true, loggingIn: true, phase: "finishing" },
  { caption: "Результат: игра запущена (три секунды)", account: { isCurrent: true, isInGame: true, lastOutcome: "game_started" }, preview: "started" },
  { caption: "Результат: Overwatch нужно обновить", account: { isCurrent: true, lastOutcome: "game_needs_update" } },
  { caption: "Результат: игра не запустилась", account: { isCurrent: true, lastOutcome: "game_not_started" } },
  { caption: "Результат: Battle.net не ответил", account: { lastOutcome: "no_answer" } },
  { caption: "E-mail неизвестен: кнопки недоступны, причина в подсказке", account: { email: null } },
  { caption: "Идёт другое действие: кнопки недоступны", busy: true },
  { caption: "BattleTag скопирован", preview: "copied" },
  { caption: "Настройки: рейтинг выключен, ячейки остаются", showRatings: false },
  { caption: "Настройки: кнопки игры нет, Launcher на всю ширину", showGame: false },
  { caption: "Настройки: выключено то и другое", showRatings: false, showGame: false },
];

function RecordCase({ c }: { c: Case }) {
  const a: Account = { ...sample, ...c.account };
  return (
    <Shot caption={c.caption}>
      <ul className="cards cards--one">
        <AccountRecord account={a} loggingIn={!!c.loggingIn} phase={c.phase ?? null} busy={!!c.busy} launcherFound
          showGameStart={c.showGame ?? true} showRatings={c.showRatings ?? true} autoRatings preview={c.preview}
          pickingRole={null} onPickRating={() => {}} onStart={() => {}} onDelete={() => {}} onGame={() => {}} />
      </ul>
    </Shot>
  );
}

const trayText = {
  ru: ["Открыть Noxdeck", " — в игре", " — в Battle.net", "Выход"],
  en: ["Open Noxdeck", " — in game", " — in Battle.net", "Exit"],
  it: ["Apri Noxdeck", " — in gioco", " — in Battle.net", "Esci"],
};
const trayLines = (language: Lang) => [
  trayText[language][0],
  `Kestrel#2417${trayText[language][1]}`,
  `Tamsin#6205${trayText[language][2]}`,
  "Marlowe#1093",
  "Quill#8841",
  "Haldor#4059",
  trayText[language][3],
];

export function StatesPage() {
  const [lang, setLanguage] = useState<Lang>("ru");
  setLang(lang);
  const s = (over: Partial<State>): State => ({ ...base, lang, ...over });
  return (
    <div className="states" key={lang}>
      <header className="states-head">
        <b>Noxdeck {base.version} · состояния</b>
        <span>Только для разработки. Данные придуманы, ядро не вызывается.</span>
        <div className="seg" role="group">
          {(["ru", "en", "it"] as const).map((l) => <button key={l} data-testid={`states-lang-${l}`} className={`seg-b${lang === l ? " is-on" : ""}`} aria-pressed={lang === l} onClick={() => setLanguage(l)}>{l.toUpperCase()}</button>)}
        </div>
      </header>

      <Section title="Главное окно" note="960×720. Нажмите на рейтинг: откроется выбор. Нажмите на BattleTag: он копируется.">
        <Frame w={960} h={720}><MainWindow state={s({})} api={api} controls={controls()} /></Frame>
      </Section>
      <Section title="Самое маленькое окно" note="640×480, список прокручивается.">
        <Frame w={640} h={480}><MainWindow state={s({})} api={api} controls={controls()} /></Frame>
      </Section>
      <Section title="Окно на весь экран" note="1440×810, показано в масштабе 66%.">
        <Frame w={1440} h={810} zoom={0.66}><MainWindow state={s({})} api={api} controls={controls(true)} /></Frame>
      </Section>
      <Section title="Главное окно во время запуска" note="960×720. Шаг виден только на карточке и на её кнопке: над списком ничего не появляется, список не сдвигается.">
        <Frame w={960} h={720}><MainWindow state={s({ operation: op("starting") })} api={api} controls={controls()} /></Frame>
      </Section>

      <Section title="Состояния записи" note="Каждое состояние из брифа, на одной и той же записи. Подписи слева не часть программы.">
        <div className="cases">{cases.map((c) => <RecordCase key={c.caption} c={c} />)}</div>
      </Section>

      <Section title="Выбор рейтинга вручную" note="Открыт для роли «Поддержка» первого аккаунта.">
        <Frame w={960} h={720}><MainWindow state={s({})} api={api} controls={controls()} initial={{ picking: { id: "a1", role: "support" as Role } }} /></Frame>
      </Section>

      <Section title="Сообщения и пустые состояния">
        <div className="pair">
          <Shot caption="Пустой список, Battle.net не найден, файл данных сброшен">
            <Frame w={600} h={520}><MainWindow state={s({ accounts: [], launcher: "not_found", notice: "data_file_reset", gameRunning: false })} api={api} controls={controls()} /></Frame>
          </Shot>
          <Shot caption="Добавление аккаунта: ждём вход, и результат прошлого действия">
            <Frame w={600} h={520}><MainWindow state={s({ operation: op("waiting_login", "add", null, true) })} api={api} controls={controls()} initial={{ message: "saved" }} /></Frame>
          </Shot>
          <Shot caption="Запуск: шаг только на карточке и на её кнопке">
            <Frame w={600} h={520}><MainWindow state={s({ operation: op("starting") })} api={api} controls={controls()} initial={{ message: "r_launcher_not_closable" }} /></Frame>
          </Shot>
          <Shot caption="Обновление рейтинга: кнопка в шапке занята">
            <Frame w={600} h={520}><MainWindow state={s({ syncing: true })} api={api} controls={controls()} /></Frame>
          </Shot>
        </div>
      </Section>

      <Section title="Диалоги">
        <div className="pair pair--dialogs">
          <div className="scrim"><ReminderDialog preview testid="remind-dialog" checkTestid="remind-dont-show" continueTestid="remind-continue" title={t("remindTitle")} body={t("remindBody")} onContinue={() => {}} onClose={() => {}} /></div>
          <div className="scrim"><ReminderDialog preview testid="logout-dialog" checkTestid="logout-dont-show" continueTestid="logout-continue" title={t("logoutTitle")} body={t("logoutBody")} onContinue={() => {}} onClose={() => {}} /></div>
          <div className="scrim"><DeleteDialog preview onConfirm={() => {}} onCancel={() => {}} /></div>
          <div className="scrim"><GameDialog preview onConfirm={() => {}} onCancel={() => {}} /></div>
        </div>
      </Section>

      <Section title="Настройки">
        <Frame w={960} h={720}><MainWindow state={s({})} api={api} controls={controls()} initial={{ settingsOpen: true }} /></Frame>
      </Section>

      <Section title="Меню в трее" note="Рисует Windows, стилей у него нет. Наши только тексты.">
        <div className="pair">
          {(["ru", "en", "it"] as const).map((l) => (
            <Shot key={l} caption={l === "ru" ? "Русский" : l === "it" ? "Italiano" : "English"}>
              <ul className="tray">{trayLines(l).map((line, i) => <li key={line} className={i === 0 || i === trayLines(l).length - 1 ? "tray-edge" : undefined}>{line}</li>)}</ul>
            </Shot>
          ))}
        </div>
      </Section>
    </div>
  );
}
