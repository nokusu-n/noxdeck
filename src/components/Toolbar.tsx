import { t } from "../i18n";
import type { Sort } from "../core";
import { Icon } from "../ui/Icon";
import battleNet from "../assets/battlenet-mark.png";

/** The list's controls: the launcher category, the sort switch, and the two ways to add an account. */
export function Toolbar({ sort, onSort, addDisabled, onAdd, onSave }: {
  sort: Sort;
  onSort: (sort: Sort) => void;
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
      <div className="seg" role="group">
        <button data-testid="sort-last-login" className={`seg-b${sort === "last_login" ? " is-on" : ""}`} aria-pressed={sort === "last_login"} onClick={() => onSort("last_login")}>{t("sortLastLogin")}</button>
        <button data-testid="sort-battletag" className={`seg-b${sort === "battletag" ? " is-on" : ""}`} aria-pressed={sort === "battletag"} onClick={() => onSort("battletag")}>{t("sortBattletag")}</button>
      </div>
      <span className="tool-br" />
      <button data-testid="save-current" className="btn btn--save" disabled={addDisabled} title={t("saveCurrentTip")} onClick={onSave}>{t("saveCurrent")}</button>
      <button data-testid="add-account" className="btn btn--primary" disabled={addDisabled} onClick={onAdd}><Icon name="i-plus" size={15} />{t("addAccount")}</button>
    </div>
  );
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
