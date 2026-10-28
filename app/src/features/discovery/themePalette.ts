import type { ColorGroup } from "../../types";

const legacyPalettes: Record<string, ColorGroup> = {
  card: { background: "#ffffff", backgroundHover: "#e3f3f6", border: "#76d2fd", fontColor: "#000000" },
  card_night: { background: "#565656", backgroundHover: "#3d75bf", border: "#76d2fd", fontColor: "#ffffff" },
  cancel: { background: "#fecbc1", backgroundHover: "#fd8b76", border: "#fc6044", fontColor: "#000000" },
  cancel_night: { background: "#400601", backgroundHover: "#bd0316", border: "#fc6044", fontColor: "#ffffff" },
};

export const themePalettes: Record<string, ColorGroup> = {
  card: { background: "#FFFFFF", backgroundHover: "#F8F9FA", border: "#0A84FF", fontColor: "#24262A" },
  card_night: { background: "#292B2F", backgroundHover: "#303237", border: "#5ACFF0", fontColor: "#F0F1F3" },
  cancel: { background: "#FFFFFF", backgroundHover: "#FFF1F0", border: "#FFD1CC", fontColor: "#D70015" },
  cancel_night: { background: "#292B2F", backgroundHover: "#3A1512", border: "#6E2A24", fontColor: "#FF453A" },
};

/** Render old default colors with the current theme without rewriting saved or custom palettes. */
export function discoveryThemePalette(kind: "card" | "cancel", night: boolean, saved: ColorGroup): ColorGroup {
  const key = `${kind}${night ? "_night" : ""}`;
  const legacy = legacyPalettes[key];
  const isLegacyDefault = (Object.keys(legacy) as (keyof ColorGroup)[])
    .every((field) => saved[field]?.trim().toLowerCase() === legacy[field]);
  return isLegacyDefault ? themePalettes[key] : saved;
}
