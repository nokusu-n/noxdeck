import { ReactNode, useEffect, useRef, useState } from "react";
import { t } from "../i18n";

/**
 * A dialog. In the program it is a real modal (<dialog>): Escape closes it, focus stays inside.
 * `preview` draws the same card inline, for the states page, where four of them sit side by side.
 */
function Modal(props: { testid: string; title: string; onClose: () => void; preview?: boolean; children: ReactNode }) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const d = ref.current;
    if (!d || d.open) return;
    d.showModal();
    // The safe choice holds the focus; a closed <dialog> cannot take it before it is opened.
    d.querySelector<HTMLElement>("[data-focus]")?.focus();
  }, []);
  const content = (
    <>
      <h3>{props.title}</h3>
      {props.children}
    </>
  );
  if (props.preview) return <div data-testid={props.testid} className="dlg" role="dialog" aria-label={props.title}>{content}</div>;
  return (
    <dialog ref={ref} data-testid={props.testid} onClose={props.onClose} className="dlg">
      {content}
    </dialog>
  );
}

/** A reminder with "don't show again" and a continue button (add account, start). */
export function ReminderDialog(props: {
  testid: string;
  checkTestid: string;
  continueTestid: string;
  title: string;
  body: string;
  preview?: boolean;
  onContinue: (dontShowAgain: boolean) => void;
  onClose: () => void;
}) {
  const [checked, setChecked] = useState(false);
  return (
    <Modal testid={props.testid} title={props.title} onClose={props.onClose} preview={props.preview}>
      <p>{props.body}</p>
      <label className="check">
        <input type="checkbox" data-testid={props.checkTestid} checked={checked} onChange={(e) => setChecked(e.target.checked)} />
        {t("dontShowAgain")}
      </label>
      <div className="dlg-btns">
        <button data-testid={props.continueTestid} className="btn btn--primary" onClick={() => props.onContinue(checked)}>{t("continue")}</button>
      </div>
    </Modal>
  );
}

export function DeleteDialog(props: { onConfirm: () => void; onCancel: () => void; preview?: boolean }) {
  return (
    <Modal testid="delete-dialog" title={t("deleteTitle")} onClose={props.onCancel} preview={props.preview}>
      <p>{t("deleteBody")}</p>
      <div className="dlg-btns">
        <button data-testid="delete-confirm" className="btn btn--danger" onClick={props.onConfirm}>{t("confirm")}</button>
        <button data-testid="delete-cancel" className="btn" onClick={props.onCancel}>{t("cancel")}</button>
      </div>
    </Modal>
  );
}

export function GameDialog(props: { onConfirm: () => void; onCancel: () => void; preview?: boolean }) {
  return (
    <Modal testid="game-dialog" title={t("gameQuestion")} onClose={props.onCancel} preview={props.preview}>
      <div className="dlg-btns">
        <button data-testid="game-yes" className="btn btn--danger" onClick={props.onConfirm}>{t("yes")}</button>
        <button data-testid="game-no" className="btn" data-focus onClick={props.onCancel}>{t("no")}</button>
      </div>
    </Modal>
  );
}
