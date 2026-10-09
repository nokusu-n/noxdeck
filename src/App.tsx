import { useEffect, useState } from "react";
import { MainWindow } from "./components/MainWindow";
import { core, State } from "./core";
import { setLang } from "./i18n";
import { windowControls } from "./window";

/** Follows the core's state and hands it to the window. Everything the player sees is in <MainWindow/>. */
export default function App() {
  const [state, setState] = useState<State | null>(null);

  useEffect(() => {
    let stop: (() => void) | undefined;
    let gone = false;
    const receive = (next: State) => { setLang(next.lang); document.documentElement.lang = next.lang; setState(next); };
    core.subscribe(receive).then((un) => (gone ? un() : (stop = un)));
    core.getState().then(receive);
    return () => {
      gone = true;
      stop?.();
    };
  }, []);

  if (!state) return null;
  return <MainWindow state={state} api={core} controls={windowControls} />;
}
