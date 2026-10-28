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
    ["discoveryCardScale", "卡片大小"],
    ["cancelButtonScale", "退出按钮大小"],
    ["replacementButtonScale", "替换按钮大小"],
    ["discoveryBarScale", "发现信息条大小"],
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
        discoveryCardScale: defaultGuiSettings.discoveryCardScale,
        cancelButtonScale: defaultGuiSettings.cancelButtonScale,
        replacementButtonScale: defaultGuiSettings.replacementButtonScale,
        discoveryBarScale: defaultGuiSettings.discoveryBarScale,
      }))}>{t("还原发现大小")}</button>
    </div>
  </section>;
}
