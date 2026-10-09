// The window's own controls. The title bar draws them; the window does the work.
import { getCurrentWindow } from "@tauri-apps/api/window";

export type WindowControls = {
  minimize: () => void;
  toggleMaximize: () => void;
  /** Same as the system close button: the core hides the window to the tray. */
  close: () => void;
  /** Calls `fn` with whether the window is maximized now and whenever that changes. Returns the stopper. */
  watchMaximized: (fn: (maximized: boolean) => void) => () => void;
};

export const windowControls: WindowControls = {
  minimize: () => void getCurrentWindow().minimize(),
  toggleMaximize: () => void getCurrentWindow().toggleMaximize(),
  close: () => void getCurrentWindow().close(),
  watchMaximized(fn) {
    const w = getCurrentWindow();
    const check = () => w.isMaximized().then(fn);
    check();
    let stop: (() => void) | undefined;
    let gone = false;
    w.onResized(check).then((un) => (gone ? un() : (stop = un)));
    return () => {
      gone = true;
      stop?.();
    };
  },
};
