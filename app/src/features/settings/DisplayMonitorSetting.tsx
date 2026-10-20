import { useEffect, useState } from "react";
import { SettingRow } from "../../components/SettingsControls";
import { Select } from "../../components/Select";
import { t } from "../../i18n";
import { getDisplayMonitors, type DisplayMonitor } from "../../services/desktop";

/**
 * The stored value is "current", the legacy "primary", or a Windows display
 * device id. Physical options are read at runtime so labels use the monitor's
 * model/hardware name instead of a generic "primary" label.
 */
export function DisplayMonitorSetting({
  value,
  onChange,
}: {
  value: string;
  onChange: (value: string) => void;
}) {
  const [monitors, setMonitors] = useState<DisplayMonitor[]>([]);

  useEffect(() => {
    let active = true;
    getDisplayMonitors()
      .then((next) => {
        if (active) setMonitors(next);
      })
      .catch(() => {
        // Browser preview and non-Windows hosts keep the "current" fallback.
      });
    return () => {
      active = false;
    };
  }, []);

  const options = [{ value: "current", label: t("当前显示器") }];
  for (const monitor of monitors) options.push({ value: monitor.id, label: monitor.name });

  const primary = monitors.find((monitor) => monitor.primary);
  const selected =
    monitors.some((monitor) => monitor.id === value) || value === "current"
      ? value
      : value === "primary"
        ? primary?.id ?? "current"
        : "current";

  return (
    <SettingRow title={t("发现界面显示器")}>
      <Select
        label={t("发现界面显示器")}
        value={selected}
        onChange={onChange}
        options={options}
      />
    </SettingRow>
  );
}
