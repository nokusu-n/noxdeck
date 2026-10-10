import { useEffect, useRef, useState } from "react";
import { t } from "../i18n";
import { games, type Role, type Sort, type State, type Tab } from "../core";
import { gameLabel } from "./GamePicker";
import { GameMark } from "../ui/GameMark";
import { roleMark } from "./Ratings";
import { Icon } from "../ui/Icon";

/** The launcher level: the launcher's name as a tab, then the two ways to add an account. */
export function Toolbar({ addDisabled, onAdd, onSave }: {
  addDisabled: boolean;
  onAdd: () => void;
  onSave: () => void;
}) {
  return (
    <div className="tool">
      <nav className="cats">
        <span className="cat is-on" aria-current="page">{t("tabBattle")}</span>
      </nav>
      <div className="tool-gap" />
      <span className="tool-br" />
      <button data-testid="save-current" className="btn btn--save" disabled={addDisabled} title={t("saveCurrentTip")} onClick={onSave}>{t("saveCurrent")}</button>
      <button data-testid="add-account" className="btn btn--primary" disabled={addDisabled} onClick={onAdd}><Icon name="i-plus" size={15} />{t("addAccount")}</button>
    </div>
  );
}

/** The same chip in a narrow window: "Call of Duty" and "WoW: Forever" would not leave room for the sorts. */
const tabShort = (tab: Tab) => tab === "call_of_duty" ? t("tabCodShort") : tab === "world_of_warcraft_forever" ? t("tabForeverShort") : null;
const tabLabel = (tab: Tab) => tab === "all" ? t("allGames") : tab === "world_of_warcraft" ? t("tabWow") : tab === "world_of_warcraft_forever" ? t("tabWowForever") : gameLabel(tab);
const sortRole = (sort: Sort): Role | null => (["tank", "damage", "support"] as const).find(role => sort.startsWith(`${role}_`)) ?? null;

/**
 * The head of the list: one chip per game, the ratings refresh (Overwatch tab only), then the sorts.
 * In a narrow window the sorts fold into one menu button; the buttons inside are the same ones.
 */
export function GameBar({ state, menuOpen = false, onTab, onSort, onRefresh }: { state: State; menuOpen?: boolean; onTab: (tab: Tab) => void; onSort: (sort: Sort) => void; onRefresh: () => void }) {
  const [menu, setMenu] = useState(menuOpen);
  const bar = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!menu) return;
    const away = (e: PointerEvent) => { if (!bar.current?.querySelector(".sortm-wrap")?.contains(e.target as Node)) setMenu(false); };
    const key = (e: KeyboardEvent) => { if (e.key === "Escape") setMenu(false); };
    document.addEventListener("pointerdown", away);
    document.addEventListener("keydown", key);
    return () => { document.removeEventListener("pointerdown", away); document.removeEventListener("keydown", key); };
  }, [menu]);
  const pick = (sort: Sort) => { setMenu(false); onSort(sort); };
  const current = sortRole(state.sort);
  return <div ref={bar} className="gamebar">
    <nav className="gchips" role="tablist" aria-label={t("gameTabs")}>
      {(["all", ...games] as Tab[]).map(tab => {
        const count = tab === "all" ? state.accounts.length : state.accounts.filter(a => a.game === tab).length;
        const label = tabLabel(tab);
        return <button key={tab} data-testid={`tab-${tab}`} className={`gchip${state.tab === tab ? " is-on" : ""}${!count ? " gchip--empty" : ""}`} role="tab" aria-selected={state.tab === tab} title={tab === "all" ? label : gameLabel(tab)} onClick={() => onTab(tab)}>
          {tab !== "all" && <GameMark game={tab} />}<span className="gchip-name">{label}</span>{tabShort(tab) && <span className="gchip-short" aria-hidden="true">{tabShort(tab)}</span>}{count > 0 && <em data-testid={`tab-count-${tab}`}>{count}</em>}
        </button>;
      })}
    </nav>
    {state.tab === "overwatch" && <button data-testid="sync" className="btn btn--icon btn-rfr" disabled={state.syncing} aria-busy={state.syncing} title={t("syncTip")} aria-label={t(state.syncing ? "syncing" : "syncRatings")} onClick={onRefresh}>
      <Icon name="i-refresh" size={16} spin={state.syncing} /><span className="sr-only">{t(state.syncing ? "syncing" : "syncRatings")}</span>
    </button>}
    <div className="sortm-wrap">
      <button className={`btn sortm-b${current ? " is-set" : ""}`} aria-haspopup="menu" aria-expanded={menu} title={`${t("sortLabel")}: ${current ? t(current) : t(state.sort === "battletag" ? "sortBattletag" : "sortLastLogin")}`} onClick={() => setMenu(open => !open)}>
        {current ? <img src={roleMark[current]} alt="" width={16} height={15} /> : <Icon name="i-sort" size={16} />}
        <Icon name="i-chev-down" size={13} />
        <span className="sr-only">{t("sortLabel")}</span>
      </button>
      <div className={`sorts${menu ? " is-open" : ""}`}>
        <div className="seg" role="group" aria-label={t("sortLabel")}>
          <button data-testid="sort-last-login" className={`seg-b${state.sort === "last_login" ? " is-on" : ""}`} aria-pressed={state.sort === "last_login"} onClick={() => pick("last_login")}>{t("sortLastLogin")}</button>
          <button data-testid="sort-battletag" className={`seg-b${state.sort === "battletag" ? " is-on" : ""}`} aria-pressed={state.sort === "battletag"} onClick={() => pick("battletag")}>{t("sortBattletag")}</button>
        </div>
        {state.tab === "overwatch" && <div className="seg seg--role" role="group" aria-label={t("sortRole")}>
          <p className="sorts-h">{t("sortRole")}</p>
          {(["tank", "damage", "support"] as const).map(role => {
            const active = current === role;
            const ascending = state.sort === `${role}_asc`;
            return <button key={role} data-testid={`sort-role-${role}`} data-direction={active ? ascending ? "asc" : "desc" : "none"} className={`seg-b role-sort${active ? " is-on" : ""}`} aria-pressed={active} title={`${t(role)}${active ? `: ${t(ascending ? "rankLowFirst" : "rankHighFirst")}` : ""}`} aria-label={t(role)} onClick={() => pick(`${role}_${active && !ascending ? "asc" : "desc"}`)}>
              <img src={roleMark[role]} alt="" width={16} height={15} /><span className="role-name">{t(role)}</span>{active && <Icon name="i-chev-down" size={12} />}
              {active && <em className="role-dir">{t(ascending ? "rankLowFirst" : "rankHighFirst")}</em>}
            </button>;
          })}
        </div>}
      </div>
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
