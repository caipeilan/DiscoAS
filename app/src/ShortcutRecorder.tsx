import { useEffect, useRef, useState } from "react";
import { shortcutFromEvent } from "./shortcut";
import { t } from "./i18n";
import { call, desktop } from "./services/desktop";
export function ShortcutRecorder({
  value,
  onChange,
}: {
  value: string;
  onChange: (value: string) => void;
}) {
  const [recording, setRecording] = useState(false);
  const [error, setError] = useState("");
  const [pending, setPending] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const button = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!recording) return;
    const key = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopImmediatePropagation();
      if (event.key === "Escape") {
        setRecording(false);
        return;
      }
      const shortcut = shortcutFromEvent(event);
      if (shortcut) {
        onChange(shortcut);
        setRecording(false);
        button.current?.focus();
      }
    };
    const blur = () => setRecording(false);
    const outside = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setRecording(false);
    };
    const timeout = window.setTimeout(blur, 40000);
    window.addEventListener("keydown", key, true);
    window.addEventListener("blur", blur);
    window.addEventListener("pointerdown", outside, true);
    return () => {
      window.clearTimeout(timeout);
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("blur", blur);
      window.removeEventListener("pointerdown", outside, true);
    };
  }, [recording, onChange]);
  // Keep the native hotkey unregistered throughout recording; listener rerenders must not restore it.
  useEffect(() => {
    if (!recording) return;
    return () => {
      if (desktop)
        call("set_shortcut_recording", { recording: false }).catch(() =>
          setError(t("快捷键恢复失败，请重新保存设置。")),
        );
    };
  }, [recording]);
  return (
    <div className="shortcut-recorder" ref={root}>
      <input
        className="shortcut-input"
        aria-label={t("全局快捷键")}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder="Alt+D"
        readOnly={recording}
      />
      <button
        type="button"
        ref={button}
        className="button secondary"
        disabled={pending}
        aria-pressed={recording}
        onClick={async () => {
          if (recording) {
            setRecording(false);
            return;
          }
          setPending(true);
          setError("");
          try {
            if (desktop)
              await call("set_shortcut_recording", { recording: true });
            if (mounted.current) setRecording(true);
            else if (desktop)
              await call("set_shortcut_recording", { recording: false });
          } catch {
            setError(t("无法开始快捷键录制，请稍后重试。"));
          } finally {
            setPending(false);
          }
        }}
      >
        {t(recording ? "按下组合键，Esc 取消" : "录制快捷键")}
      </button>
      {recording && (
        <span className="muted" role="status">
          {t("请包含 Ctrl、Alt、Shift 或 Windows 键；Esc 保留原值。")}
        </span>
      )}
      {error && (
        <span className="inline-error" role="alert">
          {error}
        </span>
      )}
    </div>
  );
}
