import { useEffect, useRef, useState } from "react";
import type { GuiSettings, HandSettings as HandPreferences, HandDiscoverySettings, Preferences } from "../../types";
import { defaultHandSettings } from "../../types";
import { SettingRow, NumberField, ScaleField, Toggle, HelpHint } from "../../components/SettingsControls";
import { Select } from "../../components/Select";
import { ShortcutRecorder } from "../../ShortcutRecorder";
import { call, onDesktopEvent } from "../../services/desktop";
import { t, errorText } from "../../i18n";

export function HandSettings({ settings, gui, onChange, onDiscoveryChange }: { settings: Preferences; gui: GuiSettings; onChange: (value: HandPreferences) => void; onDiscoveryChange: (value: HandDiscoverySettings) => void }) {
  const value = settings.hand;
  const discovery = settings.hand_discovery;
  const setDiscovery = <K extends keyof HandDiscoverySettings>(key: K, next: HandDiscoverySettings[K]) => onDiscoveryChange({ ...discovery, [key]: next });
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
  return <><section className="settings-section">
    <h2>{t("手牌模式")}</h2>
    <SettingRow title={t("手牌模式")}><Toggle label={t("手牌模式")} value={value.enabled} onChange={(v) => { if (!v && preview) { active.current = false; setPreview(false); enqueue("end_hand_preview"); } set("enabled", v); }} /></SettingRow>
    {(value.enabled || discovery.enabled) && <>
      {value.enabled && <>
      <SettingRow title={t("进入手牌时揭晓神秘歌曲")}><Toggle label={t("进入手牌时揭晓神秘歌曲")} value={value.reveal_mystery} onChange={(v) => set("reveal_mystery", v)} /></SettingRow>
      <SettingRow title={t("收牌后保留发现界面")}><Toggle label={t("收牌后保留发现界面")} value={value.keep_discovery_open} onChange={(v) => set("keep_discovery_open", v)} /></SettingRow>
      <SettingRow title={t("手牌常驻")}><Toggle label={t("手牌常驻")} value={value.resident} onChange={(v) => set("resident", v)} /></SettingRow>
      <SettingRow title={t("自动打出手牌（实验）")} description={t("优先读取播放进度，在曲终前自动打出；无法读取进度时检测曲目变化。YouTube／Bilibili 需使用浏览器扩展播放。") }>
        <Toggle label={t("自动打出手牌（实验）")} value={value.auto_play} onChange={(v) => set("auto_play", v)} />
      </SettingRow>
      {value.auto_play && <>
        <SettingRow title={t("下一首使用")}><Select label={t("下一首使用")} value={value.auto_play_position} options={[
          { value: "first", label: t("普通手牌的第一张") }, { value: "last", label: t("普通手牌的最后一张") },
        ]} onChange={(v) => set("auto_play_position", v as HandPreferences["auto_play_position"])} /></SettingRow>
        <SettingRow title={t("窗口标题后备检测")} description={t("只要是通过原平台方式（随机切歌、选择特定歌曲）进行切歌，都会被DiscoAS强制切换哦")}>
          <span className="muted">{t("接口不可用时自动启用")}</span>
        </SettingRow>
      </>}
      </>}
      <SettingRow title={t("手牌快捷键")}><ShortcutRecorder label={t("手牌快捷键")} value={value.shortcut} onChange={(v) => set("shortcut", v)} /></SettingRow>
      <details className="hand-shortcut-settings">
        <summary>{t("按位置打出（前九张）")}<span onClick={(event) => event.stopPropagation()}><HelpHint label={t("按位置打出（前九张）")}>{t("合并时按迷你发现在前、手牌在后的当前顺序打出；分开时只控制普通手牌。调整顺序后立即生效。")}</HelpHint></span></summary>
        {defaultHandSettings.play_shortcuts.map((fallback, index) => <SettingRow key={index} title={t("第 {p0} 张", { p0: index + 1 })}>
          <ShortcutRecorder label={t("打出第 {p0} 张手牌", { p0: index + 1 })} value={value.play_shortcuts?.[index] ?? fallback} onChange={(key) => {
            const keys = [...(value.play_shortcuts ?? defaultHandSettings.play_shortcuts)]; keys[index] = key; set("play_shortcuts", keys);
          }} />
        </SettingRow>)}
        <div className="appearance-actions"><button type="button" className="button secondary" onClick={() => set("play_shortcuts", [...defaultHandSettings.play_shortcuts])}>{t("恢复打出快捷键")}</button></div>
      </details>
      {value.enabled && <>
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
    </>}
    {error && <p className="inline-error padded-error" role="alert">{error}</p>}
  </section><section className="settings-section">
    <h2>{t("迷你发现")}</h2>
    <SettingRow title={t("迷你发现")}><Toggle label={t("迷你发现")} value={discovery.enabled} onChange={v => setDiscovery("enabled", v)} /></SettingRow>
    {discovery.enabled && <>
      {value.enabled && <SettingRow title={t("与手牌合并布局")}><Toggle label={t("与手牌合并布局")} value={discovery.merged} onChange={v => setDiscovery("merged", v)} /></SettingRow>}
      <SettingRow title={t("Liquid 强度")} description={t("0% 关闭玻璃效果，100% 为完整效果。") }>
        <div className="scale-field">
          <input type="range" aria-label={t("Liquid 强度滑块")} min={0} max={100} step={1} value={discovery.liquid_strength}
            style={{ "--range-progress": `${discovery.liquid_strength}%` } as React.CSSProperties}
            onChange={e => setDiscovery("liquid_strength", Number(e.target.value))} />
          <NumberField decimal label={t("Liquid 强度")} value={discovery.liquid_strength} min={0} max={100} suffix="%" onChange={v => setDiscovery("liquid_strength", v)} />
        </div>
      </SettingRow>
      <SettingRow title={t("描边粗细")}>
        <NumberField decimal label={t("描边粗细")} value={discovery.text_outline_width} min={.05} max={5} suffix="px" onChange={v => setDiscovery("text_outline_width", v)} />
      </SettingRow>
      {(!discovery.merged || !value.enabled) && <>
        <SettingRow title={t("迷你发现位置")}><Select label={t("迷你发现位置")} value={discovery.side} options={[
          { value: "bottom", label: t("屏幕底部") }, { value: "left", label: t("屏幕左侧") }, { value: "right", label: t("屏幕右侧") },
        ]} onChange={v => setDiscovery("side", v as HandDiscoverySettings["side"])} /></SettingRow>
        <SettingRow title={t("迷你发现大小")}><ScaleField label={t("迷你发现大小")} value={discovery.scale} onChange={v => setDiscovery("scale", v)} /></SettingRow>
        <SettingRow title={t("距屏幕边缘")}><NumberField decimal label={t("距屏幕边缘")} value={discovery.edge_distance} min={-500} max={500} suffix="px" onChange={v => setDiscovery("edge_distance", v)} /></SettingRow>
        <SettingRow title={t("沿边位置")}><NumberField decimal label={t("沿边位置")} value={discovery.position} min={0} max={100} suffix="%" onChange={v => setDiscovery("position", v)} /></SettingRow>
        <SettingRow title={t("卡片重叠")}><NumberField decimal label={t("卡片重叠")} value={discovery.overlap} min={0} max={90} suffix="%" onChange={v => setDiscovery("overlap", v)} /></SettingRow>
        <SettingRow title={t("倾斜角度")}><NumberField decimal label={t("倾斜角度")} value={discovery.tilt} min={0} max={45} suffix="°" onChange={v => setDiscovery("tilt", v)} /></SettingRow>
      </>}
      <SettingRow title={t("预览迷你发现布局")}><Toggle label={t("预览迷你发现布局")} value={preview} onChange={v => { active.current = v; setPreview(v); setError(""); enqueue(v ? "start_hand_preview" : "end_hand_preview"); }} /></SettingRow>
    </>}
  </section></>;
}
