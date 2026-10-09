import type { GuiSettings } from "../../types";
import { defaultGuiSettings } from "../../types";
import { SettingRow, NumberField, ScaleField, Toggle } from "../../components/SettingsControls";
import { FontPicker } from "../../components/FontPicker";
import { Select } from "../../components/Select";
import { t } from "../../i18n";

export function AppearanceSettings({
  gui,
  setGui,
  previewActive,
  onPreviewChange,
  previewBusy,
}: {
  gui: GuiSettings;
  setGui: React.Dispatch<React.SetStateAction<GuiSettings>>;
  previewActive: boolean;
  onPreviewChange: (active: boolean) => void;
  previewBusy: boolean;
}) {
  const set = <K extends keyof GuiSettings>(key: K, value: GuiSettings[K]) =>
    setGui((previous) => ({ ...previous, [key]: value }));
  return (
    <section className="settings-section appearance-settings">
      <h2>{t("外观与大小")}</h2>
      <SettingRow
        title={t("语言")}
      >
        <Select
          label={t("语言")}
          value={gui.language}
          onChange={(value) => set("language", value)}
          options={[
            { value: "zh_CN", label: t("简体中文") },
            { value: "zh_TW", label: t("繁體中文") },
            { value: "en_US", label: "English" },
          ]}
        />
      </SettingRow>
      <SettingRow
        title={t("日夜模式")}
      >
        <div className="segmented theme-picker">
          <button
            type="button"
            className={!gui.night_mode ? "selected" : ""}
            aria-pressed={!gui.night_mode}
            onClick={() => set("night_mode", false)}
          >
            {t("日间")}
          </button>
          <button
            type="button"
            className={gui.night_mode ? "selected" : ""}
            aria-pressed={gui.night_mode}
            onClick={() => set("night_mode", true)}
          >
            {t("夜间")}
          </button>
        </div>
      </SettingRow>
      <SettingRow
        title={t("字体")}
        description={t("包括此界面与卡片上的字体")}
      >
        <FontPicker
          label={t("字体")}
          value={gui.font_family}
          onChange={(value) => set("font_family", value)}
        />
      </SettingRow>
      <SettingRow
        title={t("字号")}
        description={t("同时调整此界面与卡片上的字号，大小按下方比例缩放")}
      >
        <NumberField
          label={t("字号")}
          decimal
          value={gui.font_size}
          min={10}
          max={24}
          onChange={(v) => set("font_size", v)}
          suffix="px"
        />
      </SettingRow>
      <SettingRow
        title={t("主界面大小")}
      >
        <ScaleField
          label={t("主界面大小")}
          value={gui.setting_size}
          onChange={(v) => set("setting_size", v)}
        />
      </SettingRow>
      <SettingRow
        title={t("卡片大小")}
      >
        <ScaleField
          label={t("卡片大小")}
          value={gui.card_size}
          onChange={(v) => set("card_size", v)}
        />
      </SettingRow>
      <SettingRow
        title={t("退出按钮大小")}
      >
        <ScaleField
          label={t("退出按钮大小")}
          value={gui.cancel_button_size}
          onChange={(v) => set("cancel_button_size", v)}
        />
      </SettingRow>
      <SettingRow title={t("替换按钮大小")}>
        <ScaleField label={t("替换按钮大小")} value={gui.replacement_button_size}
          onChange={(value) => set("replacement_button_size", value)} />
      </SettingRow>
      <SettingRow title={t("发现信息条大小")}>
        <ScaleField label={t("发现信息条大小")} value={gui.discovery_bar_size}
          onChange={(value) => set("discovery_bar_size", value)} />
      </SettingRow>
      <SettingRow title={t("发现界面预览")}>
        <fieldset className="preview-toggle" disabled={previewBusy}>
          <Toggle label={t("发现界面预览")} value={previewActive} onChange={onPreviewChange} />
        </fieldset>
      </SettingRow>
      <div className="appearance-actions">
        <button
          type="button"
          className="button secondary"
          onClick={() =>
            setGui({
              ...defaultGuiSettings,
              language: gui.language,
              user_configured: gui.user_configured,
            })
          }
        >
          {t("恢复默认设置")}
        </button>
      </div>
    </section>
  );
}
