import { useEffect, useState } from "react";
import type { Dispatch, SetStateAction } from "react";
import type { DesktopPreferences } from "../../types";
import { SettingRow, NumberField } from "../../components/SettingsControls";
import { InlineFeedback } from "../../components/InlineFeedback";
import { Icon } from "../../Icon";
import { t, errorText } from "../../i18n";
import { desktop, getCacheStatus, chooseCacheDirectory, clearCache } from "../../services/desktop";

export function CacheSettings({ value, saved, onChange }: {
  value: DesktopPreferences; saved: DesktopPreferences; onChange: Dispatch<SetStateAction<DesktopPreferences>>;
}) {
  const [directory, setDirectory] = useState("");
  const [working, setWorking] = useState(false);
  const [error, setError] = useState("");
  const [cleaned, setCleaned] = useState(false);
  useEffect(() => {
    let active = true;
    if (desktop) getCacheStatus().then(status => { if (active) setDirectory(status.directory); }).catch(failure => { if (active) setError(errorText(failure)); });
    return () => { active = false; };
  }, [saved.cacheDirectory]);
  const choose = async () => {
    setError("");
    try { const next = await chooseCacheDirectory(); if (next) onChange(current => ({ ...current, cacheDirectory: next })); }
    catch (failure) { setError(errorText(failure)); }
  };
  const clear = async () => {
    if (working) return;
    setCleaned(false); setError("");
    if (value.cacheDirectory !== saved.cacheDirectory) { setError(t("请先保存缓存地址")); return; }
    setWorking(true);
    try { await clearCache(); setCleaned(true); }
    catch (failure) { setError(errorText(failure)); }
    finally { setWorking(false); }
  };
  return <section className="settings-section">
    <h2>{t("缓存")}</h2>
    <SettingRow title={t("磁盘缓存上限")}>
      <NumberField label={t("磁盘缓存上限")} value={value.cacheLimitMb} min={0} max={1048576} suffix="MB" onChange={cacheLimitMb => onChange(current => ({ ...current, cacheLimitMb }))} />
    </SettingRow>
    <SettingRow title={t("内存缓存上限")}>
      <NumberField label={t("内存缓存上限")} value={value.memoryCacheLimitMb} min={8} max={1024} suffix="MB" onChange={memoryCacheLimitMb => onChange(current => ({ ...current, memoryCacheLimitMb }))} />
    </SettingRow>
    <SettingRow title={t("缓存地址")} className="cache-directory-row">
      <div className="cache-directory-control">
        <input className="input" aria-label={t("缓存地址")} value={value.cacheDirectory || directory} readOnly />
        <button type="button" className="button secondary" onClick={() => { void choose(); }}><Icon name="folder" />{t("选择")}</button>
      </div>
    </SettingRow>
    <SettingRow title={t("清理缓存")}>
      <button type="button" className="button secondary" disabled={working} onClick={() => { void clear(); }}>{working ? t("清理中…") : t("清理")}</button>
    </SettingRow>
    <InlineFeedback message={error} dismiss={() => setError("")} />
    {cleaned && <span role="status" className="settings-cache-status">{t("缓存已清理")}</span>}
  </section>;
}
