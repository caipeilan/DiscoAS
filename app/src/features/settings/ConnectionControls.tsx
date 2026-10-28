import { useId, type ReactNode } from "react";
import { SettingRow } from "../../components/SettingsControls";
import { Icon } from "../../Icon";
import { t } from "../../i18n";
import "./connectionSettings.css";

export function ConnectionSummary({ status, connected, attention, busy, open, onToggle }: {
  status: string; connected: boolean; attention?: boolean; busy: boolean;
  open: boolean; onToggle: () => void;
}) {
  const id = useId();
  return <SettingRow title={t("扩展连接")} className="connection-row">
    <span className="connection-state" role="status" id={id}
      data-tone={attention ? "error" : connected ? "connected" : "idle"}>
      {busy ? <Icon name="refresh" size={14} className="spin" /> : <span className="connection-dot" />}
      {status}
    </span>
    <button type="button" className={`button ${!connected && !open ? "primary" : "secondary"}`}
      aria-describedby={id} aria-expanded={open} disabled={busy} onClick={onToggle}>
      {open ? t("收起") : connected ? t("管理…") : t("连接…")}
    </button>
  </SettingRow>;
}

export function ConnectionNotice({ children, error = false }: { children: ReactNode; error?: boolean }) {
  return <div className="connection-message" role={error ? "alert" : "status"} data-error={error || undefined}>
    <Icon name="info" size={17} /><p>{children}</p>
  </div>;
}
