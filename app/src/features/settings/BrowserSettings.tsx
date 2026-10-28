import { useEffect, useRef, useState } from "react";
import type { BrowserBridgeStatus, DesktopPreferences } from "../../types";
import { call, desktop } from "../../services/desktop";
import { SettingRow } from "../../components/SettingsControls";
import { Select } from "../../components/Select";
import { Reveal } from "../../components/Motion";
import { t, errorText } from "../../i18n";
import { CopyableInstruction, InstallationGuide } from "./InstallationGuide";
import { ConnectionNotice, ConnectionSummary } from "./ConnectionControls";
import { browserConnection, connectionError, type ExtensionBrowser } from "./connectionModel";

export function BrowserSettings({ settings, setSettings }: {
  settings: DesktopPreferences;
  setSettings: React.Dispatch<React.SetStateAction<DesktopPreferences>>;
}) {
  const [status, setStatus] = useState<BrowserBridgeStatus | null>(null);
  const [operation, setOperation] = useState<"" | "prepare" | "folder">("");
  const working = operation !== "";
  const preparing = operation === "prepare";
  const [error, setError] = useState("");
  const [statusError, setStatusError] = useState("");
  const [open, setOpen] = useState(false);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [exported, setExported] = useState(false);
  const [browser, setBrowser] = useState<ExtensionBrowser>("chromium");
  const browserChosen = useRef(false);
  const wasConnected = useRef(false);
  const section = useRef<HTMLElement>(null);
  const [inView, setInView] = useState(false);
  const { path, connected, prepared } = browserConnection(status, browser);
  const message = connectionError(error || statusError);
  const family = browser === "firefox" ? "Firefox" : "Chrome / Edge";

  useEffect(() => {
    const observer = new IntersectionObserver(([entry]) => setInView(entry.isIntersecting));
    if (section.current) observer.observe(section.current);
    return () => observer.disconnect();
  }, []);
  useEffect(() => {
    if (connected && !wasConnected.current) { setOpen(false); setExported(false); }
    wasConnected.current = connected;
  }, [connected]);
  useEffect(() => {
    if (!desktop || settings.browserPlaybackMode !== "extension" || !inView || working) return;
    let active = true;
    let refreshing = false;
    let timer: ReturnType<typeof setInterval> | undefined;
    const refresh = async () => {
      if (refreshing) return;
      refreshing = true;
      try {
        const next = await call<BrowserBridgeStatus>("get_browser_bridge_status");
        if (active) {
          setStatus(next); setStatusError("");
          if (!browserChosen.current && next.connected && next.connectedBrowser) setBrowser(next.connectedBrowser);
        }
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
  }, [inView, working, settings.browserPlaybackMode]);

  const prepare = async (openFolder: boolean) => {
    if (working) return;
    setOperation("prepare"); setError(""); setStatusError(""); setOpen(true);
    try {
      await call<string>("export_browser_extension", { browser });
      setStatus(await call<BrowserBridgeStatus>("get_browser_bridge_status"));
      setExported(true);
      if (openFolder) await call("open_browser_extension_folder", { browser });
    } catch (failure) { setError(errorText(failure)); }
    finally { setOperation(""); }
  };
  const openFolder = async () => {
    if (working) return;
    setOperation("folder"); setError("");
    try { await call("open_browser_extension_folder", { browser }); }
    catch (failure) { setError(errorText(failure)); }
    finally { setOperation(""); }
  };
  const toggle = () => {
    if (open) { setOpen(false); return; }
    setOpen(true);
    if (!prepared && !connected) void prepare(true);
  };

  return <section ref={section} className="settings-section">
    <h2>{t("Apple Music / YouTube / Bilibili")}</h2>
    <SettingRow title={t("播放方式")} description={t("连接后会重复使用同一个播放标签页。请保持浏览器和 DiscoAS 运行。") }>
      <Select label={t("浏览器播放方式")} value={settings.browserPlaybackMode}
        onChange={(value) => setSettings((current) => ({ ...current, browserPlaybackMode: value as DesktopPreferences["browserPlaybackMode"] }))}
        options={[{ value: "extension", label: t("连接浏览器播放器") }, { value: "direct", label: t("在新标签页中打开") }]} />
    </SettingRow>
    <Reveal open={settings.browserPlaybackMode === "extension"}>
      <SettingRow title={t("浏览器")}>
        <Select label={t("浏览器")} value={browser} disabled={working}
          onChange={(value) => { browserChosen.current = true; setBrowser(value as ExtensionBrowser); setError(""); setExported(false); }}
          options={[{ value: "chromium", label: "Chrome / Edge" }, { value: "firefox", label: "Firefox" }]} />
      </SettingRow>
      <ConnectionSummary connected={connected} attention={Boolean(message)} busy={working} open={open} onToggle={toggle}
        status={preparing ? t("正在准备") : message ? t("需要处理") : connected ? t("{p0} 已连接", { p0: family }) : prepared ? t("等待浏览器") : t("未连接")} />
      <Reveal open={open}><div className="connection-flow" aria-busy={working}>
        {message && <ConnectionNotice error>{message}</ConnectionNotice>}
        {preparing && <p>{t("正在准备扩展文件…")}</p>}
        {!preparing && !prepared && !connected && <div className="connection-actions"><button type="button" className="button primary" disabled={working} onClick={() => void prepare(true)}>{t("重新准备文件")}</button></div>}
        {!preparing && prepared && (!connected || exported) && <>
          {status?.connected && status.connectedBrowser && status.connectedBrowser !== browser && <ConnectionNotice>
            {t("{p0} 扩展已连接，切换前请先停用它", { p0: status.connectedBrowser === "firefox" ? "Firefox" : "Chrome / Edge" })}
          </ConnectionNotice>}
          <ol className="connection-instructions">
            <li>{t("在浏览器地址栏打开扩展管理页：")}
              {browser === "firefox" ? <CopyableInstruction label={t("复制地址")} value="about:debugging#/runtime/this-firefox" onError={setError} /> : <>
                <CopyableInstruction label={t("复制 Chrome 地址")} value="chrome://extensions" onError={setError} />
                <CopyableInstruction label={t("复制 Edge 地址")} value="edge://extensions" onError={setError} />
              </>}
            </li>
            <li>{browser === "firefox"
              ? t("点击“临时载入附加组件”，选择下方文件夹中的 manifest.json。")
              : t("开启“开发者模式”，点击“加载已解压的扩展程序”，选择下方整个文件夹。")}
              <CopyableInstruction label={t("复制路径")} value={path} onError={setError} />
              <div className="connection-actions"><button type="button" className="button secondary" disabled={working} onClick={() => void openFolder()}>{t("打开扩展文件夹")}</button></div>
            </li>
          </ol>
          <p className="connection-waiting">{exported && connected
            ? t("在扩展管理页重新加载 DiscoAS 扩展，再刷新播放页面。")
            : t("加载后刷新播放页面，连接成功会自动收起这些步骤。")}</p>
          {browser === "firefox" && <p className="connection-waiting">{t("Firefox 重启后会移除临时扩展，届时需要重新加载。")}</p>}
        </>}
        {!preparing && connected && !exported && <>
          <CopyableInstruction value={path} label={t("复制路径")} onError={setError} />
          <div className="connection-actions">
            <button type="button" className="button secondary" disabled={working} onClick={() => void prepare(false)}>{t("重新导出扩展")}</button>
            <button type="button" className="button secondary" disabled={working} onClick={() => void openFolder()}>{t("打开扩展文件夹")}</button>
          </div>
        </>}
        <InstallationGuide title={t("详细信息")} open={detailsOpen} onOpenChange={setDetailsOpen}>
          <p>{t("首次播放被浏览器拦截时，在播放网页点一次播放。更新扩展后，需要重新加载扩展并刷新播放页面。")}</p>
          {browser === "firefox" && <p>{t("Firefox 需 115 或更新版本。永久安装需要 Mozilla 签名。")}</p>}
          {!connected && <div className="connection-actions">
            <button type="button" className="button secondary" disabled={working} onClick={() => void prepare(false)}>{t("重新导出扩展")}</button>
          </div>}
        </InstallationGuide>
      </div></Reveal>
    </Reveal>
  </section>;
}
