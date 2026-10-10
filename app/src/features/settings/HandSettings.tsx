import { useEffect, useRef, useState } from "react";
import type { GuiSettings, HandSettings as HandPreferences, Preferences } from "../../types";
import { SettingRow, NumberField, ScaleField, Toggle } from "../../components/SettingsControls";
import { Select } from "../../components/Select";
import { ShortcutRecorder } from "../../ShortcutRecorder";
import { call, onDesktopEvent } from "../../services/desktop";
import { t, errorText } from "../../i18n";

export function HandSettings({ settings, gui, onChange }: { settings: Preferences; gui: GuiSettings; onChange: (value: HandPreferences) => void }) {
  const value = settings.hand;
  const set = <K extends keyof HandPreferences>(key: K, next: HandPreferences[K]) => onChange({ ...value, [key]: next });
  const [preview, setPreview] = useState(false);
  const [error, setError] = useState("");
  const active = useRef(false);
  const mounted = useRef(false);
  const draft = useRef({ settings, gui }); draft.current = { settings, gui };
  const chain = useRef<Promise<unknown>>(Promise.resolve());
  const updateQueued = useRef(false);
  const enqueue = (command: "start_hand_preview" | "update_hand_preview" | "end_hand_preview") => {
    if (command === "update_hand_preview" && updateQueued.current) return;
    if (command === "update_hand_preview") updateQueued.current = true;
    chain.current = chain.current.catch(() => {}).then(() => {
      if (command === "update_hand_preview") updateQueued.current = false;
      return call(command, command === "end_hand_preview" ? undefined : draft.current);
    });
    void chain.current.catch((e) => { if (mounted.current) { setError(errorText(e)); if (command === "start_hand_preview") { active.current = false; setPreview(false); } } });
  };
  useEffect(() => {
    mounted.current = true; let stop: (() => void) | undefined;
    void onDesktopEvent("hand-preview-closed", () => { active.current = false; setPreview(false); }).then((cleanup) => { if (mounted.current) stop = cleanup; else cleanup(); });
    return () => { mounted.current = false; active.current = false; stop?.(); enqueue("end_hand_preview"); };
  }, []);
  useEffect(() => {
    if (!preview) return;
    const timer = setTimeout(() => { if (active.current) enqueue("update_hand_preview"); }, 100);
    return () => clearTimeout(timer);
  }, [settings, gui, preview]);
  return <section className="settings-section">
    <h2>{t("手牌模式")}</h2>
    <SettingRow title={t("手牌模式")}><Toggle label={t("手牌模式")} value={value.enabled} onChange={(v) => { if (!v && preview) { active.current = false; setPreview(false); enqueue("end_hand_preview"); } set("enabled", v); }} /></SettingRow>
    {value.enabled && <>
      <SettingRow title={t("进入手牌时揭晓神秘歌曲")}><Toggle label={t("进入手牌时揭晓神秘歌曲")} value={value.reveal_mystery} onChange={(v) => set("reveal_mystery", v)} /></SettingRow>
      <SettingRow title={t("收牌后保留发现界面")}><Toggle label={t("收牌后保留发现界面")} value={value.keep_discovery_open} onChange={(v) => set("keep_discovery_open", v)} /></SettingRow>
      <SettingRow title={t("手牌常驻")}><Toggle label={t("手牌常驻")} value={value.resident} onChange={(v) => set("resident", v)} /></SettingRow>
      <SettingRow title={t("手牌快捷键")}><ShortcutRecorder label={t("手牌快捷键")} value={value.shortcut} onChange={(v) => set("shortcut", v)} /></SettingRow>
      <SettingRow title={t("手牌上限")}><NumberField label={t("手牌上限")} value={value.capacity} min={1} max={100} suffix={t("张")} onChange={(v) => set("capacity", v)} /></SettingRow>
      <SettingRow title={t("手牌位置")}><Select label={t("手牌位置")} value={value.side} options={[
        { value: "bottom", label: t("屏幕底部") }, { value: "left", label: t("屏幕左侧") }, { value: "right", label: t("屏幕右侧") },
      ]} onChange={(v) => set("side", v as HandPreferences["side"])} /></SettingRow>
      <SettingRow title={t("手牌大小")}><ScaleField label={t("手牌大小")} value={value.scale} onChange={(v) => set("scale", v)} /></SettingRow>
      <SettingRow title={t("距屏幕边缘")}><NumberField decimal label={t("距屏幕边缘")} value={value.edge_distance} min={-500} max={500} suffix="px" onChange={(v) => set("edge_distance", v)} /></SettingRow>
      <SettingRow title={t("沿边位置")}><NumberField decimal label={t("沿边位置")} value={value.position} min={0} max={100} suffix="%" onChange={(v) => set("position", v)} /></SettingRow>
      <SettingRow title={t("卡片重叠")}><NumberField decimal label={t("卡片重叠")} value={value.overlap} min={0} max={90} suffix="%" onChange={(v) => set("overlap", v)} /></SettingRow>
      <SettingRow title={t("倾斜角度")}><NumberField decimal label={t("倾斜角度")} value={value.tilt} min={0} max={45} suffix="°" onChange={(v) => set("tilt", v)} /></SettingRow>
      <SettingRow title={t("预览手牌布局")}><Toggle label={t("预览手牌布局")} value={preview} onChange={(v) => { active.current = v; setPreview(v); setError(""); enqueue(v ? "start_hand_preview" : "end_hand_preview"); }} /></SettingRow>
      <div className="appearance-actions"><button type="button" className="button secondary" onClick={() => { void call("toggle_hand_expanded").catch((e) => setError(errorText(e))); }}>{t("展开／收纳手牌")}</button></div>
    </>}
    {error && <p className="inline-error padded-error" role="alert">{error}</p>}
  </section>;
}
