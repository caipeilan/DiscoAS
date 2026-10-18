import type { GuiSettings } from "./types";

/** Old settings keep their explicit light/dark choice; new users can follow Windows. */
export function effectiveNightMode(gui: Pick<GuiSettings, "night_mode" | "follow_system_theme">, systemDark: boolean) {
  return gui.follow_system_theme ? systemDark : gui.night_mode;
}
