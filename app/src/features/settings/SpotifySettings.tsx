import { useEffect, useRef, useState } from "react";
import type { DesktopPreferences, SpotifyBridgeStatus, SpotifySetupStatus } from "../../types";
import { call, desktop, openExternalUrl } from "../../services/desktop";
import { SettingRow } from "../../components/SettingsControls";
import { Select } from "../../components/Select";
import { t, errorText } from "../../i18n";
import { InstallationGuide } from "./InstallationGuide";

export function SpotifySettings({ settings, setSettings }: {
  settings: DesktopPreferences;
  setSettings: React.Dispatch<React.SetStateAction<DesktopPreferences>>;
}) {
  const [status, setStatus] = useState<SpotifyBridgeStatus | null>(null);
  const [setup, setSetup] = useState<SpotifySetupStatus | null>(null);
  const [working, setWorking] = useState(false);
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
    setWorking(true); setError("");
    try {
      setStatus(await call<SpotifyBridgeStatus>("get_spotify_bridge_status"));
      setSetup(await call<SpotifySetupStatus>("get_spotify_setup_status"));
      setStatusError("");
    }
    catch (failure) { setError(errorText(failure)); }
    finally { setWorking(false); }
  };
  return <section ref={section} className="settings-section">
    <h2>Spotify</h2>
    <SettingRow title={t("切歌方式")} description={t("受客户端限制，Free账户切歌时会出现异常情况（如歌曲错误、切换失败等），使用spicetify只能避免部分异常情况")}>
      <Select
        label={t("Spotify 切歌方式")}
        value={settings.spotify_playback_mode}
        onChange={(value) => setSettings((current) => ({ ...current, spotify_playback_mode: value as DesktopPreferences["spotify_playback_mode"] }))}
        options={[
          { value: "extension", label: t("Spicetify 扩展（默认）") },
          { value: "pause_then_scheme", label: t("先暂停，再打开歌曲") },
          { value: "scheme", label: t("直接打开歌曲") },
        ]}
      />
    </SettingRow>
    {settings.spotify_playback_mode === "extension" && <div className="extension-settings">
      {setup && <p role="status">{t(setup.message)}{setup.toolVersion && ` · Spicetify ${setup.toolVersion}`}</p>}
      <div className="extension-actions"><button className="button secondary" disabled={working} onClick={async () => {
        setWorking(true); setError(""); setGuideOpen(true);
        try {
          setSetup(await call<SpotifySetupStatus>("configure_spotify_support"));
          setStatus(await call<SpotifyBridgeStatus>("get_spotify_bridge_status"));
        } catch (failure) { setError(errorText(failure)); }
        finally { setWorking(false); }
      }}>{working ? t("正在配置") : t("安装 / 配置 Spicetify")}</button>
        <button className="button secondary" disabled={working} onClick={check}>{t("检查连接")}</button>
      </div>
      {status && <p role="status">{status.connected ? t("扩展已连接") : t("扩展未连接")}</p>}
      <InstallationGuide title={t("Spicetify 安装教程")} open={guideOpen} onOpenChange={setGuideOpen}>
        <ol>
          <li>{t("先打开 Spotify 桌面版并登录，首次安装后保持运行至少 60 秒。网页版不支持此扩展。")}</li>
          <li>{t("点击本页“安装 / 配置 Spicetify”，等待显示“Spotify 扩展已配置”。DiscoAS 会准备 Spicetify 并添加自己的扩展，无需手动执行命令。")}</li>
          <li>{t("退出并重新打开 Spotify，再点击“检查连接”。看到“扩展已连接”后即可选歌。")}</li>
        </ol>
        <p>{t("首次安装需你点击按钮；安装 DiscoAS 本身不会安装 Spicetify。已有的主题、其它扩展和备份会保留。")}</p>
        <p>{t("Spotify 更新后连接失败，可再次点击“安装 / 配置 Spicetify”。如提示版本不兼容，按提示处理后重试。")}</p>
        <button type="button" className="text-button" onClick={() => openExternalUrl("https://spicetify.app/docs/getting-started").catch((failure) => setError(errorText(failure)))}>{t("Spicetify 官方说明")}</button>
        {status?.extensionPath && <p className="extension-path">{status.extensionPath}</p>}
        {status?.extensionPath && <button type="button" className="button secondary" onClick={() => call("open_spotify_extension_folder").catch((failure) => setError(errorText(failure)))}>{t("打开扩展文件夹")}</button>}
      </InstallationGuide>
      {(error || statusError) && <p className="inline-error" role="alert">{error || statusError}</p>}
    </div>}
  </section>;
}
