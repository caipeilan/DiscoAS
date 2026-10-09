import type { GuiSettings } from "../../types";

export type TrayAction = "discover" | "main" | "pause" | "restart" | "quit";
export interface TrayMenuSnapshot {
  generation: number;
  gui: GuiSettings;
  labels: [string, string, string, string, string];
  paused: boolean;
}

export const trayActions: readonly TrayAction[] = ["discover", "main", "pause", "restart", "quit"];

/** Menu arrows wrap; Home/End remain predictable after font and language changes. */
export function nextMenuIndex(key: string, index: number, count = trayActions.length): number | null {
  if (count < 1) return null;
  if (key === "ArrowDown") return index < 0 ? 0 : (index + 1) % count;
  if (key === "ArrowUp") return index < 0 ? count - 1 : (index + count - 1) % count;
  if (key === "Home") return 0;
  if (key === "End") return count - 1;
  return null;
}
