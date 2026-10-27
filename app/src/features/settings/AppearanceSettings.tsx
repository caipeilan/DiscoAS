import type { GuiSettings } from "../../types";
import { defaultGuiSettings } from "../../types";
import { SettingRow, NumberField, ScaleField } from "../../components/SettingsControls";
import { FontPicker } from "../../components/FontPicker";
import { Select } from "../../components/Select";
import { t } from "../../i18n";

export function AppearanceSettings({
  gui,
  setGui,
}: {
  gui: GuiSettings;
  setGui: React.Dispatch<React.SetStateAction<GuiSettings>>;
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
        title={t("外观")}
      >
        <div className="segmented theme-picker">
          <button type="button" className={gui.follow_system_theme ? "selected" : ""}
            aria-pressed={gui.follow_system_theme}
            onClick={() => set("follow_system_theme", true)}>{t("跟随系统")}</button>
          <button
            type="button"
            className={!gui.follow_system_theme && !gui.night_mode ? "selected" : ""}
            aria-pressed={!gui.follow_system_theme && !gui.night_mode}
            onClick={() => setGui((previous) => ({ ...previous, follow_system_theme: false, night_mode: false }))}
          >
            {t("日间")}
          </button>
          <button
            type="button"
            className={!gui.follow_system_theme && gui.night_mode ? "selected" : ""}
            aria-pressed={!gui.follow_system_theme && gui.night_mode}
            onClick={() => setGui((previous) => ({ ...previous, follow_system_theme: false, night_mode: true }))}
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
      <div className="appearance-actions">
        <button
          type="button"
          className="button secondary"
          onClick={() =>
            setGui(previous => ({
              ...previous,
              follow_system_theme: defaultGuiSettings.follow_system_theme,
              night_mode: defaultGuiSettings.night_mode,
              font_family: defaultGuiSettings.font_family,
              font_size: defaultGuiSettings.font_size,
              setting_size: defaultGuiSettings.setting_size,
            }))
          }
        >
          {t("还原外观")}
        </button>
      </div>
    </section>
  );
}
