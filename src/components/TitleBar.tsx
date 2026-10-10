import { useEffect, useState } from "react";
import { t } from "../i18n";
import { Icon } from "../ui/Icon";
import type { WindowControls } from "../window";
import mark from "../assets/noxdeck-mark.png";

/** The window's own title bar: brand, the rating sync, settings, then the three window controls. */
export function TitleBar({ version, settingsOpen, onSettings, controls }: {
  version: string;
  settingsOpen: boolean;
  onSettings: () => void;
  controls: WindowControls;
}) {
  const [maximized, setMaximized] = useState(false);
  useEffect(() => controls.watchMaximized(setMaximized), [controls]);
  return (
    <div className="tbar" data-tauri-drag-region>
      <div className="tb-brand" data-tauri-drag-region>
        <img src={mark} alt="" width={18} height={18} />
        <b>{t("appName")}</b>
        <span className="tb-ver" data-testid="version">{version}</span>
      </div>
      <div className="tb-gap" data-tauri-drag-region />
      <button data-testid="settings-open" className="tb-btn" aria-pressed={settingsOpen} title={t("settings")} onClick={onSettings}>
        <Icon name="i-gear" size={17} />
        <span className="sr-only">{t("settings")}</span>
      </button>
      <i className="tb-sep" />
      <button data-testid="win-min" className="wc" title={t("minimize")} aria-label={t("minimize")} onClick={controls.minimize}>
        <svg className="wi" width="10" height="10" aria-hidden="true"><use href="#w-min" /></svg>
      </button>
      <button data-testid="win-max" className="wc" title={t(maximized ? "restore" : "maximize")} aria-label={t(maximized ? "restore" : "maximize")} onClick={controls.toggleMaximize}>
        <svg className="wi" width="10" height="10" aria-hidden="true"><use href={maximized ? "#w-restore" : "#w-max"} /></svg>
      </button>
      <button data-testid="win-close" className="wc wc--close" title={t("closeToTray")} aria-label={t("closeToTray")} onClick={controls.close}>
        <svg className="wi" width="10" height="10" aria-hidden="true"><use href="#w-close" /></svg>
      </button>
    </div>
  );
}
