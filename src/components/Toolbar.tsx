import { t } from "../i18n";
import { games, type Sort, type State, type Tab } from "../core";
import { gameLabel } from "./GamePicker";
import { GameMark } from "../ui/GameMark";
import { Icon } from "../ui/Icon";
import battleNet from "../assets/battlenet-mark.png";

/** The list's controls: the launcher category, the sort switch, and the two ways to add an account. */
export function Toolbar({ addDisabled, onAdd, onSave }: {
  addDisabled: boolean;
  onAdd: () => void;
  onSave: () => void;
}) {
  return (
    <div className="tool">
      <nav className="cats">
        <span className="cat is-on" aria-current="page">
          <img src={battleNet} alt="" width={22} height={22} />
          <span>{t("tabBattle")}</span>
        </span>
      </nav>
      <div className="tool-gap" />
      <span className="tool-br" />
      <button data-testid="save-current" className="btn btn--save" disabled={addDisabled} title={t("saveCurrentTip")} onClick={onSave}>{t("saveCurrent")}</button>
      <button data-testid="add-account" className="btn btn--primary" disabled={addDisabled} onClick={onAdd}><Icon name="i-plus" size={15} />{t("addAccount")}</button>
    </div>
  );
}

export function GameBar({ state, onTab, onSort }: { state: State; onTab: (tab: Tab) => void; onSort: (sort: Sort) => void }) {
  return <div className="gamebar">
    <nav className="game-tabs" role="tablist" aria-label={t("gameTabs")}>
      {(["all", ...games] as Tab[]).map(tab => {
        const count = tab === "all" ? state.accounts.length : state.accounts.filter(a => a.game === tab).length;
        const label = tab === "all" ? t("allGames") : tab === "world_of_warcraft" ? t("tabWow") : tab === "world_of_warcraft_forever" ? t("tabWowForever") : gameLabel(tab);
        return <button key={tab} data-testid={`tab-${tab}`} className={`btn game-tab${state.tab === tab ? " is-on" : ""}${!count ? " is-empty" : ""}`} role="tab" aria-selected={state.tab === tab} title={tab === "all" ? label : gameLabel(tab)} onClick={() => onTab(tab)}>
          {tab !== "all" && <GameMark game={tab} />}{label}{count > 0 && <span data-testid={`tab-count-${tab}`}>{count}</span>}
        </button>;
      })}
    </nav>
    <div className="tool-gap" />
    <div className="seg" role="group" aria-label={t("sortLabel")}>
      <button data-testid="sort-last-login" className={`seg-b${state.sort === "last_login" ? " is-on" : ""}`} aria-pressed={state.sort === "last_login"} onClick={() => onSort("last_login")}>{t("sortLastLogin")}</button>
      <button data-testid="sort-battletag" className={`seg-b${state.sort === "battletag" ? " is-on" : ""}`} aria-pressed={state.sort === "battletag"} onClick={() => onSort("battletag")}>{t("sortBattletag")}</button>
    </div>
  </div>;
}

/** The settings screen's header: back, then the name of the screen. */
export function SettingsHead({ back }: { back: () => void }) {
  return (
    <div className="tool tool--set">
      <button data-testid="settings-back" className="btn back" onClick={back}><Icon name="i-arrow-left" size={16} />{t("back")}</button>
      <h1>{t("settings")}</h1>
    </div>
  );
}
