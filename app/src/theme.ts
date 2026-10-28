import type { GuiSettings } from "./types";

/** Old settings keep their explicit light/dark choice; new users can follow Windows. */
export function effectiveNightMode(gui: Pick<GuiSettings, "nightMode" | "followSystemTheme">, systemDark: boolean) {
  return gui.followSystemTheme ? systemDark : gui.nightMode;
}
