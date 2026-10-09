import type { AppState, Preferences, GuiSettings, DesktopPreferences } from "../../types";
import { Icon } from "../../Icon";
import { ShortcutRecorder } from "../../ShortcutRecorder";
import { SettingRow, NumberField, Toggle } from "../../components/SettingsControls";
import { Select } from "../../components/Select";
import { samePreferences, sameGuiPreferences } from "./preferences";
import { AppearanceSettings } from "./AppearanceSettings";
import { t, errorText } from "../../i18n";
import { WeightingSettings } from "./WeightingSettings";
import { SpotifySettings } from "./SpotifySettings";
import { BrowserSettings } from "./BrowserSettings";
import { KeyboardSettings } from "./KeyboardSettings";
import { useDiscoveryPreview } from "./useDiscoveryPreview";
import "./settingsDiscovery.css";

export function Settings({
  state,
  draft,
  setDraft,
  guiDraft,
  setGuiDraft,
  desktopDraft,
  setDesktopDraft,
  busy,
  save,
  migrate,
  chooseMysteryCover,
  openDataFolder,
  openLogFolder,
}: {
  state: AppState;
  draft: Preferences;
  setDraft: React.Dispatch<React.SetStateAction<Preferences>>;
  guiDraft: GuiSettings;
  setGuiDraft: React.Dispatch<React.SetStateAction<GuiSettings>>;
  desktopDraft: DesktopPreferences;
  setDesktopDraft: React.Dispatch<React.SetStateAction<DesktopPreferences>>;
  busy: string;
  save: (
    settings: Preferences,
    gui: GuiSettings,
    desktop: DesktopPreferences,
  ) => Promise<unknown>;
  migrate: () => Promise<void>;
  chooseMysteryCover: () => Promise<void>;
  openDataFolder: () => void;
  openLogFolder: () => void;
}) {
  const preview = useDiscoveryPreview(draft, guiDraft);
  const set = <K extends keyof Preferences>(key: K, value: Preferences[K]) =>
    setDraft((previous) => ({ ...previous, [key]: value }));
  const dirty =
    !samePreferences(draft, state.settings) ||
    !sameGuiPreferences(guiDraft, state.guiSettings) ||
    JSON.stringify(desktopDraft) !== JSON.stringify(state.desktopSettings);
  const enabledSource = state.playlists.find((entry) => entry.enabled);
  const total =
    draft.number_of_discovered_songs +
    (draft.have_mystery_song ? draft.num_of_mystery_song : 0);
  const invalidCount = draft.number_of_discovered_songs < 1 || total > 15;
  return (
    <>
      <header className="page-header settings-topbar">
        <div>
          <h1>{t("设置")}</h1>
        </div>
        <button
          className="button primary"
          disabled={!!busy || !dirty || invalidCount}
          onClick={() => save(draft, guiDraft, desktopDraft)}
        >
          <Icon
            name={busy === "settings" ? "refresh" : "check"}
            className={busy === "settings" ? "spin" : ""}
          />
          {busy === "settings" ? t("保存中") : t("保存设置")}
        </button>
      </header>
      <fieldset className="settings-fields" disabled={!!busy}>
        <AppearanceSettings gui={guiDraft} setGui={setGuiDraft} previewActive={preview.active}
          onPreviewChange={preview.change} previewBusy={preview.pending} />
        {preview.error && <p className="inline-error padded-error" role="alert">{preview.error}</p>}
        <section className="settings-section">
          <h2>{t("发现偏好")}</h2>
          <SettingRow
            title={t("普通歌曲数量")}
            description={t("普通歌曲与秘密歌曲数量合计最多15首")}
          >
            <NumberField
              label={t("普通歌曲数量")}
              value={draft.number_of_discovered_songs}
              min={1}
              max={15}
              onChange={(v) => set("number_of_discovered_songs", v)}
            />
          </SettingRow>
          <SettingRow
            title={t("神秘歌曲")}
            description={t("启用后，每次发现皆会出现一个隐藏歌曲信息的选项")}
          >
            <Toggle
              label={t("包含神秘歌曲")}
              value={draft.have_mystery_song}
              onChange={(v) => set("have_mystery_song", v)}
            />
          </SettingRow>
          {draft.have_mystery_song && (
            <SettingRow
              title={t("神秘歌曲数量")}
            >
              <NumberField
                label={t("神秘歌曲数量")}
                value={draft.num_of_mystery_song}
                min={0}
                max={14}
                onChange={(v) => set("num_of_mystery_song", v)}
              />
            </SettingRow>
          )}
          {invalidCount && (
            <p className="inline-error padded-error">
              {t("普通与神秘歌曲合计不能超过 15 首，请调整数量。")}
            </p>
          )}
          <SettingRow
            title={t("取消后换一批")}
            description={t("即未选择歌曲并退出后不保留已有选项")}
          >
            <Toggle
              label={t("取消后换一批")}
              value={draft.refreshing_after_cancel}
              onChange={(v) => set("refreshing_after_cancel", v)}
            />
          </SettingRow>
          <SettingRow
            title={t("预加载批数")}
            description={t("提前加载下一批歌曲信息以加快画面加载，0表示关闭")}
          >
            <NumberField
              label={t("预加载批数")}
              value={draft.cache_batches}
              min={0}
              max={5}
              onChange={(v) => set("cache_batches", v)}
              suffix={t("批")}
            />
          </SettingRow>
          <SettingRow title={t("预加载批次间去重")}>
            <Toggle label={t("预加载批次间去重")} value={draft.preload_deduplication}
              onChange={(value) => set("preload_deduplication", value)} />
          </SettingRow>
          <SettingRow title={t("近期歌曲排除")} description={t("按平台与歌曲排除，切换同平台歌单后仍生效，排除数量超上限后最早被记录的歌曲会被释放")}>
            <Select
              label={t("近期歌曲排除")}
              value={draft.history_exclusion}
              onChange={(value) => set("history_exclusion", value as Preferences["history_exclusion"])}
              options={[
                { value: "off", label: t("不排除") },
                { value: "selected", label: t("排除已选择歌曲") },
                { value: "discovered", label: t("排除全部入选歌曲") },
              ]}
            />
          </SettingRow>
          <SettingRow title={t("排除上限")} description={t("最高10000首，仅排除实际展示的歌曲，不排除平台自动播放的曲目")}>
            <NumberField label={t("排除上限")} value={draft.history_limit} min={0} max={10000} onChange={(value) => set("history_limit", value)} />
          </SettingRow>
          <SettingRow title={t("每次发现替换数")}>
            <NumberField label={t("每次发现替换数")} value={draft.replacement_limit} min={0} max={100}
              suffix={t("次")} onChange={(value) => set("replacement_limit", value)} />
          </SettingRow>
        </section>
        <WeightingSettings value={draft.discovery_weighting} onChange={(value) => set("discovery_weighting", value)}
          songCount={enabledSource?.songCount || 0} sourceIdentity={enabledSource ? JSON.stringify([enabledSource.platform, enabledSource.kind, enabledSource.id]) : ""}
          drawCount={total} />
        <section className="settings-section">
          <h2>{t("窗口与播放")}</h2>
          <SettingRow title={t("发现界面显示器")}>
            <Select
              label={t("发现界面显示器")}
              value={desktopDraft.overlay_monitor}
              onChange={(value) => setDesktopDraft((current) => ({ ...current, overlay_monitor: value as DesktopPreferences["overlay_monitor"] }))}
              options={[
                { value: "current", label: t("当前显示器") },
                { value: "primary", label: t("主显示器") },
              ]}
            />
          </SettingRow>
          <SettingRow title={t("播放后最小化客户端")}>
            <Toggle label={t("播放后最小化客户端")} value={desktopDraft.minimize_after_playback} onChange={(value) => setDesktopDraft((current) => ({ ...current, minimize_after_playback: value }))} />
          </SettingRow>
          {desktopDraft.minimize_after_playback && <SettingRow title={t("最小化等待时间")} description={t("为给客户端留出切歌时间，不宜设置过短，最高30秒")}>
            <NumberField decimal label={t("最小化等待时间")} value={desktopDraft.minimize_delay_seconds} min={0} max={30} suffix={t("秒")} onChange={(value) => setDesktopDraft((current) => ({ ...current, minimize_delay_seconds: value }))} />
          </SettingRow>}
        </section>
        <SpotifySettings settings={desktopDraft} setSettings={setDesktopDraft} />
        <BrowserSettings settings={desktopDraft} setSettings={setDesktopDraft} />
        <section className="settings-section">
          <h2>{t("快捷键与封面")}</h2>
          <SettingRow
            title={t("全局快捷键")}
            description={t("在其他应用中也能唤出发现窗口，留空可关闭。")}
          >
            <ShortcutRecorder
              value={draft.shortcut_key}
              onChange={(value) => set("shortcut_key", value)}
            />
          </SettingRow>
          {state.shortcutError && (
            <p className="inline-error padded-error">
              {errorText(state.shortcutError)}
            </p>
          )}
          <SettingRow title={t("神秘歌曲封面")} description={t("留空使用默认问号")}>
            <div className="cover-setting">
              <input
                value={draft.mystery_song_cover}
                aria-label={t("神秘歌曲封面")}
                onChange={(e) => set("mystery_song_cover", e.target.value)}
                placeholder={t("图片链接或本地图片路径")}
              />
            <button
              className="button secondary"
              onClick={chooseMysteryCover}
            >
              <Icon name="folder" />
              {t("选择图片")}
            </button>
            </div>
          </SettingRow>
        </section>
        <KeyboardSettings value={draft.discovery_keybindings} globalShortcut={draft.shortcut_key} onChange={(value) => set("discovery_keybindings", value)} disabled={!!busy} />
        <section className="settings-section">
          <h2>{t("启动与后台运行")}</h2>
          <SettingRow
            title={t("开机自启动")}
          >
            <Toggle
              label={t("开机自启动")}
              value={desktopDraft.launch_at_login}
              onChange={(value) =>
                setDesktopDraft((s) => ({ ...s, launch_at_login: value }))
              }
            />
          </SettingRow>
          <SettingRow
            title={t("仅托盘启动")}
          >
            <Toggle
              label={t("仅托盘启动")}
              value={desktopDraft.start_hidden}
              onChange={(value) =>
                setDesktopDraft((s) => ({ ...s, start_hidden: value }))
              }
            />
          </SettingRow>
          <SettingRow
            title={t("启动更新歌单")}
          >
            <Toggle
              label={t("启动更新歌单")}
              value={desktopDraft.update_on_startup}
              onChange={(value) =>
                setDesktopDraft((s) => ({ ...s, update_on_startup: value }))
              }
            />
          </SettingRow>
        </section>
        <section className="settings-section">
          <h2>{t("本地数据")}</h2>
          <SettingRow
            title={t("迁移旧版数据")}
          >
            <button
              className="button secondary"
              disabled={!!busy}
              onClick={migrate}
            >
              <Icon name="import" />
              {busy === "migrate" ? t("迁移中") : t("选择旧版文件夹")}
            </button>
          </SettingRow>
          <SettingRow
            title={t("数据文件夹")}
          >
            <button
              className="button secondary"
              onClick={openDataFolder}
            >
              <Icon name="folder" />
              {t("打开")}
            </button>
          </SettingRow>
          <SettingRow
            title={t("错误日志")}
          >
            <button
              className="button secondary"
              onClick={openLogFolder}
            >
              <Icon name="folder" />
              {t("打开日志目录")}
            </button>
          </SettingRow>
        </section>
      </fieldset>
      {dirty && (
        <p className="unsaved-note">
          {t("有尚未保存的设置，外观已在当前窗口预览。")}
        </p>
      )}
    </>
  );
}
