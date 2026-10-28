import type { GuiSettings } from "./types";
import { discoveryThemePalette } from "./features/discovery/themePalette";
import { effectiveNightMode } from "./theme";

export function applyAppearance(gui: GuiSettings) {
  const root = document.documentElement;
  const night = effectiveNightMode(gui, window.matchMedia("(prefers-color-scheme: dark)").matches);
  root.dataset.theme = night ? "dark" : "light";
  const family = gui.fontFamily.replace(/["\\]/g, "").trim();
  root.style.fontFamily = `${family ? `"${family}", ` : ""}"Segoe UI", "Microsoft YaHei UI", "Microsoft YaHei", sans-serif`;
  root.style.setProperty("--font-scale", String(gui.fontSize / 14));
  root.style.setProperty("--setting-scale", String(gui.mainWindowScale));
  root.style.setProperty("--card-scale", String(gui.discoveryCardScale));
  root.style.setProperty("--cancel-scale", String(gui.cancelButtonScale));
  root.style.setProperty("--replacement-scale", String(gui.replacementButtonScale));
  root.style.setProperty("--discovery-bar-scale", String(gui.discoveryBarScale));
  const card = discoveryThemePalette("card", night, night ? gui.cardNightMode : gui.card);
  const cancel = discoveryThemePalette("cancel", night, night
    ? gui.cancelButtonNightMode
    : gui.cancelButton);
  for (const [prefix, group] of [
    ["card", card],
    ["cancel", cancel],
  ] as const) {
    for (const [field, color] of Object.entries(group)) {
      const property = `--${prefix}-${field.replace(/[A-Z]/g, letter => `-${letter.toLowerCase()}`)}`;
      if (color && CSS.supports("color", color))
        root.style.setProperty(property, color);
      else root.style.removeProperty(property);
    }
  }
}
