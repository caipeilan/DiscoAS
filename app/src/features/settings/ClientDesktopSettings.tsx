import { useCallback, useEffect, useState } from "react";
import { SettingRow, Toggle } from "../../components/SettingsControls";
import { Select } from "../../components/Select";
import { errorText, t } from "../../i18n";
import { createClientDesktop, getClientDesktopStatus, showLocalClient, type ClientDesktopStatus } from "../../services/desktop";
import type { DesktopPreferences } from "../../types";

type ClientWindowMode = DesktopPreferences["client_window_mode"];
type BusyAction = "" | "show" | "create" | "detect";

export function ClientDesktopSettings({
  mode,
  onChange,
  separateDesktop,
  onSeparateDesktopChange,
}: {
  mode: ClientWindowMode;
  onChange: (mode: ClientWindowMode) => void;
  separateDesktop: boolean;
  onSeparateDesktopChange: (enabled: boolean) => void;
}) {
  const [busy, setBusy] = useState<BusyAction>("");
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<ClientDesktopStatus | null>(null);
  const [checking, setChecking] = useState(true);
  const options: Array<{ value: ClientWindowMode; label: string }> = [
    { value: "off", label: t("隐藏关闭") },
    { value: "temporary", label: t("临时隐藏（不推荐）") },
    { value: "persistent", label: t("长期隐藏（推荐）") },
  ];

  const refresh = useCallback(async (force = false) => {
    setChecking(true);
    try {
      const next = await getClientDesktopStatus(force);
      setStatus(next); setError(null);
      return next;
    } catch (cause) { setError(errorText(cause)); return null; }
    finally { setChecking(false); }
  }, []);
  useEffect(() => { void refresh(); }, [refresh]);

  const create = async () => {
    setBusy("create"); setMessage(null); setError(null);
    try {
      const next = await createClientDesktop();
      setStatus(next);
      setMessage(t("DiscoAS 独立桌面已就绪"));
    } catch (cause) { setError(errorText(cause)); }
    finally { setBusy(""); }
  };
  const detect = async () => {
    setBusy("detect"); setMessage(null);
    await refresh(true);
    setBusy("");
  };

  const show = async () => {
    setBusy("show");
    setMessage(null);
    setError(null);
    try {
      const result = await showLocalClient();
      setMessage(
        result.found === 0
          ? t("没有找到客户端窗口")
          : result.shown === 0
            ? t("找到客户端，但无法显示")
            : t("已显示客户端窗口"),
      );
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setBusy("");
    }
  };

  return (
    <>
      <SettingRow
        title={t("隐藏客户端")}
        description={t(
          "开启后，播放歌曲时将不再跳出客户端，可通过下方的显示客户端按钮手动呼出",
        )}
      >
        <Select
          label={t("隐藏客户端")}
          value={mode}
          onChange={(value) => onChange(value as ClientWindowMode)}
          options={options}
        />
      </SettingRow>
      <SettingRow
        title={t("创建独立桌面（实验）")}
        description={t("将客户端置于独立的虚拟桌面，减少切歌时的窗口跳动和抢焦点行为，当然有可能更严重")}
      >
        <Toggle label={t("创建独立桌面（实验）")} value={separateDesktop} onChange={onSeparateDesktopChange} />
      </SettingRow>
      <div className="client-desktop-status" role="status" aria-live="polite">
        <div className="client-desktop-line">
          <span className={`client-desktop-dot${status?.configured && status.moveSupported ? "" : " muted"}`} aria-hidden="true" />
          <span>{checking ? t("正在检测 DiscoAS 独立桌面…") : !status ? t("桌面状态获取失败")
            : !status.supported ? t("需要 Windows 11 22H2 或更新版本")
            : !status.moveSupported ? t("当前系统暂无法移动客户端窗口")
            : status.configured ? t("DiscoAS 独立桌面已就绪") : t("尚未创建 DiscoAS 独立桌面")}</span>
          <span className="client-desktop-actions">
            <button type="button" className="button secondary" disabled={busy !== "" || !status?.supported || !!status.configured} onClick={() => void create()}>
              {busy === "create" ? t("创建中") : t("手动创建")}
            </button>
            <button type="button" className="button secondary" disabled={busy !== ""} onClick={() => void detect()}>
              {busy === "detect" ? t("检测中") : t("重新检测")}
            </button>
            <button
              type="button"
              className="button secondary"
              disabled={busy !== ""}
              onClick={() => void show()}
            >
              {busy === "show" ? t("显示中") : t("显示客户端")}
            </button>
          </span>
        </div>
        {message && <p className="client-desktop-hint">{message}</p>}
        {error && <p className="client-desktop-error">{error}</p>}
      </div>
    </>
  );
}
