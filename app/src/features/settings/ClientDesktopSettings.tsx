import { useState } from "react";
import { SettingRow } from "../../components/SettingsControls";
import { Select } from "../../components/Select";
import { errorText, t } from "../../i18n";
import { showLocalClient } from "../../services/desktop";
import type { DesktopPreferences } from "../../types";

type ClientWindowMode = DesktopPreferences["client_window_mode"];
type BusyAction = "" | "show";

export function ClientDesktopSettings({
  mode,
  onChange,
}: {
  mode: ClientWindowMode;
  onChange: (mode: ClientWindowMode) => void;
}) {
  const [busy, setBusy] = useState<BusyAction>("");
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const options: Array<{ value: ClientWindowMode; label: string }> = [
    { value: "off", label: t("隐藏关闭") },
    { value: "temporary", label: t("临时隐藏（不推荐）") },
    { value: "persistent", label: t("长期隐藏（推荐）") },
  ];

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
      <div className="client-desktop-status" role="status" aria-live="polite">
        <div className="client-desktop-line">
          {message && <span className="client-desktop-message">{message}</span>}
          <span className="client-desktop-actions">
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
        {error && <p className="client-desktop-error">{error}</p>}
      </div>
    </>
  );
}
