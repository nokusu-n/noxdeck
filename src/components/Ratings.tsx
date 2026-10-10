import { Account, Rank, Role } from "../core";
import { t } from "../i18n";
import { Icon } from "../ui/Icon";
import bronze from "../assets/ratings/Badge_1_Bronze.webp";
import silver from "../assets/ratings/Badge_2_Silver.webp";
import gold from "../assets/ratings/Badge_3_Gold.webp";
import platinum from "../assets/ratings/Badge_4_Platinum.webp";
import emerald from "../assets/ratings/Badge_5_Emerald.webp";
import diamond from "../assets/ratings/Badge_6_Diamond.webp";
import master from "../assets/ratings/Badge_7_Master.webp";
import grandmaster from "../assets/ratings/Badge_8_Grandmaster.webp";
import champion from "../assets/ratings/Badge_9_Champion.webp";
import tankMark from "../assets/roles/tank.webp";
import damageMark from "../assets/roles/dps.webp";
import supportMark from "../assets/roles/support.webp";

export const badges: Record<Rank, string> = { bronze, silver, gold, platinum, emerald, diamond, master, grandmaster, champion };
export const roleMark: Record<Role, string> = { tank: tankMark, damage: damageMark, support: supportMark };
export const roles: Role[] = ["tank", "damage", "support"];

function Cell({ account, role, autoRatings, open, onOpen }: {
  account: Account; role: Role; autoRatings: boolean; open: boolean;
  onOpen: (role: Role, anchor: HTMLElement) => void;
}) {
  const v = account.ratings[role];
  const name = t(role);
  const rank = v && v.rank !== "placement" ? v.rank : null;
  const label = !v ? `${name}: ${t("noneLong")}` : rank ? `${name}: ${t(rank)} ${v.division}` : `${name}: ${t("placement")}`;
  return (
    <button type="button" data-testid={`rating-pick-${account.id}-${role}`} className="rt" title={label} aria-haspopup="dialog" aria-expanded={open} onClick={(e) => onOpen(role, e.currentTarget)}>
      <img className="rt-role" src={roleMark[role]} alt="" />
      <span data-testid={`rating-${account.id}-${role}`} className="rt-v">
        {rank ? <img className={`bd bd--${rank}`} src={badges[rank]} alt="" /> : <span className={`bd ${v ? "bd--calib" : "bd--none"}`} />}
        <span className="rtx">
          {rank ? <><span className="rn">{t(rank)}</span>{" "}<b className="rd">{v?.division}</b></>
            : <span className={`rn ${v ? "" : "rn--none"}`}>{v ? t("placementShort") : t("none")}</span>}
        </span>
      </span>
      {autoRatings && v?.source === "service" && (
        <span data-testid={`rating-sync-${account.id}-${role}`} className="sy" title={t("synced")} aria-label={t("synced")}><Icon name="i-refresh" size={11} /></span>
      )}
      <span className="sr-only">{name}</span>
    </button>
  );
}

/** Three role cells on Overwatch accounts. Stored ranks survive a change of game. */
export function Ratings({ account, autoRatings, openRole, onOpen }: {
  account: Account; autoRatings: boolean; openRole: Role | null;
  onOpen: (role: Role, anchor: HTMLElement) => void;
}) {
  if (account.game !== "overwatch") return null;
  return (
    <div data-testid={`ratings-${account.id}`} className="c-rates">
      {roles.map((role) => <Cell key={role} account={account} role={role} autoRatings={autoRatings} open={openRole === role} onOpen={onOpen} />)}
    </div>
  );
}
