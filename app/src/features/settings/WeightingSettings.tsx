import type { DiscoveryWeighting, DiscoveryWeightingMode, RankedWeighting } from "../../types";
import { useEffect, useRef, useState } from "react";
import { NumberField, SettingRow, Toggle } from "../../components/SettingsControls";
import { Select } from "../../components/Select";
import { Icon } from "../../Icon";
import { AnimatedDetails, Reveal } from "../../components/Motion";
import { t } from "../../i18n";
import { calculateDiscoveryWeighting, parseSongBasis, recommendedSoftBand } from "./weightingCalculator";
import "./weighting.css";

export type WeightingPatch = {
  weighting_mode?: DiscoveryWeightingMode;
  discovery_weighting?: DiscoveryWeighting;
  ranked_weighting?: RankedWeighting;
};

export function WeightingSettings({ mode, traditional, ranked, onChange, songCount, sourceIdentity, drawCount }: {
  mode: DiscoveryWeightingMode;
  traditional: DiscoveryWeighting;
  ranked: RankedWeighting;
  onChange: (patch: WeightingPatch) => void;
  songCount: number;
  sourceIdentity: string;
  drawCount: number;
}) {
  const recommended = recommendedSoftBand(songCount);
  // Tracks the last automatically applied soft band. A later source change only
  // updates a value that was never customized by the user.
  const lastAutoBand = useRef(20);
  const previousSource = useRef({ songCount, sourceIdentity });
  const latest = useRef({ mode, ranked, onChange, recommended });
  latest.current = { mode, ranked, onChange, recommended };
  const changeMode = (next: DiscoveryWeightingMode) => {
    if (next === "traditional") {
      onChange({ weighting_mode: "traditional", discovery_weighting: { ...traditional, enabled: true } });
      return;
    }
    if (next === "ranked") {
      let band = ranked.soft_band;
      if (band === lastAutoBand.current) {
        band = recommended;
        lastAutoBand.current = recommended;
      }
      onChange({
        weighting_mode: "ranked",
        discovery_weighting: { ...traditional, enabled: false },
        ranked_weighting: { ...ranked, soft_band: band },
      });
      return;
    }
    onChange({ weighting_mode: next, discovery_weighting: { ...traditional, enabled: false } });
  };
  useEffect(() => {
    const previous = previousSource.current;
    previousSource.current = { songCount, sourceIdentity };
    if (previous.songCount === songCount && previous.sourceIdentity === sourceIdentity) return;
    const current = latest.current;
    if (
      current.mode === "ranked" &&
      current.ranked.soft_band === lastAutoBand.current &&
      current.ranked.soft_band !== current.recommended
    ) {
      lastAutoBand.current = current.recommended;
      current.onChange({ ranked_weighting: { ...current.ranked, soft_band: current.recommended } });
    }
  }, [songCount, sourceIdentity]);
  return <section className="settings-section weighting-settings">
    <h2>{t("加权发现")}</h2>
    <SettingRow title={t("加权模式")}>
      <Select label={t("加权模式")} value={mode} onChange={(value) => changeMode(value as DiscoveryWeightingMode)}
        options={[
          { value: "none", label: t("不加权") },
          { value: "traditional", label: t("传统加权") },
          { value: "ranked", label: t("排序相对加权（适合1k+曲目歌单）") },
          { value: "rotation", label: t("轮换均衡加权（适合<1k曲目歌单）") },
        ]} />
    </SettingRow>
    {mode === "rotation" && <p className="setting-explanation">{t("最近出现的曲目暂时降低权重，随后逐渐恢复；刚选中的曲目恢复得更慢。恢复速度随歌单曲数和每批发现数量自动调整。")}</p>}
    {mode === "traditional" && <TraditionalWeightingSettings value={traditional} onChange={(value) => onChange({ discovery_weighting: value })}
      songCount={songCount} sourceIdentity={sourceIdentity} drawCount={drawCount} />}
    {mode === "ranked" && <RankedWeightingSettings value={ranked} onChange={(value) => onChange({ ranked_weighting: value })}
      recommendedBand={recommended} />}
  </section>;
}

function TraditionalWeightingSettings({ value, onChange, songCount, sourceIdentity, drawCount }: {
  value: DiscoveryWeighting;
  onChange: (value: DiscoveryWeighting) => void;
  songCount: number;
  sourceIdentity: string;
  drawCount: number;
}) {
  const [basis, setBasis] = useState(songCount > 0 ? String(songCount) : "");
  const [calculated, setCalculated] = useState(false);
  useEffect(() => { setBasis(songCount > 0 ? String(songCount) : ""); setCalculated(false); }, [songCount, sourceIdentity]);
  useEffect(() => { setCalculated(false); }, [drawCount]);
  const set = (next: DiscoveryWeighting) => { setCalculated(false); onChange(next); };
  const count = parseSongBasis(basis);
  const result = count === null ? null : calculateDiscoveryWeighting(count, drawCount, true);
  return <>
    <SettingRow title={t("歌曲基数")}>
      <div className="weighting-calculation">
        <input type="text" inputMode="numeric" aria-label={t("歌曲基数")}
          aria-invalid={basis.trim() !== "" && count === null || undefined}
          value={basis} onChange={(event) => { setBasis(event.target.value); setCalculated(false); }} />
        <button type="button" className="button secondary" disabled={result === null}
          onClick={() => { if (result) { onChange(result); setCalculated(true); } }}
          aria-label={t("计算加权参数")}>
          {calculated && <Icon name="check" size={14} />}
          {t("计算")}
        </button>
      </div>
    </SettingRow>
    <AnimatedDetails className="weighting-parameters" summary={t("手动调整")}>
      <SettingRow title={t("原始权重")}><NumberField decimal label={t("原始权重")} value={value.base_weight} min={1} max={10000} suffix=""
        onChange={(base_weight) => set({ ...value, base_weight, max_weight: Math.max(base_weight, value.max_weight) })} /></SettingRow>
      <SettingRow title={t("入选后减权")}><NumberField decimal label={t("入选后减权")} value={value.discovered_penalty} min={0} max={10000} suffix=""
        onChange={(discovered_penalty) => set({ ...value, discovered_penalty })} /></SettingRow>
      <SettingRow title={t("选择后减权")}><NumberField decimal label={t("选择后减权")} value={value.selected_penalty} min={0} max={10000} suffix=""
        onChange={(selected_penalty) => set({ ...value, selected_penalty })} /></SettingRow>
      <SettingRow title={t("恢复原权重所需发现次数")}><NumberField label={t("恢复原权重所需发现次数")} value={value.recovery_batches} min={1} max={10000} suffix={t("次")}
        onChange={(recovery_batches) => set({ ...value, recovery_batches, boost_after_batches: Math.max(recovery_batches, value.boost_after_batches) })} /></SettingRow>
      <SettingRow title={t("开始加权所需发现次数")}><NumberField label={t("开始加权所需发现次数")} value={value.boost_after_batches} min={value.recovery_batches} max={10000} suffix={t("次")}
        onChange={(boost_after_batches) => set({ ...value, boost_after_batches })} /></SettingRow>
      <SettingRow title={t("每次发现加权")}><NumberField decimal label={t("每次发现加权")} value={value.boost_per_batch} min={0} max={10000} suffix=""
        onChange={(boost_per_batch) => set({ ...value, boost_per_batch })} /></SettingRow>
      <SettingRow title={t("最高权重")}><NumberField decimal label={t("最高权重")} value={value.max_weight} min={value.base_weight} max={100000} suffix=""
        onChange={(max_weight) => set({ ...value, max_weight })} /></SettingRow>
    </AnimatedDetails>
  </>;
}

function RankedWeightingSettings({ value, onChange, recommendedBand }: {
  value: RankedWeighting;
  onChange: (value: RankedWeighting) => void;
  recommendedBand: number;
}) {
  const set = (patch: Partial<RankedWeighting>) => onChange({ ...value, ...patch });
  return <>
    <SettingRow title={t("过渡窗口")} description={t("排除窗口之外的歌曲按此数量从低权重平滑恢复到正常概率；0 表示硬边界。排除模式为「不排除」时从最新展示开始软起排。默认是启用歌单曲数的 10%，没有歌单时为 20。")}
      hint={t("当前歌单推荐过渡窗口：{p0} 首", { p0: recommendedBand })}>
      <NumberField label={t("过渡窗口")} value={value.soft_band} min={0} max={10000} onChange={(soft_band) => set({ soft_band })} />
    </SettingRow>
    <SettingRow title={t("时间冷却")} description={t("在排序过渡之外，再按发现轮次压低最近展示的歌曲；不作用于从未出现的歌曲。")}>
      <Toggle label={t("时间冷却")} value={value.time_cooling} onChange={(time_cooling) => set({ time_cooling })} />
    </SettingRow>
    <Reveal open={value.time_cooling}>
      <div className="settings-subrows">
        <SettingRow title={t("冷却强度")} description={t("0 到 0.95，越接近 0.95 冷却越强。")}>
          <NumberField decimal label={t("冷却强度")} value={value.time_cooling_strength} min={0} max={0.95} suffix=""
            onChange={(time_cooling_strength) => set({ time_cooling_strength })} />
        </SettingRow>
        <SettingRow title={t("冷却刷新次数")} description={t("发现轮次尺度，越大冷却恢复越慢。")}>
          <NumberField label={t("冷却刷新次数")} value={value.time_cooling_batches} min={1} max={10000} suffix={t("次")}
            onChange={(time_cooling_batches) => set({ time_cooling_batches })} />
        </SettingRow>
      </div>
    </Reveal>
    <SettingRow title={t("老歌复现")} description={t("超过等待次数后逐步提高很久没展示过的歌曲权重；默认关闭，开启后老歌概率会高于从未出现的歌曲。")}>
      <Toggle label={t("老歌复现")} value={value.revival} onChange={(revival) => set({ revival })} />
    </SettingRow>
    <Reveal open={value.revival}>
      <div className="settings-subrows">
        <SettingRow title={t("复现增益")} description={t("0 到 2；0.5 表示最高约 1.5 倍权重。")}>
          <NumberField decimal label={t("复现增益")} value={value.revival_gain} min={0} max={2} suffix=""
            onChange={(revival_gain) => set({ revival_gain })} />
        </SettingRow>
        <SettingRow title={t("等待次数")} description={t("距离上次展示达到该轮次后才开始复现。")}>
          <NumberField label={t("等待次数")} value={value.revival_wait} min={0} max={10000} suffix={t("次")}
            onChange={(revival_wait) => set({ revival_wait })} />
        </SettingRow>
        <SettingRow title={t("爬升次数")} description={t("复现权重从基线爬到满增益所需的轮次。")}>
          <NumberField label={t("爬升次数")} value={value.revival_span} min={1} max={10000} suffix={t("次")}
            onChange={(revival_span) => set({ revival_span })} />
        </SettingRow>
      </div>
    </Reveal>
  </>;
}
