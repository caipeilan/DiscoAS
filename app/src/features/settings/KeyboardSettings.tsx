import { useEffect, useRef, useState } from "react";
import type { DiscoveryKeybindings } from "../../types";
import { SettingRow } from "../../components/SettingsControls";
import { call, desktop } from "../../services/desktop";
import { t } from "../../i18n";
import { defaultDiscoveryKeybindings, keybindingFromEvent, validateDiscoveryKeybindings } from "./discoveryKeybindings";

type Action = keyof DiscoveryKeybindings;

export function KeyboardSettings({ value, onChange, disabled = false, globalShortcut = "" }: {
  value: DiscoveryKeybindings;
  onChange: (value: DiscoveryKeybindings) => void;
  disabled?: boolean;
  globalShortcut?: string;
}) {
  const [recording, setRecording] = useState<Action | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const container = useRef<HTMLElement>(null);
  const mounted = useRef(true);
  const buttons = useRef<Partial<Record<Action, HTMLButtonElement | null>>>({});
  const labels = {
    up: t("向上"), left: t("向左"), down: t("向下"), right: t("向右"), select: t("确认选歌"), replace: t("替换歌曲"),
  };
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  useEffect(() => { if (disabled && recording) setRecording(null); }, [disabled, recording]);
  useEffect(() => {
    if (!recording) return;
    const stop = () => setRecording(null);
    const key = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopImmediatePropagation();
      if (event.key === "Escape") { stop(); buttons.current[recording]?.focus(); return; }
      if (event.repeat || event.isComposing || ["Control", "Alt", "Shift", "Meta", "AltGraph", "Process"].includes(event.key)) return;
      const binding = keybindingFromEvent(event);
      if (!binding) { setError(t("错误：选歌按键无效")); return; }
      const next = { ...value, [recording]: binding };
      const invalid = validateDiscoveryKeybindings(next, globalShortcut);
      if (invalid) {
        setError(invalid === "错误：选歌按键不能重复" ? t("错误：选歌按键不能重复")
          : invalid === "错误：选歌按键不能与全局快捷键重复" ? t("错误：选歌按键不能与全局快捷键重复") : t("错误：选歌按键无效"));
        return;
      }
      onChange(next);
      setError("");
      stop();
      buttons.current[recording]?.focus();
    };
    const outside = (event: PointerEvent) => {
      if (!container.current?.contains(event.target as Node)) stop();
    };
    const timeout = window.setTimeout(stop, 40000);
    window.addEventListener("keydown", key, true);
    window.addEventListener("blur", stop);
    window.addEventListener("pointerdown", outside, true);
    return () => {
      window.clearTimeout(timeout);
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("blur", stop);
      window.removeEventListener("pointerdown", outside, true);
    };
  }, [recording, value, onChange, globalShortcut]);
  // A configured local combination may match the global hotkey; pause that registration while recording.
  useEffect(() => {
    if (!recording) return;
    return () => {
      if (desktop) call("set_shortcut_recording", { recording: false }).catch(() => {
        if (mounted.current) setError(t("快捷键恢复失败，请重新保存设置。"));
      });
    };
  }, [recording]);
  const start = async (action: Action) => {
    if (pending || disabled) return;
    if (recording) { setRecording(null); return; }
    setPending(true);
    setError("");
    try {
      if (desktop) await call("set_shortcut_recording", { recording: true });
      if (mounted.current) setRecording(action);
      else if (desktop) await call("set_shortcut_recording", { recording: false });
    } catch {
      setError(t("无法开始快捷键录制，请稍后重试。"));
    } finally { if (mounted.current) setPending(false); }
  };
  return <section className="settings-section keyboard-settings" ref={container}>
    <h2>{t("键盘选歌")}</h2>
    {(["up", "left", "down", "right", "select", "replace"] as const).map((action) => <SettingRow key={action} title={labels[action]}>
      <button type="button" className={`button secondary keybinding-button${recording === action ? " recording" : ""}`}
        ref={(element) => { buttons.current[action] = element; }} disabled={disabled || pending || (!!recording && recording !== action)}
        aria-label={t("修改 {p0} 按键", { p0: labels[action] })} aria-pressed={recording === action}
        onClick={() => start(action)}>
        {recording === action ? t("按下按键，Esc 取消") : value[action]}
      </button>
    </SettingRow>)}
    <div className="appearance-actions">
      {error && <span className="inline-error" role="alert">{error}</span>}
      <button type="button" className="button secondary" disabled={disabled || pending || !!recording}
        onClick={() => { setError(""); onChange({ ...defaultDiscoveryKeybindings }); }}>{t("恢复默认按键")}</button>
    </div>
  </section>;
}
