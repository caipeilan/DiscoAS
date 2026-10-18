import type { ColorGroup } from "../../types";

const legacyPalettes: Record<string, ColorGroup> = {
  card: { background: "#ffffff", background_hover: "#e3f3f6", border: "#76d2fd", font_color: "#000000" },
  card_night: { background: "#565656", background_hover: "#3d75bf", border: "#76d2fd", font_color: "#ffffff" },
  cancel: { background: "#fecbc1", background_hover: "#fd8b76", border: "#fc6044", font_color: "#000000" },
  cancel_night: { background: "#400601", background_hover: "#bd0316", border: "#fc6044", font_color: "#ffffff" },
};

export const themePalettes: Record<string, ColorGroup> = {
  card: { background: "#FFFFFF", background_hover: "#F8F9FA", border: "#0A84FF", font_color: "#24262A" },
  card_night: { background: "#292B2F", background_hover: "#303237", border: "#5ACFF0", font_color: "#F0F1F3" },
  cancel: { background: "#FFFFFF", background_hover: "#FFF1F0", border: "#FFD1CC", font_color: "#D70015" },
  cancel_night: { background: "#292B2F", background_hover: "#3A1512", border: "#6E2A24", font_color: "#FF453A" },
};

/** Render old default colors with the current theme without rewriting saved or custom palettes. */
export function discoveryThemePalette(kind: "card" | "cancel", night: boolean, saved: ColorGroup): ColorGroup {
  const key = `${kind}${night ? "_night" : ""}`;
  const legacy = legacyPalettes[key];
  const isLegacyDefault = (Object.keys(legacy) as (keyof ColorGroup)[])
    .every((field) => saved[field]?.trim().toLowerCase() === legacy[field]);
  return isLegacyDefault ? themePalettes[key] : saved;
}
