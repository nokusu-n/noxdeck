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

/** Adding an account: there is no record yet, so its panel carries the step and the Cancel button. */
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

/**
 * A start of the launcher or the game shows nothing above the list: the record's own chip and the
 * spinner on its button are the whole progress display. The step is still announced, to screen
 * readers and to the tests that wait on this element, without taking a line of the window.
 */
export function OperationStatus({ phase, title, body }: { phase: string; title: string; body: string }) {
  return (
    <div data-testid="operation" data-phase={phase} className="sr-only" role="status">
      <b>{title}</b>
      <p>{body}</p>
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
