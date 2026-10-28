import type { GuiSettings, Preferences } from "../../types";

export function samePreferences(a: Preferences, b: Preferences) {
  const { sources: _a, ...left } = a;
  const { sources: _b, ...right } = b;
  return JSON.stringify(left) === JSON.stringify(right);
}
export function sameGuiPreferences(a: GuiSettings, b: GuiSettings) {
  if (Boolean(a.followSystemTheme) !== Boolean(b.followSystemTheme)) return false;
  const keys: Array<keyof GuiSettings> = [
    "nightMode",
    "discoveryCardScale",
    "cancelButtonScale",
    "replacementButtonScale",
    "discoveryBarScale",
    "mainWindowScale",
    "fontFamily",
    "fontSize",
    "language",
    "card",
    "cancelButton",
    "setting",
    "cardNightMode",
    "cancelButtonNightMode",
    "settingNightMode",
  ];
  return keys.every((key) => JSON.stringify(a[key]) === JSON.stringify(b[key]));
}
