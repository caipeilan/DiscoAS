import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, emitTo } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openUrl } from "@tauri-apps/plugin-opener";
import { t } from "../i18n";
import type { TrayMenuSnapshot } from "../features/tray/trayMenuModel";
import type { LibraryProgress, DiscoveryState, DiscoveryStatus, PreferencePatch, GuiSettings,
  HistoryEntry, HistoryMutation, HistoryIdentity, HistoryCover, PreviewPointer, PreviewKey, PreviewCloseRect, HandSnapshot, HandKey, HandVisibility, HandSurface, HandDockSnapshot, HandPoint,
  HandDragSnapshot, HandDragIntent, HandDragEnd } from "../types";

export interface PlaySongArgs { platform: string; songId: string; playlistId: string; typename: string }

import type { AutoHandState } from "../features/hand/autoHandState";
import type { OnboardingState } from "../features/onboarding/onboardingState";

export type DesktopCommand =
  | "get_onboarding_state" | "advance_onboarding" | "start_onboarding"
  | "get_auto_hand_state"
  | "set_auto_hand_enabled" | "open_create_hand" | "create_hand_card"
  | "hand_ready" | "present_hand" | "hand_arrival_ready" | "set_hand_hit_regions" | "collect_hand_card" | "show_collected_hand_card" | "play_hand_card"
  | "discard_hand_card" | "clear_hand" | "reorder_hand" | "toggle_hand" | "hide_hand" | "toggle_hand_expanded"
  | "begin_hand_dock_drag" | "save_hand_dock"
  | "hand_dock_ready" | "position_hand_dock" | "set_hand_dock_hit_regions"
  | "end_hand_preview"
  | "begin_hand_drag" | "present_hand_drag" | "set_hand_drag_intent" | "end_hand_drag" | "hand_drag_ready"
  | "refresh_hand_discovery" | "hand_discovery_displayed" | "replace_hand_discovery_card" | "collect_hand_discovery_card" | "play_hand_discovery_card" | "reorder_hand_discovery"
  | "tray_menu_ready"
  | "present_tray_menu"
  | "dismiss_tray_menu"
  | "tray_menu_action"
  | "get_app_state"
  | "init_preload"
  | "get_image"
  | "import_playlist"
  | "import_local_audio"
  | "choose_local_audio"
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
  | "discover_batch"
  | "get_discovery_state"
  | "get_discovery_preferences"
  | "replace_discovery_song"
  | "mutate_discovery_history"
  | "get_history_covers"
  | "end_discovery_preview"
  | "set_preview_close_rect"
  | "report_cancelled"
  | "play_song"
  | "show_discover"
  | "show_main"
  | "show_local_client"
  | "get_client_desktop_status"
  | "create_client_desktop"
  | "get_client_hide_enabled"
  | "get_display_monitors"
  | "set_shortcut_recording"
  | "finish_startup"
  | "startup_ready"
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

export interface ShowClientResult {
  found: number;
  shown: number;
}
export const showLocalClient = () => call<ShowClientResult>("show_local_client");
export const getClientHideEnabled = () => call<boolean>("get_client_hide_enabled");

export interface ClientDesktopStatus {
  supported: boolean;
  moveSupported: boolean;
  configured: boolean;
  desktopId: string | null;
  desktopName: string;
}
export const getClientDesktopStatus = (refresh = false) =>
  call<ClientDesktopStatus>("get_client_desktop_status", { refresh });
export const createClientDesktop = () => call<ClientDesktopStatus>("create_client_desktop");

export interface DisplayMonitor {
  id: string;
  name: string;
  primary: boolean;
}
export const getDisplayMonitors = () => call<DisplayMonitor[]>("get_display_monitors");

export interface DesktopEvents {
  "onboarding-changed": OnboardingState;
  "open-create-hand": void;
  "auto-hand-state-changed": AutoHandState;
  "open-settings-requested": { category: "playback" };
  "hand-dock-state-changed": HandDockSnapshot;
  "hand-dock-moved": { generation: number; point: HandPoint; surface: HandSurface };
  "hand-dock-hide": number;
  "hand-clear-requested": void;
  "hand-clear-finished": string | null;
  "hand-busy-changed": boolean;
  "hand-state-changed": HandSnapshot;
  "hand-visibility-changed": HandVisibility;
  "hand-hide": number;
  "hand-escape": void;
  "hand-pointer": [number, number, boolean];
  "hand-key": HandKey;
  "hand-preview-closed": void;
  "hand-drag-state": HandDragSnapshot;
  "hand-drag-intent": HandDragIntent;
  "hand-drag-end": HandDragEnd;
  "hand-drag-presented": number;
  "hand-error": string;
  "hand-discovery-play-requested": string;
  "hand-discovery-refresh-requested": void;
  "tray-menu-open": TrayMenuSnapshot;
  "library-changed": { discoveryInvalidated: boolean } | void;
  "library-progress": LibraryProgress;
  "startup-refresh-failed": void;
  "preferences-changed": PreferencePatch;
  "gui-changed": PreferencePatch | void;
  "desktop-changed": PreferencePatch | void;
  "cover-refresh-failed": void;
  "client-window-result": { platform: string; minimized: boolean; warning: string | null };
  "playback-result": { platform: string; songId: string; success: boolean; confirmed: boolean; error: string | null };
  "discovery-state-changed": DiscoveryState;
  "discovery-status-changed": DiscoveryStatus;
  "discovery-history-changed": { source: string | null } | null;
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
export const mutateDiscoveryHistory = (mutation: HistoryMutation, source?: string) =>
  call<HistoryEntry[]>("mutate_discovery_history", { mutation, source });
export const getHistoryCovers = (identities: HistoryIdentity[]) =>
  call<HistoryCover[]>("get_history_covers", { identities });
export const endDiscoveryPreview = () => call<void>("end_discovery_preview");
export const setPreviewCloseRect = (rect: PreviewCloseRect) => call<void>("set_preview_close_rect", { rect });
export const listenDiscoveryState = (handler: (state: DiscoveryState) => void) =>
  onDesktopEvent("discovery-state-changed", handler);
export const listenDiscoveryStatus = (handler: (state: DiscoveryStatus) => void) => onDesktopEvent("discovery-status-changed", handler);
export const listenPreviewPointer = (handler: (pointer: PreviewPointer) => void) =>
  onDesktopEvent("preview-pointer", handler);
export const listenPreviewKey = (handler: (key: PreviewKey) => void) => onDesktopEvent("preview-key", handler);
export const listenPreviewAppearance = (handler: (gui: GuiSettings) => void) =>
  onDesktopEvent("preview-appearance", handler);
export const listenPreviewClosed = (handler: () => void) => onDesktopEvent("preview-closed", handler);

/** The only module that depends on Tauri's browser bridge. */
export const desktop = isTauri();

export async function listenFileDrop(handler: (paths: string[]) => void): Promise<() => void> {
  if (!desktop) return () => {};
  return getCurrentWindow().onDragDropEvent(({ payload }) => {
    if (payload.type === "drop") handler(payload.paths);
  });
}

export async function mainWindowAction(action: "minimize" | "maximize" | "close" | "drag"): Promise<void> {
  if (!desktop) return;
  const window = getCurrentWindow();
  if (action === "minimize") await window.minimize();
  else if (action === "maximize") await window.toggleMaximize();
  else if (action === "close") await window.close();
  else await window.startDragging();
}

export async function watchMainWindowState(
  changed: (state: { maximized: boolean; focused: boolean }) => void,
  failed: (error: unknown) => void,
): Promise<() => void> {
  if (!desktop) return () => {};
  const window = getCurrentWindow();
  let state = { maximized: false, focused: true };
  let stopped = false, resizeTimer: ReturnType<typeof setTimeout> | undefined;
  const update = (next: typeof state) => {
    if (!stopped && (next.maximized !== state.maximized || next.focused !== state.focused)) { state = next; changed(next); }
  };
  const stopResize = await window.onResized(() => {
    clearTimeout(resizeTimer);
    resizeTimer = setTimeout(() => { void window.isMaximized().then((maximized) => update({ ...state, maximized })).catch(failed); }, 80);
  });
  const stopFocus = await window.onFocusChanged(({ payload }) => update({ ...state, focused: payload }));
  const [maximized, focused] = await Promise.all([window.isMaximized(), window.isFocused()]);
  update({ maximized, focused });
  return () => { stopped = true; clearTimeout(resizeTimer); stopResize(); stopFocus(); };
}

export const requestHandClear = () => emitTo("hand", "hand-clear-requested");
export const notifyHandClear = (error: string | null) => emitTo("hand-dock", "hand-clear-finished", error);
export const notifyHandBusy = (busy: boolean) => emitTo("hand-dock", "hand-busy-changed", busy);
export const requestHandDiscoveryRefresh = () => emitTo("hand", "hand-discovery-refresh-requested");
export async function openPlaybackSettings() {
  if (!desktop) return;
  await emitTo("main", "open-settings-requested", { category: "playback" });
  await call("show_main");
}

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
