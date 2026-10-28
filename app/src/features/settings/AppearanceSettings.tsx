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
          <button type="button" className={gui.followSystemTheme ? "selected" : ""}
            aria-pressed={gui.followSystemTheme}
            onClick={() => set("followSystemTheme", true)}>{t("跟随系统")}</button>
          <button
            type="button"
            className={!gui.followSystemTheme && !gui.nightMode ? "selected" : ""}
            aria-pressed={!gui.followSystemTheme && !gui.nightMode}
            onClick={() => setGui((previous) => ({ ...previous, followSystemTheme: false, nightMode: false }))}
          >
            {t("日间")}
          </button>
          <button
            type="button"
            className={!gui.followSystemTheme && gui.nightMode ? "selected" : ""}
            aria-pressed={!gui.followSystemTheme && gui.nightMode}
            onClick={() => setGui((previous) => ({ ...previous, followSystemTheme: false, nightMode: true }))}
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
          value={gui.fontFamily}
          onChange={(value) => set("fontFamily", value)}
        />
      </SettingRow>
      <SettingRow
        title={t("字号")}
        description={t("同时调整此界面与卡片上的字号，大小按下方比例缩放")}
      >
        <NumberField
          label={t("字号")}
          decimal
          value={gui.fontSize}
          min={10}
          max={24}
          onChange={(v) => set("fontSize", v)}
          suffix="px"
        />
      </SettingRow>
      <SettingRow
        title={t("主界面大小")}
      >
        <ScaleField
          label={t("主界面大小")}
          value={gui.mainWindowScale}
          onChange={(v) => set("mainWindowScale", v)}
        />
      </SettingRow>
      <div className="appearance-actions">
        <button
          type="button"
          className="button secondary"
          onClick={() =>
            setGui(previous => ({
              ...previous,
              followSystemTheme: defaultGuiSettings.followSystemTheme,
              nightMode: defaultGuiSettings.nightMode,
              fontFamily: defaultGuiSettings.fontFamily,
              fontSize: defaultGuiSettings.fontSize,
              mainWindowScale: defaultGuiSettings.mainWindowScale,
            }))
          }
        >
          {t("还原外观")}
        </button>
      </div>
    </section>
  );
}
