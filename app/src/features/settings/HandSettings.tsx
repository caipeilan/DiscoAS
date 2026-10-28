import { useState } from "react";
import type { HandSettings as HandPreferences, HandDiscoverySettings, Preferences } from "../../types";
import { SettingRow, NumberField, ScaleField, Toggle } from "../../components/SettingsControls";
import { Select } from "../../components/Select";
import { Reveal } from "../../components/Motion";
import { call } from "../../services/desktop";
import { t, errorText } from "../../i18n";
import { AUTO_PLAY_AVAILABLE, supportsAutoPlay } from "./autoHandDetection";

export function HandSettings({ settings, onChange, onDiscoveryChange }: { settings: Preferences; onChange: (value: HandPreferences) => void; onDiscoveryChange: (value: HandDiscoverySettings) => void }) {
  const value = settings.hand;
  const platform = settings.playlist_albums.find(source => source.enabled)?.name;
  const autoPlaySupported = supportsAutoPlay(platform);
  // Detection is chosen by the desktop observer from platform capability. The
  // old saved field remains readable but is intentionally not edited here.
  const discovery = settings.hand_discovery;
  const setDiscovery = <K extends keyof HandDiscoverySettings>(key: K, next: HandDiscoverySettings[K]) => onDiscoveryChange({ ...discovery, [key]: next });
  const set = <K extends keyof HandPreferences>(key: K, next: HandPreferences[K]) => onChange({ ...value, [key]: next });
  const [error, setError] = useState("");
  return <><section className="settings-section">
    <h2>{t("手牌模式")}</h2>
    <SettingRow title={t("手牌模式")}><Toggle label={t("手牌模式")} value={value.enabled} onChange={(v) => set("enabled", v)} /></SettingRow>
    <Reveal open={value.enabled || discovery.enabled}>
      <Reveal open={value.enabled}>
      <SettingRow title={t("进入手牌时揭晓神秘歌曲")}><Toggle label={t("进入手牌时揭晓神秘歌曲")} value={value.reveal_mystery} onChange={(v) => set("reveal_mystery", v)} /></SettingRow>
      <SettingRow title={t("手牌常驻")}><Toggle label={t("手牌常驻")} value={value.resident} onChange={(v) => set("resident", v)} /></SettingRow>
      <SettingRow title={t("创建手牌")} description={t("开启后，通过输入链接或ID，创建对应曲目的手牌")}><Toggle label={t("创建手牌")} value={value.create_cards} onChange={v => set("create_cards", v)} /></SettingRow>
      {AUTO_PLAY_AVAILABLE && <>
      <SettingRow title={t("自动打出手牌（实验）")}>
        {autoPlaySupported
          ? <Toggle label={t("自动打出手牌（实验）")} value={value.auto_play} onChange={(v) => set("auto_play", v)} />
          : <span>{t("Spotify 暂不支持自动打出手牌")}</span>}
      </SettingRow>
      {autoPlaySupported && value.auto_play && <>
        <SettingRow title={t("下一首使用")}><Select label={t("下一首使用")} value={value.auto_play_position} options={[
          { value: "first", label: t("手牌的第一张") }, { value: "last", label: t("手牌的最后一张") },
        ]} onChange={(v) => set("auto_play_position", v as HandPreferences["auto_play_position"])} /></SettingRow>
      </>}
      </>}
      </Reveal>
      <Reveal open={value.enabled}>
      <SettingRow title={t("手牌上限")}><NumberField label={t("手牌上限")} value={value.capacity} min={1} max={100} suffix={t("张")} onChange={(v) => set("capacity", v)} /></SettingRow>
      <SettingRow title={t("手牌位置")}><Select label={t("手牌位置")} value={value.side} options={[
        { value: "bottom", label: t("屏幕底部") }, { value: "left", label: t("屏幕左侧") }, { value: "right", label: t("屏幕右侧") },
      ]} onChange={(v) => set("side", v as HandPreferences["side"])} /></SettingRow>
      <SettingRow title={t("手牌大小")}><ScaleField label={t("手牌大小")} value={value.scale} onChange={(v) => set("scale", v)} /></SettingRow>
      <SettingRow title={t("距屏幕边缘")}><NumberField decimal label={t("距屏幕边缘")} value={value.edge_distance} min={-500} max={500} suffix="px" onChange={(v) => set("edge_distance", v)} /></SettingRow>
      <SettingRow title={t("沿边位置")}><NumberField decimal label={t("沿边位置")} value={value.position} min={0} max={100} suffix="%" onChange={(v) => set("position", v)} /></SettingRow>
      <SettingRow title={t("卡片重叠")}><NumberField decimal label={t("卡片重叠")} value={value.overlap} min={0} max={90} suffix="%" onChange={(v) => set("overlap", v)} /></SettingRow>
      <SettingRow title={t("倾斜角度")}><NumberField decimal label={t("倾斜角度")} value={value.tilt} min={0} max={45} suffix="°" onChange={(v) => set("tilt", v)} /></SettingRow>
      <div className="appearance-actions"><button type="button" className="button secondary" onClick={() => { void call("toggle_hand_expanded").catch((e) => setError(errorText(e))); }}>{t("展开／收纳手牌")}</button></div>
      </Reveal>
    </Reveal>
    {error && <p className="inline-error padded-error" role="alert">{error}</p>}
  </section><section className="settings-section">
    <h2>{t("迷你发现")}</h2>
    <SettingRow title={t("迷你发现")}><Toggle label={t("迷你发现")} value={discovery.enabled} onChange={v => setDiscovery("enabled", v)} /></SettingRow>
    <Reveal open={discovery.enabled}>
      {value.enabled && <SettingRow title={t("与手牌合并布局")}><Toggle label={t("与手牌合并布局")} value={discovery.merged} onChange={v => setDiscovery("merged", v)} /></SettingRow>}
      <SettingRow title={t("Liquid 效果")} description={t("实时模糊更接近液态玻璃，但会增加游戏运行时的 GPU 合成开销；静态磨砂更省性能。")}>
        <Select label={t("Liquid 效果")} value={discovery.liquid_mode ?? "static"} options={[
          { value: "static", label: t("静态磨砂") }, { value: "blur", label: t("实时模糊") },
        ]} onChange={v => setDiscovery("liquid_mode", v as HandDiscoverySettings["liquid_mode"])} />
      </SettingRow>
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
      <Reveal open={!discovery.merged || !value.enabled}>
        <SettingRow title={t("迷你发现位置")}><Select label={t("迷你发现位置")} value={discovery.side} options={[
          { value: "bottom", label: t("屏幕底部") }, { value: "left", label: t("屏幕左侧") }, { value: "right", label: t("屏幕右侧") },
        ]} onChange={v => setDiscovery("side", v as HandDiscoverySettings["side"])} /></SettingRow>
        <SettingRow title={t("迷你发现大小")}><ScaleField label={t("迷你发现大小")} value={discovery.scale} onChange={v => setDiscovery("scale", v)} /></SettingRow>
        <SettingRow title={t("距屏幕边缘")}><NumberField decimal label={t("距屏幕边缘")} value={discovery.edge_distance} min={-500} max={500} suffix="px" onChange={v => setDiscovery("edge_distance", v)} /></SettingRow>
        <SettingRow title={t("沿边位置")}><NumberField decimal label={t("沿边位置")} value={discovery.position} min={0} max={100} suffix="%" onChange={v => setDiscovery("position", v)} /></SettingRow>
        <SettingRow title={t("卡片重叠")}><NumberField decimal label={t("卡片重叠")} value={discovery.overlap} min={0} max={90} suffix="%" onChange={v => setDiscovery("overlap", v)} /></SettingRow>
        <SettingRow title={t("倾斜角度")}><NumberField decimal label={t("倾斜角度")} value={discovery.tilt} min={0} max={45} suffix="°" onChange={v => setDiscovery("tilt", v)} /></SettingRow>
      </Reveal>
    </Reveal>
  </section></>;
}
