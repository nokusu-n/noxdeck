import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { games, type Account, type Game } from "../core";
import { t } from "../i18n";
import { GameMark } from "../ui/GameMark";

export const gameLabel = (game: Game) => t(game === "none" ? "noGame" : game);

export function GamePicker({ account, anchor, container, disabled, onPick, onClose }: {
  account: Account; anchor: HTMLElement; container: HTMLElement; disabled: boolean;
  onPick: (game: Game) => void; onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [at, setAt] = useState({ left: 0, top: 0 });
  useLayoutEffect(() => {
    const menu = ref.current;
    if (!menu) return;
    const w = container.getBoundingClientRect(), b = anchor.getBoundingClientRect();
    const scale = w.width / container.offsetWidth || 1;
    const left = Math.max(8, Math.min((b.right - w.left) / scale - menu.offsetWidth, w.width / scale - menu.offsetWidth - 8));
    const below = (b.bottom - w.top) / scale + 6;
    const top = below + menu.offsetHeight < w.height / scale - 8 ? below : Math.max(8, (b.top - w.top) / scale - menu.offsetHeight - 6);
    setAt({ left, top });
  }, [anchor, container]);
  useEffect(() => {
    ref.current?.querySelector<HTMLElement>("button:not(:disabled)")?.focus();
    const away = (e: PointerEvent) => { if (!ref.current?.contains(e.target as Node) && !anchor.contains(e.target as Node)) onClose(); };
    const key = (e: KeyboardEvent) => { if (e.key === "Escape") { onClose(); anchor.focus(); } };
    document.addEventListener("pointerdown", away);
    document.addEventListener("keydown", key);
    return () => { document.removeEventListener("pointerdown", away); document.removeEventListener("keydown", key); };
  }, [anchor, onClose]);
  const row = (game: Game) => <button key={game} data-testid={`game-option-${account.id}-${game}`} className={`gp-i game-option${account.game === game ? " is-on" : ""}`} role="menuitemradio" aria-checked={account.game === game} disabled={disabled} onClick={() => onPick(game)}>
    <i className="opt-dot" /><span className="gp-mark">{game !== "none" && <GameMark game={game} />}</span><span>{gameLabel(game)}</span>
  </button>;
  return <div ref={ref} data-testid={`game-picker-${account.id}`} className="gp" role="menu" aria-label={t("accountGame")} style={at}>
    <header className="gp-h"><b>{t("accountGame")}</b>{account.battletag && <span>{account.battletag}</span>}</header>
    {games.map(row)}
    <hr />
    {row("none")}
    <p className="gp-note">{disabled ? t("r_game_change_blocked") : t("gameNote")}</p>
  </div>;
}
