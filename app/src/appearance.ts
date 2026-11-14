import type { GuiSettings } from "./types";
import { discoveryThemePalette } from "./features/discovery/themePalette";
import { effectiveNightMode } from "./theme";
import { effectiveSkin, skinFrameImage, pencilFrameImage } from "./skins";
import pencilPigment from "./assets/skins/indigestion-pencil.png?inline";

let applied = "";
export function applyAppearance(gui: GuiSettings) {
  const root = document.documentElement;
  const night = effectiveNightMode(gui, window.matchMedia("(prefers-color-scheme: dark)").matches);
  const signature = JSON.stringify([gui, night]);
  if (signature === applied) return;
  const skin = effectiveSkin(gui.skin);
  const framesChanged = root.dataset.skin !== skin || root.dataset.theme !== (night ? "dark" : "light");
  root.dataset.skin = skin;
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
      if (skin === "classic" && color && CSS.supports("color", color))
        root.style.setProperty(property, color);
      else root.style.removeProperty(property);
    }
  }
  if (framesChanged) {
    const colors = skin !== "classic" ? getComputedStyle(root) : null;
    for (const [suffix, token] of [
      ["", "--control-border"], ["-accent", "--accent-focus"], ["-danger", "--danger"], ["-muted", "--border"],
    ] as const) {
      if (colors) {
        const color = colors.getPropertyValue(token).trim();
        root.style.setProperty("--skin-frame" + suffix, skin === "indigestion" ? pencilFrameImage(color, pencilPigment) : skinFrameImage(color));
      }
      else root.style.removeProperty("--skin-frame" + suffix);
    }
    if (skin === "curry-udon") root.style.setProperty("--skin-shape", skinFrameImage("#fff", "#fff"));
    else root.style.removeProperty("--skin-shape");
  }
  applied = signature;
}
