import { useCallback, useEffect, useRef, useState } from "react";
import type { AppState, Preferences, GuiSettings, DesktopPreferences } from "../types";
import { emptyState } from "../types";
import { call, desktop, onDesktopEvent, isCurrentWindowVisible } from "../services/desktop";
import type { DesktopCommand, DesktopEvents } from "../services/desktop";
import { samePreferences, sameGuiPreferences } from "../features/settings/preferences";
import { t, errorText } from "../i18n";
import type { Notice } from "./useNotice";

export type StateAction = (key: string, command: DesktopCommand, args?: Record<string, unknown>, success?: string) => Promise<boolean | undefined>;

/** Synchronizes desktop snapshots without overwriting unsaved settings drafts. */
export function useAppState(floating: boolean, notice: Notice) {
  const [state, setState] = useState<AppState>(emptyState);
  const [draft, setDraft] = useState<Preferences>(emptyState.settings);
  const [guiDraft, setGuiDraft] = useState<GuiSettings>(emptyState.guiSettings);
  const [desktopDraft, setDesktopDraft] = useState<DesktopPreferences>(
    emptyState.desktopSettings,
  );
  const savedDesktop = useRef(emptyState.desktopSettings);
  const savedGui = useRef(emptyState.guiSettings);
  const savedSettings = useRef(emptyState.settings);
  const snapshotVersion = useRef(0);
  const applySnapshot = useCallback((next: AppState) => {
    ++snapshotVersion.current;
    const previous = savedSettings.current;
    const previousGui = savedGui.current;
    const previousDesktop = savedDesktop.current;
    setDraft((current) =>
      samePreferences(current, previous)
        ? next.settings
        : { ...current, playlist_albums: next.settings.playlist_albums },
    );
    savedSettings.current = next.settings;
    setGuiDraft((current) =>
      sameGuiPreferences(current, previousGui) ? next.guiSettings : current,
    );
    savedGui.current = next.guiSettings;
    setDesktopDraft((current) =>
      JSON.stringify(current) === JSON.stringify(previousDesktop)
        ? next.desktopSettings
        : current,
    );
    savedDesktop.current = next.desktopSettings;
    setState(next);
  }, []);
  const [startup, setStartup] = useState(desktop);
  const [busy, setBusy] = useState("");
  const reload = useCallback(async () => {
    if (!desktop) return;
    const version = ++snapshotVersion.current;
    const snapshot = await call<AppState>("get_app_state");
    // Event refreshes can finish out of order or after an explicit source/settings mutation.
    if (version === snapshotVersion.current) applySnapshot(snapshot);
  }, [applySnapshot]);
  useEffect(() => {
    reload()
      .catch((e) => notice(errorText(e), true))
      .finally(() => setStartup(false));
    if (desktop && !floating) call("init_preload").catch(() => { });
  }, [reload, notice]);
  useEffect(() => {
    if (!desktop) return;
    let active = true;
    const removers: Array<() => void> = [];
    const attach = <E extends keyof DesktopEvents>(event: E, handler: (payload: DesktopEvents[E]) => void) =>
      onDesktopEvent(event, handler).then((fn) => {
        if (active) removers.push(fn);
        else fn();
      });
    for (const event of ["startup-refresh-failed", "gui-changed", "desktop-changed"] as const)
      attach(event, () => reload().catch((e) => notice(errorText(e), true)));
    attach("cover-refresh-failed", () => {
      notice(t("错误：封面加载失败"), true);
    });
    attach("client-window-result", (payload) => {
      if (payload.warning) notice(payload.warning, true);
    });
    // Playback failure is reported in the main window without reopening the floating discovery view.
    if (!floating) attach("playback-result", (payload) => {
      if (payload.error) isCurrentWindowVisible()
        .then((visible) => { if (visible) notice(errorText(payload.error), true); })
        .catch(() => { });
    });
    return () => {
      active = false;
      removers.forEach((fn) => fn());
    };
  }, [reload, notice]);
  const action = async (
    key: string,
    command: DesktopCommand,
    args?: Record<string, unknown>,
    success?: string,
  ) => {
    if (busy) return;
    setBusy(key);
    try {
      const next = await call<AppState>(command, args);
      applySnapshot(next);
      if (success) notice(success);
      return true;
    } catch (e) {
      notice(errorText(e), true);
      return false;
    } finally {
      setBusy("");
    }
  };
  return {
    state, draft, setDraft, guiDraft, setGuiDraft, desktopDraft, setDesktopDraft,
    applySnapshot, reload, startup, busy, setBusy, action
  };
}

export type AppStateController = ReturnType<typeof useAppState>;
