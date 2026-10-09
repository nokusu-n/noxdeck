import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { Account, Rank, ranks, RatingValue, Role } from "../core";
import { t } from "../i18n";
import { Icon } from "../ui/Icon";
import { badges, roleMark } from "./Ratings";

const divisions = [5, 4, 3, 2, 1];

/**
 * The hand choice for one role of one account: two special values, then nine ranks by five
 * divisions. A popover over the window, never a modal: it sits against the cell that opened it,
 * flips above it when there is no room below, and stays inside the window.
 */
export function RatingPicker({ account, role, anchor, container, onPick, onClose }: {
  account: Account; role: Role; anchor: HTMLElement; container: HTMLElement;
  onPick: (value: RatingValue | null) => void; onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [at, setAt] = useState<{ left: number; top: number } | null>(null);
  const v = account.ratings[role];
  const id = account.id;
  const option = (value: string) => `rating-option-${id}-${role}-${value}`;

  useLayoutEffect(() => {
    const pick = ref.current;
    if (!pick) return;
    const w = container.getBoundingClientRect();
    const b = anchor.getBoundingClientRect();
    const z = w.width / container.offsetWidth || 1; // the window may be drawn scaled (the states page)
    const room = 8;
    const width = pick.offsetWidth, height = pick.offsetHeight;
    const ww = w.width / z, wh = w.height / z;
    const left = Math.max(room, Math.min((b.left - w.left) / z + b.width / z / 2 - width / 2, ww - width - room));
    const below = (b.bottom - w.top) / z + 6;
    const above = (b.top - w.top) / z - height - 6;
    const top = below + height <= wh - room ? below : above >= room ? above : Math.max(room, wh - height - room);
    setAt({ left, top });
  }, [anchor, container]);

  useEffect(() => {
    ref.current?.querySelector<HTMLElement>(".is-on, .pk-div")?.focus();
    const away = (e: PointerEvent) => {
      const target = e.target as Node;
      if (!ref.current?.contains(target) && !anchor.contains(target)) onClose();
    };
    const key = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      onClose();
      anchor.focus();
    };
    document.addEventListener("pointerdown", away);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("pointerdown", away);
      document.removeEventListener("keydown", key);
    };
  }, [anchor, onClose]);

  const special = (value: "none" | "placement", on: boolean, children: ReactNode) => (
    <button type="button" data-testid={option(value)} className={`chip-btn${on ? " is-on" : ""}`} aria-pressed={on} onClick={() => onPick(value === "none" ? null : { rank: "placement" })}>{children}</button>
  );

  return (
    <div ref={ref} data-testid={`rating-picker-${id}-${role}`} className="pk" role="dialog" aria-label={t(role)} style={{ left: at?.left ?? 0, top: at?.top ?? 0, visibility: at ? "visible" : "hidden" }}>
      <header className="pk-head">
        <img src={roleMark[role]} alt="" width={20} height={18} />
        <div><b>{t(role)}</b><span>{account.battletag ?? account.email ?? t("none")}</span></div>
        <button type="button" className="btn btn--icon" title={t("close")} aria-label={t("close")} onClick={onClose}><Icon name="i-x" size={14} /></button>
      </header>
      <div className="pk-special">
        {special("none", !v, <>{t("none")}<span>{t("noneLong")}</span></>)}
        {special("placement", v?.rank === "placement", t("placement"))}
      </div>
      {ranks.map((rank: Rank) => (
        <div key={rank} className="pk-row">
          <img src={badges[rank]} alt="" />
          <span className="pk-rn">{t(rank)}</span>
          <div className="pk-divs" role="group" aria-label={t(rank)}>
            {divisions.map((d) => {
              const on = v?.rank === rank && v.division === d;
              return <button key={d} type="button" data-testid={option(`${rank}:${d}`)} className={`pk-div${on ? " is-on" : ""}`} aria-pressed={on} aria-label={`${t(rank)} ${d}`} onClick={() => onPick({ rank, division: d })}>{d}</button>;
            })}
          </div>
        </div>
      ))}
    </div>
  );
}
