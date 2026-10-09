import type { ColorGroup } from "../../types";

const legacyPalettes: Record<string, ColorGroup> = {
  card: { background: "#ffffff", background_hover: "#e3f3f6", border: "#76d2fd", font_color: "#000000" },
  card_night: { background: "#565656", background_hover: "#3d75bf", border: "#76d2fd", font_color: "#ffffff" },
  cancel: { background: "#fecbc1", background_hover: "#fd8b76", border: "#fc6044", font_color: "#000000" },
  cancel_night: { background: "#400601", background_hover: "#bd0316", border: "#fc6044", font_color: "#ffffff" },
};

const themePalettes: Record<string, ColorGroup> = {
  card: { background: "#ffffff", background_hover: "#f8f9fa", border: "#168ca3", font_color: "#24262a" },
  card_night: { background: "#292b2f", background_hover: "#303237", border: "#61c4d7", font_color: "#f0f1f3" },
  cancel: { background: "#ffffff", background_hover: "#fff0f1", border: "#edc0c5", font_color: "#bf333e" },
  cancel_night: { background: "#292b2f", background_hover: "#462c32", border: "#79414a", font_color: "#ff9da5" },
};

/** Render old default colors with the current theme without rewriting saved or custom palettes. */
export function discoveryThemePalette(kind: "card" | "cancel", night: boolean, saved: ColorGroup): ColorGroup {
  const key = `${kind}${night ? "_night" : ""}`;
  const legacy = legacyPalettes[key];
  const isLegacyDefault = (Object.keys(legacy) as (keyof ColorGroup)[])
    .every((field) => saved[field]?.trim().toLowerCase() === legacy[field]);
  return isLegacyDefault ? themePalettes[key] : saved;
}
