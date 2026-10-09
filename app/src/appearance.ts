import type { GuiSettings } from "./types";
import { discoveryThemePalette } from "./features/discovery/themePalette";

export function applyAppearance(gui: GuiSettings) {
  const root = document.documentElement;
  root.dataset.theme = gui.night_mode ? "dark" : "light";
  const family = gui.font_family.replace(/["\\]/g, "").trim();
  root.style.fontFamily = `${family ? `"${family}", ` : ""}"Segoe UI", "Microsoft YaHei UI", "Microsoft YaHei", sans-serif`;
  root.style.setProperty("--font-scale", String(gui.font_size / 14));
  root.style.setProperty("--setting-scale", String(gui.setting_size));
  root.style.setProperty("--card-scale", String(gui.card_size));
  root.style.setProperty("--cancel-scale", String(gui.cancel_button_size));
  root.style.setProperty("--replacement-scale", String(gui.replacement_button_size));
  root.style.setProperty("--discovery-bar-scale", String(gui.discovery_bar_size));
  const card = discoveryThemePalette("card", gui.night_mode, gui.night_mode ? gui.card_night_mode : gui.card);
  const cancel = discoveryThemePalette("cancel", gui.night_mode, gui.night_mode
    ? gui.cancel_button_night_mode
    : gui.cancel_button);
  for (const [prefix, group] of [
    ["card", card],
    ["cancel", cancel],
  ] as const) {
    for (const [field, color] of Object.entries(group)) {
      const property = `--${prefix}-${field.replace(/_/g, "-")}`;
      if (color && CSS.supports("color", color))
        root.style.setProperty(property, color);
      else root.style.removeProperty(property);
    }
  }
}
