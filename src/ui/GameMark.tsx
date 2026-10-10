import type { Game } from "../core";
import { OverwatchMark } from "./Icon";
import wow from "../assets/games/world-of-warcraft.svg";
import forever from "../assets/games/WOW_Forever_Icon_Vector_Infinity.svg";
import cod from "../assets/games/Call_of_Duty_logo_2023_2.svg";

export function GameMark({ game }: { game: Game }) {
  if (game === "none") return null;
  if (game === "overwatch") return <OverwatchMark />;
  return <img src={game === "world_of_warcraft" ? wow : game === "world_of_warcraft_forever" ? forever : cod} alt="" width={16} height={16} />;
}
