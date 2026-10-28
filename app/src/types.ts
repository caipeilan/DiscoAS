export interface SourceConfig {
  platform: string;
  sourceId: string;
  sourceKind: string;
  title: string;
  remark: string;
  updatedAt: string;
  enabled: boolean;
}
export interface Preferences {
  normalSongCount: number;
  mysterySongsEnabled: boolean;
  mysterySongCount: number;
  mysteryCover: string;
  preloadBatchCount: number;
  deduplicatePreloadedBatches: boolean;
  discoveryWeighting: DiscoveryWeighting;
  weightingMode: DiscoveryWeightingMode;
  rankedWeighting: RankedWeighting;
  replacementLimit: number;
  refreshOnCancel: boolean;
  discoveryShortcut: string;
  recentExclusionMode: "off" | "selected" | "discovered";
  recentExclusionLimit: number;
  discoveryKeybindings: DiscoveryKeybindings;
  hand: HandSettings;
  handDiscovery: HandDiscoverySettings;
  sources: SourceConfig[];
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
  enabled: boolean; revealMystery: boolean; resident: boolean; keepDiscoveryOpen: boolean;
  autoPlayEnabled: boolean; createCards: boolean; autoPlayPosition: "first" | "last";
  autoPlayDetection: "system" | "window" | "mixed";
  shortcut: string; playShortcuts: string[]; side: "left" | "right" | "bottom"; capacity: number;
  scale: number; edgeOffsetPx: number; positionPercent: number; overlapPercent: number; tiltDegrees: number;
}
export const defaultHandSettings: HandSettings = {
  enabled: false, revealMystery: false, resident: false, keepDiscoveryOpen: false,
  autoPlayEnabled: false, createCards: false, autoPlayPosition: "first",
  autoPlayDetection: "mixed",
  shortcut: "Alt+H", side: "bottom", capacity: 10, scale: 1, edgeOffsetPx: 12, positionPercent: 50, overlapPercent: 45, tiltDegrees: 12,
  playShortcuts: Array.from({ length: 9 }, (_, i) => `Alt+${i + 1}`),
};
export type LiquidMode = "static" | "blur";
export interface HandDiscoverySettings extends Pick<HandSettings, "side" | "scale" | "edgeOffsetPx" | "positionPercent" | "overlapPercent" | "tiltDegrees"> { enabled: boolean; merged: boolean; textOutlineWidth: number; liquidStrength: number; liquidMode: LiquidMode }
export const defaultHandDiscoverySettings: HandDiscoverySettings = { enabled: false, merged: true, textOutlineWidth: 1, liquidStrength: 100, liquidMode: "static", side: "bottom", scale: 1, edgeOffsetPx: 12, positionPercent: 50, overlapPercent: 45, tiltDegrees: 12 };
export interface HandCard { id: string; song: Song; collectedAt: number; mysteryRevealed: boolean; discovery?: boolean; placeholder?: boolean }
export interface CollectedHandCard { id: string; discovery: DiscoveryState }
export interface HandRect { left: number; top: number; width: number; height: number }
export interface HandPoint { x: number; y: number }
export interface HandSurface { left: number; top: number; width: number; height: number; scale: number }
export interface HandDragPose { x: number; y: number; angle: number; width: number; height: number; scale: number }
export interface HandDragSnapshot {
  generation: number; card: HandCard; settings: HandSettings;
  discoverySettings: HandDiscoverySettings; gui: GuiSettings; pose: HandDragPose;
}
export interface HandDragIntent { playable: boolean; collecting: boolean; dx: number; dy: number }
export interface HandDragEnd { cancelled: boolean }
export interface HandDockSnapshot {
  generation: number; count: number; settings: HandSettings; gui: GuiSettings;
  workArea: HandRect; workAreas: HandRect[]; dockPoint: HandPoint | null; expanded: boolean;
  discoverySettings?: HandDiscoverySettings; discovery?: DiscoveryState | null;
}
export interface HandSnapshot {
  cardOrder?: string[];
  generation: number; cards: HandCard[]; settings: HandSettings; gui: GuiSettings;
  keys: DiscoveryKeybindings; workArea: HandRect; preview: boolean; focus: boolean; expanded: boolean;
  workAreas?: HandRect[]; dockPoint?: HandPoint | null; surface?: HandSurface;
  arrival: { id: string; rect: HandRect; mysteryCover?: string | null } | null;
  discoverySettings?: HandDiscoverySettings; discovery?: DiscoveryState | null;
}
export interface HandVisibility { generation: number; expanded: boolean; focus: boolean; surface?: HandSurface }
export interface DiscoveryWeighting {
  enabled: boolean;
  baseWeight: number;
  discoveredPenalty: number;
  selectedPenalty: number;
  recoveryBatches: number;
  boostAfterBatches: number;
  boostPerBatch: number;
  maxWeight: number;
}
export type DiscoveryWeightingMode = "none" | "traditional" | "ranked" | "rotation";
export interface RankedWeighting {
  softBand: number;
  timeCooling: boolean;
  timeCoolingStrength: number;
  timeCoolingBatches: number;
  revival: boolean;
  revivalGain: number;
  revivalWait: number;
  revivalSpan: number;
}
export const defaultRankedWeighting: RankedWeighting = {
  softBand: 20,
  timeCooling: false,
  timeCoolingStrength: 0.85,
  timeCoolingBatches: 50,
  revival: false,
  revivalGain: 0.5,
  revivalWait: 200,
  revivalSpan: 200,
};
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
export interface SourceSummary {
  platform: string;
  sourceId: string;
  sourceKind: string;
  title: string;
  remark: string;
  enabled: boolean;
  songCount: number;
  missingCount?: number;
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
  sources: SourceSummary[];
  dataPath: string;
  legacyDataPath: string | null;
  shortcutError: string | null;
  startupRefreshError: string | null;
}
export interface LibraryPatch {
  settings: Preferences;
  updatedSources: SourceSummary[];
  startupRefreshError: string | null;
}
export interface DesktopPreferences {
  launchAtLogin: boolean;
  startHidden: boolean;
  updateOnStartup: boolean;
  pauseBeforePlayback: boolean;
  overlayMonitor: string;
  minimizeAfterPlayback: boolean;
  temporaryHideSeconds: number;
  clientWindowMode: "off" | "temporary" | "persistent";
  clientSeparateDesktop: boolean;
  spotifyPlaybackMode: "scheme" | "pause_then_scheme" | "extension";
  spotifyDefaultsVersion: number;
  browserPlaybackMode: "extension" | "direct";
  renderingMode: "auto" | "integrated" | "discrete" | "cpu";
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
  sourceId: string;
  sourceKind: string;
  title: string;
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
  issue?: { code: string; path: string; repairable: boolean } | null;
  recoveryPath?: string | null;
}
export interface BrowserBridgeStatus {
  connected: boolean;
  connectedBrowser?: "chromium" | "firefox" | null;
  extensionPath: string | null;
  extensionPaths?: { chromium: string; firefox: string } | null;
  preparedBrowsers?: ("chromium" | "firefox")[];
}
export interface ColorGroup {
  background: string;
  backgroundHover: string;
  border: string;
  fontColor: string;
}
export interface SystemFont {
  family: string;
  label: string;
  aliases: string[];
}
export interface GuiSettings {
  nightMode: boolean;
  followSystemTheme: boolean;
  discoveryCardScale: number;
  cancelButtonScale: number;
  replacementButtonScale: number;
  discoveryBarScale: number;
  mainWindowScale: number;
  fontFamily: string;
  fontSize: number;
  language: string;
  userConfigured: boolean;
  card: ColorGroup;
  cancelButton: ColorGroup;
  setting: ColorGroup;
  cardNightMode: ColorGroup;
  cancelButtonNightMode: ColorGroup;
  settingNightMode: ColorGroup;
}
export const defaultGuiSettings: GuiSettings = {
  nightMode: false,
  followSystemTheme: true,
  discoveryCardScale: 1,
  cancelButtonScale: 1,
  replacementButtonScale: 1,
  discoveryBarScale: 1,
  mainWindowScale: 1,
  fontFamily: "",
  fontSize: 14,
  language: "zh_CN",
  userConfigured: false,
  card: {
    background: "#FFFFFF",
    backgroundHover: "#F8F9FA",
    border: "#0A84FF",
    fontColor: "#24262A",
  },
  cancelButton: {
    background: "#FFFFFF",
    backgroundHover: "#FFF1F0",
    border: "#FFD1CC",
    fontColor: "#D70015",
  },
  setting: {
    background: "#FFFFFF",
    backgroundHover: "#F2F7FF",
    border: "#B3D4FF",
    fontColor: "#000000",
  },
  cardNightMode: {
    background: "#292B2F",
    backgroundHover: "#303237",
    border: "#5ACFF0",
    fontColor: "#F0F1F3",
  },
  cancelButtonNightMode: {
    background: "#292B2F",
    backgroundHover: "#3A1512",
    border: "#6E2A24",
    fontColor: "#FF453A",
  },
  settingNightMode: {
    background: "#292B2F",
    backgroundHover: "#102A43",
    border: "#1D4E89",
    fontColor: "#FFFFFF",
  },
};
export interface Song {
  songId: string;
  title: string;
  artistNames: string[];
  albumPicUrl: string;
  coverDataUri: string | null;
  coverError: string | null;
  mysteryMode: boolean;
  platform: string;
  sourceId: string;
  sourceKind: string;
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
  { id: "AppleMusic", label: "Apple Music", color: "#fa243c", hint: "https://music.apple.com/… 公开歌单或专辑链接" },
  { id: "YouTube", label: "YouTube", color: "#c83f48", hint: "https://www.youtube.com/playlist?list=… 或视频链接" },
  { id: "Bilibili", label: "Bilibili", color: "#388ea8", hint: "公开视频、收藏夹、合集或系列链接" },
  { id: "LocalMusic", label: "本地音频", color: "#758c8f", hint: "" },
];
export const emptyState: AppState = {
  version: "",
  desktopSettings: {
    launchAtLogin: false,
    startHidden: false,
    updateOnStartup: true,
    pauseBeforePlayback: false,
    overlayMonitor: "current",
    minimizeAfterPlayback: true,
    temporaryHideSeconds: 4,
    clientWindowMode: "off",
    clientSeparateDesktop: false,
    spotifyPlaybackMode: "extension",
    spotifyDefaultsVersion: 1,
    browserPlaybackMode: "extension",
    renderingMode: "auto",
  },
  guiSettings: defaultGuiSettings,
  settings: {
    normalSongCount: 3,
    mysterySongsEnabled: true,
    mysterySongCount: 1,
    mysteryCover: "",
    preloadBatchCount: 2,
    deduplicatePreloadedBatches: false,
    discoveryWeighting: { enabled: false, baseWeight: 100, discoveredPenalty: 20,
      selectedPenalty: 40, recoveryBatches: 5, boostAfterBatches: 10,
      boostPerBatch: 5, maxWeight: 500 },
    weightingMode: "none",
    rankedWeighting: defaultRankedWeighting,
    replacementLimit: 1,
    refreshOnCancel: false,
    discoveryShortcut: "Alt+D",
    recentExclusionMode: "off",
    recentExclusionLimit: 200,
    discoveryKeybindings: { up: "W", left: "A", down: "S", right: "D", select: "Enter", replace: "R" },
    hand: defaultHandSettings,
    handDiscovery: defaultHandDiscoverySettings,
    sources: [],
  },
  sources: [],
  dataPath: "",
  legacyDataPath: null,
  shortcutError: null,
  desktopSettingsError: null,
  startupRefreshError: null,
};

export type DiscoveryStatus = Omit<DiscoveryState, "songs">;
export interface DiscoveryPreferences { settings: Preferences; guiSettings: GuiSettings }
export type PreferencePatch = Partial<Pick<AppState, "settings" | "guiSettings" | "desktopSettings">> & { discoveryInvalidated?: boolean };
