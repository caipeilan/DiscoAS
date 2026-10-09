import { useEffect, useState } from "react";
import type { DiscoveryWeighting } from "../../types";
import { NumberField, SettingRow, Toggle } from "../../components/SettingsControls";
import { Icon } from "../../Icon";
import { t } from "../../i18n";
import { calculateDiscoveryWeighting, parseSongBasis } from "./weightingCalculator";
import "./weighting.css";

export function WeightingSettings({ value, onChange, songCount, sourceIdentity, drawCount }: {
  value: DiscoveryWeighting; onChange: (value: DiscoveryWeighting) => void;
  songCount: number; sourceIdentity: string; drawCount: number;
}) {
  const [basis, setBasis] = useState(songCount > 0 ? String(songCount) : "");
  const [calculated, setCalculated] = useState(false);
  useEffect(() => { setBasis(songCount > 0 ? String(songCount) : ""); setCalculated(false); }, [songCount, sourceIdentity]);
  useEffect(() => { setCalculated(false); }, [drawCount]);
  const set = (next: DiscoveryWeighting) => { setCalculated(false); onChange(next); };
  const count = parseSongBasis(basis);
  const result = count === null ? null : calculateDiscoveryWeighting(count, drawCount, value.enabled);
  return <section className="settings-section weighting-settings">
    <h2>{t("加权发现")}</h2>
    <SettingRow title={t("加权发现")}>
      <Toggle label={t("加权发现")} value={value.enabled} onChange={(enabled) => set({ ...value, enabled })} />
    </SettingRow>
    {value.enabled && <>
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
      <details className="weighting-parameters">
        <summary>{t("手动调整")}</summary>
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
      </details>
    </>}
  </section>;
}
