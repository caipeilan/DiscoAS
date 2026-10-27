import type { GuiSettings } from "../../types";
import { defaultGuiSettings } from "../../types";
import { SettingRow, ScaleField } from "../../components/SettingsControls";
import { DisplayMonitorSetting } from "./DisplayMonitorSetting";
import { t } from "../../i18n";

export function DiscoveryAppearanceSettings({ gui, setGui, monitor, onMonitorChange }: {
  gui: GuiSettings;
  setGui: React.Dispatch<React.SetStateAction<GuiSettings>>;
  monitor: string;
  onMonitorChange: (value: string) => void;
}) {
  const sizes = [
    ["card_size", "卡片大小"],
    ["cancel_button_size", "退出按钮大小"],
    ["replacement_button_size", "替换按钮大小"],
    ["discovery_bar_size", "发现信息条大小"],
  ] as const;
  return <section className="settings-section">
    <h2>{t("发现外观与布局")}</h2>
    <DisplayMonitorSetting value={monitor} onChange={onMonitorChange} />
    {sizes.map(([key, label]) => <SettingRow key={key} title={t(label)}>
      <ScaleField label={t(label)} value={gui[key]} onChange={value => setGui(previous => ({ ...previous, [key]: value }))} />
    </SettingRow>)}
    <div className="appearance-actions">
      <button type="button" className="button secondary" onClick={() => setGui(previous => ({
        ...previous,
        card_size: defaultGuiSettings.card_size,
        cancel_button_size: defaultGuiSettings.cancel_button_size,
        replacement_button_size: defaultGuiSettings.replacement_button_size,
        discovery_bar_size: defaultGuiSettings.discovery_bar_size,
      }))}>{t("还原发现大小")}</button>
    </div>
  </section>;
}
