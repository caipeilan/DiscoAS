import type { AppState, Preferences, GuiSettings, DesktopPreferences, PreferencePatch } from "../../types";
import type { AppStateController } from "../../hooks/useAppState";
import type { Notice } from "../../hooks/useNotice";
import { samePreferences, sameGuiPreferences } from "./preferences";
import { call } from "../../services/desktop";
import { t, errorText } from "../../i18n";

/** Settings mutations remain separate from the settings form and window appearance. */
export function useSettings(app: AppStateController, notice: Notice, reportSaveError?: (message: string) => void) {
  const { state, draft, guiDraft, desktopDraft, busy, setBusy, applySnapshot, applyPatch, setDraft, setGuiDraft, setDesktopDraft } = app;
  const migrate = async () => {
    if (busy) return;
    if (
      !samePreferences(draft, state.settings) ||
      !sameGuiPreferences(guiDraft, state.guiSettings) ||
      JSON.stringify(desktopDraft) !== JSON.stringify(state.desktopSettings)
    ) {
      notice(t("请先保存未保存的设置，再迁移旧版数据。"), true);
      return;
    }
    setBusy("migrate");
    try {
      const next = await call<AppState | null>("import_legacy");
      if (next) {
        applySnapshot(next);
        notice(t("旧版歌单和发现设置已迁移。"));
      }
    } catch (e) {
      notice(errorText(e), true);
    } finally {
      setBusy("");
    }
  };
  const saveSettings = async (
    settings: Preferences,
    gui: GuiSettings,
    preferences: DesktopPreferences,
  ) => {
    if (busy) return;
    reportSaveError?.("");
    setBusy("settings");
    try {
      await call("set_shortcut_recording", { recording: false });
      if (!samePreferences(settings, state.settings)) {
        const next = await call<PreferencePatch>("save_preferences", { settings });
        applyPatch(next);
        setDraft(next.settings!);
      }
      if (!sameGuiPreferences(gui, state.guiSettings)) {
        const next = await call<PreferencePatch>("save_gui_preferences", {
          settings: gui,
        });
        applyPatch(next);
        setGuiDraft(next.guiSettings!);
      }
      if (
        JSON.stringify(preferences) !== JSON.stringify(state.desktopSettings)
      ) {
        const next = await call<PreferencePatch>("save_desktop_preferences", {
          settings: preferences,
        });
        applyPatch(next);
        setDesktopDraft(next.desktopSettings!);
      }
      notice(t("设置已保存。"));
    } catch (e) {
      if (reportSaveError) reportSaveError(errorText(e));
      else notice(errorText(e), true);
    } finally {
      setBusy("");
    }
  };
  const discardSettings = () => {
    if (busy) return;
    setDraft(state.settings);
    setGuiDraft(state.guiSettings);
    setDesktopDraft(state.desktopSettings);
    reportSaveError?.("");
  };
  const chooseMysteryCover = async () => {
    try {
      const path = await call<string | null>("choose_mystery_cover");
      if (path) setDraft((previous) => ({ ...previous, mystery_song_cover: path }));
    } catch (e) {
      notice(errorText(e), true);
    }
  };
  const openDataFolder = () => {
    call("open_data_folder").catch((e) => notice(errorText(e), true));
  };
  const openLogFolder = () => {
    call("open_log_folder").catch((e) => notice(errorText(e), true));
  };
  return { migrate, saveSettings, discardSettings, chooseMysteryCover, openDataFolder, openLogFolder };
}
