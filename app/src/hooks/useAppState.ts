import { useCallback, useEffect, useRef, useState } from "react";
import type { AppState, Preferences, GuiSettings, DesktopPreferences, PreferencePatch, LibraryPatch } from "../types";
import { emptyState } from "../types";
import { call, desktop, onDesktopEvent, isCurrentWindowVisible } from "../services/desktop";
import type { DesktopCommand, DesktopEvents } from "../services/desktop";
import { samePreferences, sameGuiPreferences } from "../features/settings/preferences";
import { mergeLibraryEntries } from "../features/library/libraryModel";
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
  const savedLibrary = useRef(emptyState.playlists);
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
    savedLibrary.current = next.playlists;
    setState(next);
  }, []);
  const applyPatch = useCallback((patch: PreferencePatch) => {
    if (patch.settings) {
      const previous = savedSettings.current, next = patch.settings;
      setDraft((draft) => samePreferences(draft, previous) ? next : { ...draft, playlist_albums: next.playlist_albums });
      savedSettings.current = next;
    }
    if (patch.guiSettings) {
      const previous = savedGui.current, next = patch.guiSettings;
      setGuiDraft((draft) => sameGuiPreferences(draft, previous) ? next : draft);
      savedGui.current = next;
    }
    if (patch.desktopSettings) {
      const previous = savedDesktop.current, next = patch.desktopSettings;
      setDesktopDraft((draft) => JSON.stringify(draft) === JSON.stringify(previous) ? next : draft);
      savedDesktop.current = next;
    }
    setState((current) => ({ ...current,
      ...(patch.settings ? { settings: patch.settings } : {}),
      ...(patch.guiSettings ? { guiSettings: patch.guiSettings } : {}),
      ...(patch.desktopSettings ? { desktopSettings: patch.desktopSettings, desktopSettingsError: null } : {}),
    }));
  }, []);
  const [startup, setStartup] = useState(desktop);
  const applyLibraryPatch = useCallback(async (patch: LibraryPatch) => {
    const version = ++snapshotVersion.current;
    const playlists = mergeLibraryEntries(savedLibrary.current, patch);
    applyPatch({ settings: patch.settings });
    // A source added before the first snapshot cannot be reconstructed from labels alone.
    if (!playlists) {
      const snapshot = await call<AppState>("get_app_state");
      if (snapshotVersion.current === version) applySnapshot(snapshot);
      return;
    }
    savedLibrary.current = playlists;
    setState(current => ({ ...current, playlists, startupRefreshError: patch.startupRefreshError }));
  }, [applyPatch, applySnapshot]);
  const [busy, setBusy] = useState("");
  useEffect(() => {
    if (!floating && state.desktopSettingsError) notice(errorText(state.desktopSettingsError), true);
  }, [floating, state.desktopSettingsError, notice]);
  const reload = useCallback(async () => {
    if (!desktop) return;
    const version = ++snapshotVersion.current;
    const before = { settings: savedSettings.current, gui: savedGui.current, desktop: savedDesktop.current };
    const snapshot = await call<AppState>("get_app_state");
    // Event refreshes can finish out of order or after an explicit source/settings mutation.
    if (version === snapshotVersion.current) applySnapshot({ ...snapshot,
      settings: savedSettings.current === before.settings ? snapshot.settings : savedSettings.current,
      guiSettings: savedGui.current === before.gui ? snapshot.guiSettings : savedGui.current,
      desktopSettings: savedDesktop.current === before.desktop ? snapshot.desktopSettings : savedDesktop.current,
    });
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
    attach("startup-refresh-failed", () => reload().catch((e) => notice(errorText(e), true)));
    attach("preferences-changed", applyPatch);
    for (const event of ["gui-changed", "desktop-changed"] as const)
      attach(event, (patch) => patch ? applyPatch(patch) : void reload().catch((e) => notice(errorText(e), true)));
    attach("cover-refresh-failed", () => {
      notice(t("错误：封面加载失败"), true);
    });
    attach("client-window-result", (payload) => {
      if (payload.warning) notice(payload.warning, true);
    });
    attach("hand-error", (error) => { if (!floating) notice(errorText(error), true); });
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
  }, [reload, notice, applyPatch]);
  const action = async (
    key: string,
    command: DesktopCommand,
    args?: Record<string, unknown>,
    success?: string,
  ) => {
    if (busy || startup) return;
    setBusy(key);
    try {
      const next = await call<LibraryPatch>(command, args);
      await applyLibraryPatch(next);
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
    applySnapshot, applyPatch, applyLibraryPatch, reload, startup, busy, setBusy, action
  };
}

export type AppStateController = ReturnType<typeof useAppState>;
