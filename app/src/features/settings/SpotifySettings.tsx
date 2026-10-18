import { useEffect, useRef, useState } from "react";
import type { DesktopPreferences, SpotifyBridgeStatus, SpotifySetupStatus } from "../../types";
import { call, desktop, openExternalUrl } from "../../services/desktop";
import { SettingRow } from "../../components/SettingsControls";
import { Select } from "../../components/Select";
import { t, errorText } from "../../i18n";
import { InstallationGuide } from "./InstallationGuide";
import { Reveal } from "../../components/Motion";
import { Icon } from "../../Icon";
import { ConnectionSteps } from "./ConnectionSteps";
import { InlineFeedback } from "../../components/InlineFeedback";

export function SpotifySettings({ settings, setSettings }: {
  settings: DesktopPreferences;
  setSettings: React.Dispatch<React.SetStateAction<DesktopPreferences>>;
}) {
  const [status, setStatus] = useState<SpotifyBridgeStatus | null>(null);
  const [setup, setSetup] = useState<SpotifySetupStatus | null>(null);
  const [operation, setOperation] = useState<"" | "check" | "configure">("");
  const lastAction = useRef<"check" | "configure">("check");
  const working = operation !== "";
  const [error, setError] = useState("");
  const [statusError, setStatusError] = useState("");
  const [guideOpen, setGuideOpen] = useState(false);
  const section = useRef<HTMLElement>(null);
  const [inView, setInView] = useState(false);
  useEffect(() => {
    const observer = new IntersectionObserver(([entry]) => setInView(entry.isIntersecting));
    if (section.current) observer.observe(section.current);
    return () => observer.disconnect();
  }, []);
  useEffect(() => {
    if (!desktop || settings.spotify_playback_mode !== "extension" || !inView || working) return;
    let active = true;
    let timer: ReturnType<typeof setInterval> | undefined;
    const refresh = () => Promise.all([
      call<SpotifyBridgeStatus>("get_spotify_bridge_status"),
      call<SpotifySetupStatus>("get_spotify_setup_status"),
    ]).then(([bridge, installed]) => {
      if (active) { setStatus(bridge); setSetup(installed); setStatusError(""); }
    }).catch((failure) => { if (active) setStatusError(errorText(failure)); });
    const visibility = () => {
      clearInterval(timer);
      if (document.hidden) return;
      void refresh();
      timer = setInterval(refresh, 4000);
    };
    visibility();
    document.addEventListener("visibilitychange", visibility);
    return () => { active = false; clearInterval(timer); document.removeEventListener("visibilitychange", visibility); };
  }, [settings.spotify_playback_mode, inView, working]);
  const check = async () => {
    lastAction.current = "check";
    setOperation("check"); setError("");
    try {
      setStatus(await call<SpotifyBridgeStatus>("get_spotify_bridge_status"));
      setSetup(await call<SpotifySetupStatus>("get_spotify_setup_status"));
      setStatusError("");
    }
    catch (failure) { setError(errorText(failure)); }
    finally { setOperation(""); }
  };
  const configure = async () => {
    lastAction.current = "configure";
    setOperation("configure"); setError(""); setGuideOpen(true);
    try {
      setSetup(await call<SpotifySetupStatus>("configure_spotify_support"));
      setStatus(await call<SpotifyBridgeStatus>("get_spotify_bridge_status"));
      setStatusError("");
    } catch (failure) { setError(errorText(failure)); }
    finally { setOperation(""); }
  };
  const connected = Boolean(status?.connected);
  const installed = setup?.phase === "ready";
  return <section ref={section} className="settings-section">
    <h2>Spotify</h2>
    <SettingRow title={t("播放方式")} description={t("Spotify Free 账户的选歌受客户端限制，扩展无法保证每次都播放指定歌曲。") }>
      <Select
        label={t("Spotify 切歌方式")}
        value={settings.spotify_playback_mode}
        onChange={(value) => setSettings((current) => ({ ...current, spotify_playback_mode: value as DesktopPreferences["spotify_playback_mode"] }))}
        options={[
          { value: "extension", label: t("连接桌面播放器") },
          { value: "pause_then_scheme", label: t("先暂停，再打开歌曲") },
          { value: "scheme", label: t("直接打开歌曲") },
        ]}
      />
    </SettingRow>
    <Reveal open={settings.spotify_playback_mode === "extension"}><div className="extension-settings" aria-busy={working}>
      <div className="connection-status" role="status" data-connected={connected}>
        <Icon name={connected ? "check" : "info"} size={21} /><div>
          <strong>{connected ? t("Spotify 已连接") : installed ? t("等待 Spotify 连接") : t("连接 Spotify 桌面版")}</strong>
          <p>{connected ? t("选择歌曲后，将在 Spotify 中播放。") : installed ? t("退出并重新打开 Spotify，连接状态会自动更新。") : t("先打开 Spotify 桌面版并登录，再安装连接扩展。")}</p>
        </div>
      </div>
      <ConnectionSteps steps={["安装扩展", "重启 Spotify", "确认连接"]} current={installed ? 1 : 0} complete={connected} />
      {!installed && !connected && <p className="connection-notice">{t("此操作会准备 Spicetify 并将 DiscoAS 扩展加入 Spotify 配置；已有主题、其他扩展和备份会保留。首次安装 Spotify 后，请先保持运行至少 60 秒。")}</p>}
      <div className="extension-actions">
        <button type="button" className="button primary" disabled={working} onClick={installed || connected ? check : configure}>
          <Icon name={working ? "refresh" : installed || connected ? "refresh" : "plus"} className={working ? "spin" : ""} />
          {operation === "configure" ? t("正在安装扩展…") : operation === "check" ? t("正在检查连接…") : installed || connected ? t("检查连接") : t("安装 Spicetify 扩展")}
        </button>
        {(installed || connected) && <button type="button" className="button secondary" disabled={working} onClick={configure}>{t("重新配置扩展")}</button>}
        {!installed && !connected && <button type="button" className="text-button" disabled={working} onClick={check}>{t("检查连接")}</button>}
      </div>
      {setup && <p role="status">{t(setup.message)}{setup.toolVersion && ` · Spicetify ${setup.toolVersion}`}</p>}
      <InlineFeedback message={error || statusError} retry={() => { void (error && lastAction.current === "configure" ? configure() : check()); }} busy={working} dismiss={() => { setError(""); setStatusError(""); }} />
      <InstallationGuide title={t("安装说明与故障排查")} open={guideOpen} onOpenChange={setGuideOpen}>
        <ol>
          <li>{t("先打开 Spotify 桌面版并登录，首次安装后保持运行至少 60 秒。网页版不支持此扩展。")}</li>
          <li>{t("点击“安装 Spicetify 扩展”，等待安装完成。DiscoAS 会准备 Spicetify 并添加自己的扩展。")}</li>
          <li>{t("退出并重新打开 Spotify，再点击“检查连接”。看到“扩展已连接”后即可选歌。")}</li>
        </ol>
        <p>{t("首次安装需你点击按钮；安装 DiscoAS 本身不会安装 Spicetify。已有的主题、其它扩展和备份会保留。")}</p>
        <p>{t("Spotify 更新后连接失败，可点击“重新配置扩展”。如提示版本不兼容，按提示处理后重试。")}</p>
        <button type="button" className="text-button" onClick={() => openExternalUrl("https://spicetify.app/docs/getting-started").catch((failure) => setError(errorText(failure)))}>{t("Spicetify 官方说明")}</button>
        {status?.extensionPath && <p className="extension-path">{status.extensionPath}</p>}
        {status?.extensionPath && <button type="button" className="button secondary" onClick={() => call("open_spotify_extension_folder").catch((failure) => setError(errorText(failure)))}>{t("打开扩展文件夹")}</button>}
      </InstallationGuide>
    </div></Reveal>
  </section>;
}
