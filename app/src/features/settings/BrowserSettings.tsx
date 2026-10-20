import { useEffect, useRef, useState } from "react";
import type { BrowserBridgeStatus, DesktopPreferences } from "../../types";
import { call, desktop } from "../../services/desktop";
import { SettingRow } from "../../components/SettingsControls";
import { Select } from "../../components/Select";
import { t, errorText } from "../../i18n";
import { CopyableInstruction, InstallationGuide } from "./InstallationGuide";
import { Reveal } from "../../components/Motion";
import { Icon } from "../../Icon";
import { ConnectionSteps } from "./ConnectionSteps";
import { InlineFeedback } from "../../components/InlineFeedback";

export function BrowserSettings({ settings, setSettings }: {
  settings: DesktopPreferences;
  setSettings: React.Dispatch<React.SetStateAction<DesktopPreferences>>;
}) {
  const [status, setStatus] = useState<BrowserBridgeStatus | null>(null);
  const [operation, setOperation] = useState<"" | "prepare" | "check">("");
  const lastPrepared = useRef(false);
  const working = operation !== "";
  const [error, setError] = useState("");
  const [guideOpen, setGuideOpen] = useState(false);
  const [preparedPath, setPreparedPath] = useState("");
  const [browser, setBrowser] = useState<"chromium" | "firefox">("chromium");
  const browserChosen = useRef(false);
  const section = useRef<HTMLElement>(null);
  const [inView, setInView] = useState(false);
  const [statusError, setStatusError] = useState("");
  const extensionPath = preparedPath || status?.extensionPaths?.[browser]
    || (browser === "chromium" ? status?.extensionPath : null);
  const connected = Boolean(status?.connected &&
    (status.connectedBrowser ? status.connectedBrowser === browser : browser === "chromium"));
  useEffect(() => {
    const observer = new IntersectionObserver(([entry]) => setInView(entry.isIntersecting));
    if (section.current) observer.observe(section.current);
    return () => observer.disconnect();
  }, []);
  useEffect(() => {
    if (!desktop || settings.browser_playback_mode !== "extension" || !inView || working) return;
    let active = true;
    let timer: ReturnType<typeof setInterval> | undefined;
    const refresh = () => call<BrowserBridgeStatus>("get_browser_bridge_status").then((next) => {
      if (!active) return;
      setStatus(next); setStatusError("");
      if (!browserChosen.current && next.connected && next.connectedBrowser) setBrowser(next.connectedBrowser);
    }).catch((failure) => { if (active) setStatusError(errorText(failure)); });
    const visibility = () => {
      clearInterval(timer);
      if (document.hidden) return;
      void refresh(); timer = setInterval(refresh, 4000);
    };
    visibility();
    document.addEventListener("visibilitychange", visibility);
    return () => { active = false; clearInterval(timer); document.removeEventListener("visibilitychange", visibility); };
  }, [inView, working, settings.browser_playback_mode]);
  const action = async (prepare: boolean) => {
    lastPrepared.current = prepare;
    setOperation(prepare ? "prepare" : "check"); setError("");
    try {
      if (prepare) {
        setGuideOpen(true);
        setPreparedPath(await call<string>("export_browser_extension", { browser }));
        await call("open_browser_extension_folder", { browser });
      }
      setStatus(await call<BrowserBridgeStatus>("get_browser_bridge_status"));
      setStatusError("");
    } catch (failure) { setError(errorText(failure)); }
    finally { setOperation(""); }
  };
  return <section ref={section} className="settings-section">
    <h2>{t("Apple Music / YouTube / Bilibili")}</h2>
    <SettingRow title={t("播放方式")} description={t("连接后会重复使用同一个播放标签页。请保持浏览器和 DiscoAS 运行。") }>
      <Select
        label={t("浏览器播放方式")}
        value={settings.browser_playback_mode}
        onChange={(value) => setSettings((current) => ({ ...current, browser_playback_mode: value as DesktopPreferences["browser_playback_mode"] }))}
        options={[
          { value: "extension", label: t("连接浏览器播放器") },
          { value: "direct", label: t("在新标签页中打开") },
        ]}
      />
    </SettingRow>
    <Reveal open={settings.browser_playback_mode === "extension"}><SettingRow title={t("浏览器")}>
      <Select label={t("浏览器")} value={browser} disabled={working}
        onChange={(value) => { browserChosen.current = true; setBrowser(value as "chromium" | "firefox"); setPreparedPath(""); }}
        options={[{ value: "chromium", label: "Chrome / Edge" }, { value: "firefox", label: "Firefox" }]} />
    </SettingRow>
    <div className="extension-settings" aria-busy={working}>
        <div className="connection-status" role="status" data-connected={connected}>
          <Icon name={connected ? "check" : "info"} size={21} /><div>
            <strong>{connected ? t("浏览器已连接") : preparedPath ? t("在浏览器中加载扩展") : t("连接浏览器播放器")}</strong>
            <p>{connected ? t("选择内容后，将在连接的浏览器中播放。") : preparedPath ? t("按照下方说明加载扩展，再刷新播放页面。连接状态会自动更新。") : t("先准备扩展文件，再到浏览器中完成加载。")}</p>
          </div>
        </div>
        <ConnectionSteps steps={["准备文件", "在浏览器中加载", "确认连接"]} current={preparedPath ? 1 : 0} complete={connected} />
        <div className="extension-actions"><button type="button" className="button primary" disabled={working} onClick={() => action(!preparedPath && !connected)}><Icon name={working ? "refresh" : preparedPath || connected ? "refresh" : "folder"} className={working ? "spin" : ""} />{operation === "prepare" ? t("正在准备扩展…") : operation === "check" ? t("正在检查连接…") : preparedPath || connected ? t("检查连接") : t("准备扩展文件")}</button>
          {(preparedPath || connected) && <button type="button" className="button secondary" disabled={working} onClick={() => action(true)}>{t("打开扩展文件夹")}</button>}
          {!preparedPath && !connected && <button type="button" className="text-button" disabled={working} onClick={() => action(false)}>{t("检查连接")}</button>}</div>
        <InlineFeedback message={error || statusError} retry={() => { void action(Boolean(error) && lastPrepared.current); }} busy={working} dismiss={() => { setError(""); setStatusError(""); }} />
        {status?.connected && status.connectedBrowser && status.connectedBrowser !== browser &&
          <p role="status">{t("{p0} 扩展已连接，切换前请先停用它", {
            p0: status.connectedBrowser === "firefox" ? "Firefox" : "Chrome / Edge",
          })}</p>}
        <InstallationGuide title={t("加载扩展与故障排查")} open={guideOpen} onOpenChange={setGuideOpen}>
          <ol>
            <li>{t("点击“准备扩展文件”，在稍后的浏览器设置中选择打开的文件夹。")}
              {extensionPath && <CopyableInstruction label={t("复制路径")}
                value={extensionPath} onError={setError} />}
            </li>
            <li>{t("在要使用的浏览器地址栏粘贴对应地址并按 Enter：")}
              {browser === "firefox"
                ? <CopyableInstruction label={t("复制 Firefox 地址")} value="about:debugging#/runtime/this-firefox" onError={setError} />
                : <>
                  <CopyableInstruction label={t("复制 Chrome 地址")} value="chrome://extensions" onError={setError} />
                  <CopyableInstruction label={t("复制 Edge 地址")} value="edge://extensions" onError={setError} />
                </>}
            </li>
            <li>{browser === "firefox"
              ? t("点击“临时载入附加组件”，选择第 1 步文件夹内的 manifest.json 文件。")
              : t("开启“开发者模式”，点击“加载已解压的扩展程序”（Edge 可能显示“加载解压缩的扩展”）。选择第 1 步的整个文件夹，其中应包含 manifest.json。")}</li>
            <li>{t("重新打开或刷新 YouTube / Bilibili / Apple Music 页面，保持 DiscoAS 运行，再点击“检查连接”。看到“扩展已连接”即可开始选歌；首次播放如被拦截，在网页点一次播放。")}</li>
          </ol>
          {browser === "firefox" && <>
            <p>{t("Firefox 需115或更新版本")}</p>
            <p>{t("普通版 Firefox 重启后会移除临时扩展，需要重新加载；永久安装需要 Mozilla 签名。")}</p>
          </>}
          <p>{t("应用更新后，在浏览器扩展页重新加载 DiscoAS 扩展，再刷新视频页面。")}</p>
          {!connected && <p>{t("仍未连接时，确认扩展已启用；切换浏览器前，先在原浏览器停用此扩展。")}</p>}
        </InstallationGuide>
    </div></Reveal>
  </section>;
}
