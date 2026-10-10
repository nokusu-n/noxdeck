import type { CSSProperties } from "react";
import type { Game } from "../core";
import { OverwatchMark } from "./Icon";
import wow from "../assets/games/world-of-warcraft.svg";
import forever from "../assets/games/WOW_Forever_Icon_Vector_Infinity.svg";
import cod from "../assets/games/Call_of_Duty_logo_2023_2.svg";

const art = { world_of_warcraft: wow, world_of_warcraft_forever: forever, call_of_duty: cod };

/**
 * A game's mark in one colour, the colour of the text around it: grey at rest, blue on a chosen
 * chip, green on a running button. Overwatch is drawn from the sprite, the others as a mask of
 * their artwork, so the four always look like one set. Call of Duty's wordmark is wider than tall.
 */
export function GameMark({ game, size = 16 }: { game: Game; size?: number }) {
  if (game === "none") return null;
  if (game === "overwatch") return <OverwatchMark size={size} />;
  const style = { "--gm": `url(${JSON.stringify(art[game])})`, width: game === "call_of_duty" ? Math.round(size * 1.7) : size, height: size } as CSSProperties;
  return <span className="gm" style={style} aria-hidden="true" />;
}
