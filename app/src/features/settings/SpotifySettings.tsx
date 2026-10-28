import { useEffect, useRef, useState } from "react";
import type { DesktopPreferences, SpotifyBridgeStatus, SpotifySetupStatus } from "../../types";
import { call, desktop, openExternalUrl } from "../../services/desktop";
import { SettingRow } from "../../components/SettingsControls";
import { Select } from "../../components/Select";
import { Reveal } from "../../components/Motion";
import { t, errorText } from "../../i18n";
import { CopyableInstruction, InstallationGuide } from "./InstallationGuide";
import { ConnectionNotice, ConnectionSummary } from "./ConnectionControls";
import { spotifyNeedsAttention, spotifySetupMessage } from "./connectionModel";

export function SpotifySettings({ settings, setSettings }: {
  settings: DesktopPreferences;
  setSettings: React.Dispatch<React.SetStateAction<DesktopPreferences>>;
}) {
  const [status, setStatus] = useState<SpotifyBridgeStatus | null>(null);
  const [setup, setSetup] = useState<SpotifySetupStatus | null>(null);
  const [operation, setOperation] = useState<"" | "configure" | "check">("");
  const working = operation !== "";
  const [error, setError] = useState("");
  const [statusError, setStatusError] = useState("");
  const [open, setOpen] = useState(false);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const section = useRef<HTMLElement>(null);
  const [inView, setInView] = useState(false);
  const wasConnected = useRef(false);
  const connected = Boolean(status?.connected);
  const busy = working || setup?.phase === "configuring";
  const attention = spotifyNeedsAttention(setup) || Boolean(error || statusError);
  const message = spotifySetupMessage(setup ? { ...setup, message: t(setup.message) } : null, error || statusError);

  useEffect(() => {
    const observer = new IntersectionObserver(([entry]) => setInView(entry.isIntersecting));
    if (section.current) observer.observe(section.current);
    return () => observer.disconnect();
  }, []);
  useEffect(() => {
    if (connected && !wasConnected.current) setOpen(false);
    wasConnected.current = connected;
  }, [connected]);
  useEffect(() => {
    if (!desktop || settings.spotifyPlaybackMode !== "extension" || !inView || working) return;
    let active = true;
    let refreshing = false;
    let timer: ReturnType<typeof setInterval> | undefined;
    const refresh = async () => {
      if (refreshing) return;
      refreshing = true;
      try {
        const [bridge, installed] = await Promise.all([
          call<SpotifyBridgeStatus>("get_spotify_bridge_status"),
          call<SpotifySetupStatus>("get_spotify_setup_status"),
        ]);
        if (active) { setStatus(bridge); setSetup(installed); setStatusError(""); }
      } catch (failure) { if (active) setStatusError(errorText(failure)); }
      finally { refreshing = false; }
    };
    const visibility = () => {
      clearInterval(timer);
      if (document.hidden) return;
      void refresh(); timer = setInterval(refresh, 4000);
    };
    visibility(); document.addEventListener("visibilitychange", visibility);
    return () => { active = false; clearInterval(timer); document.removeEventListener("visibilitychange", visibility); };
  }, [settings.spotifyPlaybackMode, inView, working]);

  const configure = async (repair = false) => {
    if (busy) return;
    setOperation("configure"); setError(""); setStatusError(""); setOpen(true);
    try {
      setSetup(await call<SpotifySetupStatus>("configure_spotify_support", { repair }));
      setStatus(await call<SpotifyBridgeStatus>("get_spotify_bridge_status"));
    } catch (failure) {
      setError(errorText(failure));
      try {
        const next = await call<SpotifySetupStatus>("get_spotify_setup_status");
        setSetup(next);
        if (next.issue) setError("");
      } catch { /* Keep the configuration failure if reading status also failed. */ }
    } finally { setOperation(""); }
  };
  const check = async () => {
    if (busy) return;
    setOperation("check"); setError(""); setStatusError("");
    try {
      setSetup(await call<SpotifySetupStatus>("get_spotify_setup_status"));
      setStatus(await call<SpotifyBridgeStatus>("get_spotify_bridge_status"));
    } catch (failure) { setError(errorText(failure)); }
    finally { setOperation(""); }
  };
  const toggle = () => {
    if (open) { setOpen(false); return; }
    setOpen(true);
    if (!connected && setup?.phase !== "ready" && !setup?.issue) void configure();
  };
  const failure = (value: unknown) => { setError(errorText(value)); setOpen(true); };

  return <section ref={section} className="settings-section">
    <h2>Spotify</h2>
    <SettingRow title={t("播放方式")} description={t("Spotify Free 账户的选歌受客户端限制，扩展无法保证每次都播放指定歌曲。") }>
      <Select label={t("Spotify 切歌方式")} value={settings.spotifyPlaybackMode}
        onChange={(value) => setSettings((current) => ({ ...current, spotifyPlaybackMode: value as DesktopPreferences["spotifyPlaybackMode"] }))}
        options={[
          { value: "extension", label: t("连接桌面播放器") },
          { value: "pause_then_scheme", label: t("先暂停，再打开歌曲") },
          { value: "scheme", label: t("直接打开歌曲") },
        ]} />
    </SettingRow>
    <Reveal open={settings.spotifyPlaybackMode === "extension"}>
      <ConnectionSummary connected={connected} attention={attention} busy={busy} open={open} onToggle={toggle}
        status={busy ? operation === "check" ? t("正在检查连接…") : t("正在配置") : connected ? t("已连接") : attention ? t("需要处理") : setup?.phase === "ready" ? t("等待 Spotify") : t("未连接")} />
      <Reveal open={open}><div className="connection-flow" aria-busy={busy}>
        {busy ? <p>{operation === "check" ? t("正在检查连接…") : t("正在检查环境并配置扩展…")}</p>
          : message ? <ConnectionNotice error={attention}>{t(message)}</ConnectionNotice>
          : connected ? null
          : <p>{t("退出并重新打开 Spotify。连接成功后，安装步骤会自动收起。")}</p>}
        {!busy && setup?.issue?.repairable && <p className="connection-repair-note">{t("将旧目录移入恢复副本，保留当前主题、其他扩展和有效备份，然后继续配置。")}</p>}
        {!busy && (attention || setup?.phase === "pending") && <div className="connection-actions">
          <button type="button" className="button primary" onClick={() => void (setup?.issue?.repairable ? configure(true) : setup?.issue ? check() : configure())}>
            {setup?.issue?.repairable ? t("处理并继续") : setup?.issue ? t("重新检查") : t("重新连接")}
          </button>
          {setup?.issue?.code === "legacy_state" && !setup.issue.repairable && <button type="button" className="button secondary"
            onClick={() => openExternalUrl("https://www.spotify.com/download/windows/").catch(failure)}>{t("下载 Spotify 桌面版")}</button>}
          {setup?.issue && <button type="button" className="text-button"
            onClick={() => call("open_spotify_config_folder").catch(failure)}>{t("打开配置文件夹")}</button>}
        </div>}
        {connected && !busy && <div className="connection-actions">
          <button type="button" className="button secondary" onClick={() => void configure()}>{t("重新配置扩展")}</button>
          <button type="button" className="button secondary" onClick={() => call("open_spotify_extension_folder").catch(failure)}>{t("打开扩展文件夹")}</button>
        </div>}
        <InstallationGuide title={t("详细信息")} open={detailsOpen} onOpenChange={setDetailsOpen}>
          {setup?.issue?.path && <CopyableInstruction value={setup.issue.path} label={t("复制路径")} onError={setError} />}
          {setup?.issue?.code === "state_path" && <p>{t("SPICETIFY_STATE 需指向名为 spicetify 的目录。调整或取消该环境变量后，重新打开 DiscoAS。")}</p>}
          {setup?.issue?.code === "legacy_state" && !setup.issue.repairable && <p>{t("先恢复 Spotify 的官方客户端文件，保留上述目录中的主题与扩展，再点击重新检查。")}</p>}
          {setup?.recoveryPath && <><p>{t("旧目录的恢复副本：")}</p><CopyableInstruction value={setup.recoveryPath} label={t("复制路径")} onError={setError} /></>}
          <p>{t("连接时会添加 DiscoAS 扩展，沿用现有 Spicetify 配置。首次使用 Spotify，请先打开客户端并登录。")}</p>
          {setup?.toolVersion && <p className="connection-detail-line">Spicetify {setup.toolVersion === "existing" ? t("使用已有安装") : setup.toolVersion}</p>}
          <div className="connection-actions">
            {!connected && <button type="button" className="button secondary" disabled={busy} onClick={() => void configure()}>{t("重新配置扩展")}</button>}
            {!connected && status?.extensionPath && <button type="button" className="button secondary" disabled={busy}
              onClick={() => call("open_spotify_extension_folder").catch(failure)}>{t("打开扩展文件夹")}</button>}
            <button type="button" className="text-button" onClick={() => openExternalUrl("https://spicetify.app/docs/getting-started").catch(failure)}>{t("Spicetify 官方说明")}</button>
          </div>
        </InstallationGuide>
      </div></Reveal>
    </Reveal>
  </section>;
}
