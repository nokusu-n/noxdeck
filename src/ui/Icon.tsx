import { SYMBOLS } from "./sprite";

/** The sprite every <Icon/> and <Mark/> draws from. Rendered once, above the window. */
export function Sprite() {
  return <svg width="0" height="0" style={{ position: "absolute" }} aria-hidden="true" dangerouslySetInnerHTML={{ __html: `<defs>${SYMBOLS}</defs>` }} />;
}

export function Icon({ name, size = 16, spin = false }: { name: string; size?: number; spin?: boolean }) {
  return <svg className={`ic${spin ? " spin" : ""}`} width={size} height={size} aria-hidden="true"><use href={`#${name}`} /></svg>;
}

/** The game's mark. It is drawn in the colour of the text around it: grey at rest, green while running. */
export function OverwatchMark({ size = 16 }: { size?: number }) {
  return <svg className="ow" width={size} height={size} aria-hidden="true"><use href="#i-ow" /></svg>;
}
