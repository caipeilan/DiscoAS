import type { GuiSettings, Preferences } from "../../types";

export function samePreferences(a: Preferences, b: Preferences) {
  const { playlist_albums: _a, ...left } = a;
  const { playlist_albums: _b, ...right } = b;
  return JSON.stringify(left) === JSON.stringify(right);
}
export function sameGuiPreferences(a: GuiSettings, b: GuiSettings) {
  const keys: Array<keyof GuiSettings> = [
    "night_mode",
    "card_size",
    "cancel_button_size",
    "replacement_button_size",
    "discovery_bar_size",
    "setting_size",
    "font_family",
    "font_size",
    "language",
    "card",
    "cancel_button",
    "setting",
    "card_night_mode",
    "cancel_button_night_mode",
    "setting_night_mode",
  ];
  return keys.every((key) => JSON.stringify(a[key]) === JSON.stringify(b[key]));
}
