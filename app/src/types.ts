export interface PlaylistSetting {
  name: string;
  playlist_album_id: string;
  typename: string;
  playlist_album_name: string;
  playlist_album_remark: string;
  update_time: string;
  enabled: boolean;
}
export interface Preferences {
  number_of_discovered_songs: number;
  have_mystery_song: boolean;
  num_of_mystery_song: number;
  mystery_song_cover: string;
  cache_batches: number;
  preload_deduplication: boolean;
  discovery_weighting: DiscoveryWeighting;
  replacement_limit: number;
  refreshing_after_cancel: boolean;
  shortcut_key: string;
  history_exclusion: "off" | "selected" | "discovered";
  history_limit: number;
  discovery_keybindings: DiscoveryKeybindings;
  hand: HandSettings;
  playlist_albums: PlaylistSetting[];
}
export interface DiscoveryKeybindings {
  up: string;
  left: string;
  down: string;
  right: string;
  select: string;
  replace: string;
}
export interface HandSettings {
  enabled: boolean; reveal_mystery: boolean; resident: boolean; keep_discovery_open: boolean;
  shortcut: string; side: "left" | "right" | "bottom"; capacity: number;
  scale: number; edge_distance: number; position: number; overlap: number; tilt: number;
}
export const defaultHandSettings: HandSettings = {
  enabled: false, reveal_mystery: false, resident: false, keep_discovery_open: false,
  shortcut: "Alt+H", side: "bottom", capacity: 10, scale: 1, edge_distance: 12, position: 50, overlap: 45, tilt: 12,
};
export interface HandCard { id: string; song: Song; collectedAt: number; mysteryRevealed: boolean }
export interface CollectedHandCard { id: string; discovery: DiscoveryState }
export interface HandRect { left: number; top: number; width: number; height: number }
export interface HandSnapshot {
  cardOrder?: string[];
  generation: number; cards: HandCard[]; settings: HandSettings; gui: GuiSettings;
  keys: DiscoveryKeybindings; workArea: HandRect; preview: boolean; focus: boolean; expanded: boolean;
  arrival: { id: string; rect: HandRect } | null;
}
export interface HandVisibility { generation: number; expanded: boolean; focus: boolean }
export interface DiscoveryWeighting {
  enabled: boolean;
  base_weight: number;
  discovered_penalty: number;
  selected_penalty: number;
  recovery_batches: number;
  boost_after_batches: number;
  boost_per_batch: number;
  max_weight: number;
}
export interface DiscoveryState {
  songs: Song[];
  batchEpoch: number;
  remainingSongs: number;
  replacementsRemaining: number;
  exclusionEnabled: boolean;
  replacementEnabled: boolean;
  preview: boolean;
}
export interface HistoryIdentity { platform: string; songId: string }
export interface HistoryMutation {
  identities: HistoryIdentity[];
  action: "delete" | "discovered" | "selected";
  value?: boolean;
}
export interface HistoryCover extends HistoryIdentity { coverDataUri: string | null }
export interface PreviewPointer { x: number; y: number }
export interface PreviewKey {
  action: "up" | "left" | "down" | "right" | "select" | "replace";
  key: string; code: string; ctrlKey: boolean; altKey: boolean;
  shiftKey: boolean; metaKey: boolean; repeat: boolean; source: "native";
}
export type HandKey = Omit<PreviewKey, "action"> & { action: PreviewKey["action"] | "discard" };
export interface PreviewCloseRect { left: number; top: number; width: number; height: number }
export interface LibraryEntry {
  platform: string;
  id: string;
  kind: string;
  title: string;
  remark: string;
  enabled: boolean;
  songCount: number;
  coverUrl: string;
  coverDataUri: string | null;
  updatedAt: string;
  cacheError: string | null;
}
export interface AppState {
  version: string;
  settings: Preferences;
  guiSettings: GuiSettings;
  desktopSettings: DesktopPreferences;
  desktopSettingsError: string | null;
  playlists: LibraryEntry[];
  dataPath: string;
  legacyDataPath: string | null;
  shortcutError: string | null;
  startupRefreshError: string | null;
}
export interface DesktopPreferences {
  launch_at_login: boolean;
  start_hidden: boolean;
  update_on_startup: boolean;
  overlay_monitor: "current" | "primary";
  minimize_after_playback: boolean;
  minimize_delay_seconds: number;
  spotify_playback_mode: "scheme" | "pause_then_scheme" | "extension";
  spotify_defaults_version: number;
  browser_playback_mode: "extension" | "direct";
}
export interface LibraryProgress {
  requestId: string;
  phase: "fetching" | "cover" | "saving" | "done";
  completed: number;
  total: number | null;
  pages?: number;
}
export interface HistoryEntry {
  songId: string;
  platform: string;
  playlistId: string;
  typename: string;
  name: string;
  artistNames: string[];
  discoveredAt: number | null;
  selectedAt: number | null;
  coverUrl?: string | null;
  coverKey?: string | null;
  coverDataUri?: string | null;
}
export interface SpotifyBridgeStatus {
  connected: boolean;
  extensionPath: string | null;
}
export interface SpotifySetupStatus {
  phase: string;
  message: string;
  toolVersion: string | null;
  toolPath: string | null;
  managed: boolean;
}
export interface BrowserBridgeStatus {
  connected: boolean;
  connectedBrowser?: "chromium" | "firefox" | null;
  extensionPath: string | null;
  extensionPaths?: { chromium: string; firefox: string } | null;
}
export interface ColorGroup {
  background: string;
  background_hover: string;
  border: string;
  font_color: string;
}
export interface SystemFont {
  family: string;
  label: string;
  aliases: string[];
}
export interface GuiSettings {
  night_mode: boolean;
  card_size: number;
  cancel_button_size: number;
  replacement_button_size: number;
  discovery_bar_size: number;
  setting_size: number;
  font_family: string;
  font_size: number;
  language: string;
  user_configured: boolean;
  card: ColorGroup;
  cancel_button: ColorGroup;
  setting: ColorGroup;
  card_night_mode: ColorGroup;
  cancel_button_night_mode: ColorGroup;
  setting_night_mode: ColorGroup;
}
export const defaultGuiSettings: GuiSettings = {
  night_mode: false,
  card_size: 1,
  cancel_button_size: 1,
  replacement_button_size: 1,
  discovery_bar_size: 1,
  setting_size: 1,
  font_family: "",
  font_size: 14,
  language: "zh_CN",
  user_configured: false,
  card: {
    background: "#FFFFFF",
    background_hover: "#e3f3f6",
    border: "#76d2fd",
    font_color: "#000000",
  },
  cancel_button: {
    background: "#fecbc1",
    background_hover: "#fd8b76",
    border: "#fc6044",
    font_color: "#000000",
  },
  setting: {
    background: "#FFFFFF",
    background_hover: "#d0ebf0",
    border: "#76e8fd",
    font_color: "#000000",
  },
  card_night_mode: {
    background: "#565656",
    background_hover: "#3d75bf",
    border: "#76d2fd",
    font_color: "#ffffff",
  },
  cancel_button_night_mode: {
    background: "#400601",
    background_hover: "#bd0316",
    border: "#fc6044",
    font_color: "#ffffff",
  },
  setting_night_mode: {
    background: "#565656",
    background_hover: "#3dabbf",
    border: "#76c6fd",
    font_color: "#ffffff",
  },
};
export interface Song {
  songId: string;
  name: string;
  artistNames: string[];
  albumPicUrl: string;
  coverDataUri: string | null;
  coverError: string | null;
  mysteryMode: boolean;
  platform: string;
  playlistId: string;
  typename: string;
  filename: string;
  detailError: string | null;
}
export const platforms = [
  {
    id: "NeteaseCloudMusic",
    label: "网易云音乐",
    color: "#d9535b",
    hint: "https://music.163.com/#/playlist?id=…",
  },
  {
    id: "QQMusic",
    label: "QQ 音乐",
    color: "#35a37d",
    hint: "https://y.qq.com/n/ryqq/playlist/…",
  },
  {
    id: "KugouMusic",
    label: "酷狗音乐",
    color: "#4b8dc9",
    hint: "酷狗分享链接、gcid 或歌单 ID",
  },
  {
    id: "Spotify",
    label: "Spotify",
    color: "#388a52",
    hint: "https://open.spotify.com/playlist/…",
  },
  { id: "KuwoMusic", label: "酷我音乐", color: "#c09220", hint: "酷我歌单或专辑分享链接、ID" },
  { id: "QishuiMusic", label: "汽水音乐", color: "#dd5674", hint: "汽水音乐完整歌单或专辑分享链接、ID" },
  { id: "YouTube", label: "YouTube", color: "#c83f48", hint: "https://www.youtube.com/playlist?list=… 或视频链接" },
  { id: "Bilibili", label: "Bilibili", color: "#388ea8", hint: "公开视频、收藏夹、合集或系列链接" },
];
export const emptyState: AppState = {
  version: "",
  desktopSettings: {
    launch_at_login: false,
    start_hidden: false,
    update_on_startup: true,
    overlay_monitor: "current",
    minimize_after_playback: true,
    minimize_delay_seconds: 4,
    spotify_playback_mode: "extension",
    spotify_defaults_version: 1,
    browser_playback_mode: "extension",
  },
  guiSettings: defaultGuiSettings,
  settings: {
    number_of_discovered_songs: 3,
    have_mystery_song: true,
    num_of_mystery_song: 1,
    mystery_song_cover: "",
    cache_batches: 2,
    preload_deduplication: false,
    discovery_weighting: { enabled: false, base_weight: 100, discovered_penalty: 20,
      selected_penalty: 40, recovery_batches: 5, boost_after_batches: 10,
      boost_per_batch: 5, max_weight: 500 },
    replacement_limit: 1,
    refreshing_after_cancel: false,
    shortcut_key: "Alt+D",
    history_exclusion: "off",
    history_limit: 200,
    discovery_keybindings: { up: "W", left: "A", down: "S", right: "D", select: "Enter", replace: "R" },
    hand: defaultHandSettings,
    playlist_albums: [],
  },
  playlists: [],
  dataPath: "",
  legacyDataPath: null,
  shortcutError: null,
  desktopSettingsError: null,
  startupRefreshError: null,
};
