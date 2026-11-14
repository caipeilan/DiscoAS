export const skins = [
  { id: "classic", label: "经典" },
  { id: "curry-udon", label: "咖喱乌冬面" },
] as const;

export type SkinId = (typeof skins)[number]["id"];

export function effectiveSkin(value: unknown): SkinId {
  return skins.find(skin => skin.id === value)?.id ?? "classic";
}

/** A tiled border scales with its control without observing layout or adding DOM. */
export function skinFrameImage(color: string) {
  const svg = '<svg xmlns="http://www.w3.org/2000/svg" width="112" height="112" viewBox="0 0 112 112">'
    + '<path fill="none" stroke="' + color + '" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" '
    + 'd="M13 2.5C27 1 37 4 56 2.5S85 1 99 2.5Q109.5 2 109.5 13C111 30 108 40 109.5 56S111 86 109.5 99Q110 109.5 99 109.5C82 111 74 108 56 109.5S29 111 13 109.5Q2.5 110 2.5 99C1 83 4 73 2.5 56S1 29 2.5 13Q2 2.5 13 2.5Z"/></svg>';
  return 'url("data:image/svg+xml,' + encodeURIComponent(svg) + '")';
}
