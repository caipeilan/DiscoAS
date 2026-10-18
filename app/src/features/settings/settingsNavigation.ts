export const settingsCategories = [
  { id: "general", label: "通用" },
  { id: "discovery", label: "发现" },
  { id: "hand", label: "手牌与迷你发现" },
  { id: "playback", label: "播放连接" },
  { id: "keyboard", label: "快捷键" },
  { id: "advanced", label: "高级" },
] as const;
export type SettingsCategory = typeof settingsCategories[number]["id"];
export const settingsSections = {
  appearance: "general", startup: "general",
  discovery: "discovery", exclusion: "discovery", weighting: "discovery", cover: "discovery",
  hand: "hand", window: "playback", spotify: "playback", browser: "playback",
  keyboard: "keyboard", rendering: "advanced", data: "advanced",
} as const;
export type SettingsSectionId = keyof typeof settingsSections;
export function isSettingsCategory(value: string | null): value is SettingsCategory {
  return settingsCategories.some(category => category.id === value);
}
