import type { DesktopPreferences } from "../../types";
import { SettingRow } from "../../components/SettingsControls";
import { Select } from "../../components/Select";
import { t } from "../../i18n";

export function RenderingSettings({ value, onChange }: {
  value: DesktopPreferences["rendering_mode"];
  onChange: (value: DesktopPreferences["rendering_mode"]) => void;
}) {
  return <section className="settings-section">
    <h2>{t("渲染方式")}</h2>
    <SettingRow title={t("渲染方式")} description={t("渲染方式保存后重启生效。显卡偏好为实验选项，取决于运行时与可用显卡。") }>
      <Select label={t("渲染方式")} value={value} options={[
        { value: "auto", label: t("自动（硬件加速）") },
        { value: "integrated", label: t("优先集显") },
        { value: "discrete", label: t("优先独显") },
        { value: "cpu", label: t("CPU 软件渲染") },
      ]} onChange={selected => onChange(selected as DesktopPreferences["rendering_mode"])} />
    </SettingRow>
  </section>;
}
