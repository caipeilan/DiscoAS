import { useCallback, useEffect, useState } from "react";
import { SettingRow } from "../../components/SettingsControls";
import { Select } from "../../components/Select";
import { errorText, t } from "../../i18n";
import {
  createClientDesktop,
  getClientDesktopStatus,
  showLocalClient,
  type ClientDesktopStatus,
} from "../../services/desktop";
import type { DesktopPreferences } from "../../types";

type ClientWindowMode = DesktopPreferences["client_window_mode"];
type BusyAction = "" | "create" | "show" | "detect";

export function ClientDesktopSettings({
  mode,
  onChange,
}: {
  mode: ClientWindowMode;
  onChange: (mode: ClientWindowMode) => void;
}) {
  const [status, setStatus] = useState<ClientDesktopStatus | null>(null);
  const [busy, setBusy] = useState<BusyAction>("");
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const enabled = mode !== "off";

  const refresh = useCallback(async (): Promise<ClientDesktopStatus | null> => {
    try {
      const next = await getClientDesktopStatus();
      setStatus(next);
      setError(null);
      return next;
    } catch (cause) {
      setError(errorText(cause));
      return null;
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (!enabled || !status?.supported || status.configured) return;
    const timer = window.setInterval(() => {
      void refresh();
    }, 2000);
    return () => window.clearInterval(timer);
  }, [enabled, status?.supported, status?.configured, refresh]);

  const create = async () => {
    setBusy("create");
    setMessage(null);
    setError(null);
    try {
      const next = await createClientDesktop();
      setStatus(next);
      setMessage(t("已创建后台桌面，请在任务视图中重命名为 DiscoAS，然后点重新检测。"));
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setBusy("");
    }
  };

  const detect = async () => {
    setBusy("detect");
    setMessage(null);
    const next = await refresh();
    if (next) {
      setMessage(
        next.configured
          ? t("已重新检测到 DiscoAS 桌面")
          : t("尚未找到 DiscoAS 桌面"),
      );
    }
    setBusy("");
  };

  const show = async () => {
    setBusy("show");
    setMessage(null);
    setError(null);
    try {
      const count = await showLocalClient();
      setMessage(
        count > 0
          ? t("已显示客户端窗口")
          : t("没有找到可显示的客户端窗口"),
      );
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setBusy("");
    }
  };

  const supported = status?.supported !== false;
  const options = [
    { value: "off", label: t("关闭") },
    { value: "temporary", label: t("临时存放"), disabled: !supported },
    { value: "persistent", label: t("长期存放（推荐）"), disabled: !supported },
  ];

  return (
    <>
      <SettingRow
        title={t("客户端单独桌面")}
        description={t(
          "开启后，客户端将存放于一个独立的桌面中，以避免播放歌曲时客户端界面在当前屏幕中弹出",
        )}
      >
        <Select
          label={t("客户端单独桌面")}
          value={mode}
          onChange={(value) => onChange(value as ClientWindowMode)}
          options={options}
        />
      </SettingRow>
      {enabled && (
        <div className="client-desktop-status" role="status" aria-live="polite">
          {!status ? (
            <div className="client-desktop-line">
              <span>{t("正在检测后台桌面…")}</span>
              <span className="client-desktop-actions">
                <button
                  type="button"
                  className="button secondary"
                  disabled={busy !== ""}
                  onClick={() => void detect()}
                >
                  {busy === "detect" ? t("检测中") : t("重新检测")}
                </button>
              </span>
            </div>
          ) : !status.supported ? (
            <p className="client-desktop-hint">
              {t("需要 Windows 11 22H2 或更新版本")}
            </p>
          ) : status.configured ? (
            <div className="client-desktop-line">
              <span className="client-desktop-dot" aria-hidden="true" />
              <strong>{status.desktopName}</strong>
              <span>{t("已就绪")}</span>
              <span className="client-desktop-actions">
                <button
                  type="button"
                  className="button secondary"
                  disabled={busy !== ""}
                  onClick={() => void show()}
                >
                  {busy === "show" ? t("显示中") : t("显示客户端")}
                </button>
                <button
                  type="button"
                  className="button secondary"
                  disabled={busy !== ""}
                  onClick={() => void detect()}
                >
                  {busy === "detect" ? t("检测中") : t("重新检测")}
                </button>
              </span>
            </div>
          ) : (
            <>
              <div className="client-desktop-line">
                <span className="client-desktop-dot muted" aria-hidden="true" />
                <span>{t("未找到 DiscoAS 桌面")}</span>
                <span className="client-desktop-actions">
                  <button
                    type="button"
                    className="button primary"
                    disabled={busy !== ""}
                    onClick={() => void create()}
                  >
                    {busy === "create" ? t("创建中") : t("创建后台桌面")}
                  </button>
                  <button
                    type="button"
                    className="button secondary"
                    disabled={busy !== ""}
                    onClick={() => void detect()}
                  >
                    {busy === "detect" ? t("检测中") : t("重新检测")}
                  </button>
                </span>
              </div>
              <p className="client-desktop-hint">
                {t("创建后按 Win+Tab，右键新桌面 → 重命名 → 输入 DiscoAS，再点重新检测。")}
              </p>
            </>
          )}
          {message && <p className="client-desktop-hint">{message}</p>}
          {error && <p className="client-desktop-error">{error}</p>}
        </div>
      )}
    </>
  );
}
