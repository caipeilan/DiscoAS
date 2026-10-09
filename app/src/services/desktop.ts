import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openUrl } from "@tauri-apps/plugin-opener";
import { t } from "../i18n";
import type { TrayMenuSnapshot } from "../features/tray/trayMenuModel";
import type { LibraryProgress, Song, DiscoveryState, Preferences, GuiSettings,
  HistoryEntry, HistoryMutation, HistoryIdentity, HistoryCover, PreviewPointer, PreviewKey, PreviewCloseRect } from "../types";

export interface PlaySongArgs { platform: string; songId: string; playlistId: string; typename: string }

export type DesktopCommand =
  | "tray_menu_ready"
  | "present_tray_menu"
  | "dismiss_tray_menu"
  | "tray_menu_action"
  | "get_app_state"
  | "init_preload"
  | "get_image"
  | "import_playlist"
  | "cancel_library_operation"
  | "edit_playlist_remark"
  | "get_discovery_history"
  | "repair_discovery_history_metadata"
  | "clear_discovery_history"
  | "record_discovery_displayed"
  | "export_spotify_extension"
  | "get_spotify_bridge_status"
  | "open_spotify_extension_folder"
  | "get_spotify_setup_status"
  | "configure_spotify_support"
  | "export_browser_extension"
  | "get_browser_bridge_status"
  | "open_browser_extension_folder"
  | "enable_playlist"
  | "remove_playlist"
  | "remove_playlists"
  | "get_system_fonts"
  | "import_legacy"
  | "save_preferences"
  | "save_gui_preferences"
  | "save_desktop_preferences"
  | "choose_mystery_cover"
  | "open_data_folder"
  | "open_log_folder"
  | "discover_songs"
  | "discover_batch"
  | "get_discovery_state"
  | "replace_discovery_song"
  | "mutate_discovery_history"
  | "get_history_covers"
  | "start_discovery_preview"
  | "update_discovery_preview"
  | "end_discovery_preview"
  | "set_preview_close_rect"
  | "report_cancelled"
  | "play_song"
  | "show_discover"
  | "show_main"
  | "set_shortcut_recording"
  | "finish_startup"
  | "log_frontend_error"
  | "check_for_updates";

export interface UpdateInfo {
  currentVersion: string;
  latestVersion: string | null;
  status: "update_available" | "up_to_date" | "no_release";
  releaseUrl: string;
  releaseNotes: string;
  publishedAt: string | null;
  downloadUrl: string | null;
  fullDownloadUrl: string | null;
}
export const checkForUpdates = () => call<UpdateInfo>("check_for_updates");

export interface DesktopEvents {
  "tray-menu-open": TrayMenuSnapshot;
  "library-changed": { discoveryInvalidated: boolean } | void;
  "library-progress": LibraryProgress;
  "startup-refresh-failed": void;
  "gui-changed": void;
  "desktop-changed": void;
  "cover-refresh-failed": void;
  "client-window-result": { platform: string; minimized: boolean; warning: string | null };
  "playback-result": { platform: string; songId: string; success: boolean; confirmed: boolean; error: string | null };
  "discovery-changed": Song[];
  "discovery-state-changed": DiscoveryState;
  "discovery-history-changed": void;
  "preview-pointer": PreviewPointer;
  "preview-key": PreviewKey;
  "preview-appearance": GuiSettings;
  "preview-closed": void;
  "show-overlay": void;
  "cancel-overlay": void;
}

export const discoverBatch = (force = false) => call<DiscoveryState>("discover_batch", { force });
export const getDiscoveryState = () => call<DiscoveryState>("get_discovery_state");
export const replaceDiscoverySong = (args: PlaySongArgs, batchEpoch: number) =>
  call<DiscoveryState>("replace_discovery_song", { args, batchEpoch });
export const mutateDiscoveryHistory = (mutation: HistoryMutation) =>
  call<HistoryEntry[]>("mutate_discovery_history", { mutation });
export const getHistoryCovers = (identities: HistoryIdentity[]) =>
  call<HistoryCover[]>("get_history_covers", { identities });
export const startDiscoveryPreview = (settings: Preferences, gui: GuiSettings) =>
  call<DiscoveryState>("start_discovery_preview", { settings, gui });
export const updateDiscoveryPreview = (settings: Preferences, gui: GuiSettings) =>
  call<void>("update_discovery_preview", { settings, gui });
export const endDiscoveryPreview = () => call<void>("end_discovery_preview");
export const setPreviewCloseRect = (rect: PreviewCloseRect) => call<void>("set_preview_close_rect", { rect });
export const listenDiscoveryState = (handler: (state: DiscoveryState) => void) =>
  onDesktopEvent("discovery-state-changed", handler);
export const listenPreviewPointer = (handler: (pointer: PreviewPointer) => void) =>
  onDesktopEvent("preview-pointer", handler);
export const listenPreviewKey = (handler: (key: PreviewKey) => void) => onDesktopEvent("preview-key", handler);
export const listenPreviewAppearance = (handler: (gui: GuiSettings) => void) =>
  onDesktopEvent("preview-appearance", handler);
export const listenPreviewClosed = (handler: () => void) => onDesktopEvent("preview-closed", handler);

/** The only module that depends on Tauri's browser bridge. */
export const desktop = isTauri();

export function call<T>(name: DesktopCommand, args?: Record<string, unknown>): Promise<T> {
  return desktop
    ? invoke<T>(name, args).catch((error) => {
      if (name !== "log_frontend_error")
        invoke("log_frontend_error", { context: name }).catch(() => { });
      throw error;
    })
    : Promise.reject(t("请在 DiscoAS 桌面版中使用此功能。"));
}

export function onDesktopEvent<E extends keyof DesktopEvents>(
  event: E,
  handler: (payload: DesktopEvents[E]) => void | Promise<void>,
): Promise<() => void> {
  if (!desktop) return Promise.resolve(() => {});
  return listen<DesktopEvents[E]>(event, ({ payload }) => handler(payload));
}

export async function hideCurrentWindow() {
  if (desktop) await getCurrentWindow().hide();
}

export async function showCurrentWindow() {
  if (!desktop) return;
  await getCurrentWindow().show();
  await getCurrentWindow().setFocus();
}

export async function isCurrentWindowVisible(): Promise<boolean> {
  if (!desktop) return true;
  const window = getCurrentWindow();
  return await window.isVisible() && !(await window.isMinimized());
}

export function openExternalUrl(url: string): Promise<void> {
  if (desktop) return openUrl(url);
  window.open(url, "_blank", "noopener,noreferrer");
  return Promise.resolve();
}

export async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    throw new Error(t("错误：无法复制，请手动复制"));
  }
}
