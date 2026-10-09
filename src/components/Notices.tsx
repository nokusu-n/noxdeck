import type { ReactNode } from "react";
import { t } from "../i18n";
import { Icon } from "../ui/Icon";
import mark from "../assets/noxdeck-mark.png";

type Tone = "warn" | "bad" | "ok" | "info";
const icon: Record<Tone, string> = { warn: "i-alert", bad: "i-alert", ok: "i-check", info: "i-info" };

/** A message over the list: an icon, a sentence, and sometimes one action. */
export function Banner({ tone, testid, children, action }: { tone: Tone; testid: string; children: ReactNode; action?: ReactNode }) {
  return (
    <div data-testid={testid} className={`banner banner--${tone}`} role={tone === "ok" || tone === "info" ? "status" : undefined}>
      <Icon name={icon[tone]} size={18} />
      <p>{children}</p>
      {action}
    </div>
  );
}

/** A launcher action is running, and the player may have to do something (or cancel). */
export function WaitPanel({ testid, phase, title, body, onCancel }: { testid: string; phase: string; title: string; body: string; onCancel: (() => void) | null }) {
  return (
    <div data-testid={testid} data-phase={phase} className="wait">
      <svg className="mk spin" width="22" height="22" aria-hidden="true"><use href="#m-busy" /></svg>
      <div>
        <b>{title}</b>
        <p>{body}</p>
      </div>
      {onCancel && <button data-testid="cancel" className="btn" onClick={onCancel}>{t("cancel")}</button>}
    </div>
  );
}

export function EmptyState() {
  return (
    <div data-testid="empty-state" className="empty">
      <img src={mark} alt="" width={36} height={36} />
      <b>{t("emptyTitle")}</b>
      <p>{t("emptyHint")}</p>
    </div>
  );
}
