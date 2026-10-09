import { useEffect, useRef, useState } from "react";
import type { BrowserBridgeStatus, DesktopPreferences } from "../../types";
import { call, desktop } from "../../services/desktop";
import { SettingRow } from "../../components/SettingsControls";
import { Select } from "../../components/Select";
import { t, errorText } from "../../i18n";
import { CopyableInstruction, InstallationGuide } from "./InstallationGuide";

export function BrowserSettings({ settings, setSettings }: {
  settings: DesktopPreferences;
  setSettings: React.Dispatch<React.SetStateAction<DesktopPreferences>>;
}) {
  const [status, setStatus] = useState<BrowserBridgeStatus | null>(null);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState("");
  const [guideOpen, setGuideOpen] = useState(false);
  const [preparedPath, setPreparedPath] = useState("");
  const [browser, setBrowser] = useState<"chromium" | "firefox">("chromium");
  const browserChosen = useRef(false);
  const extensionPath = preparedPath || status?.extensionPaths?.[browser]
    || (browser === "chromium" ? status?.extensionPath : null);
  const connected = Boolean(status?.connected &&
    (status.connectedBrowser ? status.connectedBrowser === browser : browser === "chromium"));
  useEffect(() => {
    let active = true;
    if (desktop) call<BrowserBridgeStatus>("get_browser_bridge_status").then((next) => {
      if (!active) return;
      setStatus(next);
      if (!browserChosen.current && next.connected && next.connectedBrowser) setBrowser(next.connectedBrowser);
    }).catch(() => {});
    return () => { active = false; };
  }, []);
  const action = async (prepare: boolean) => {
    setWorking(true); setError("");
    try {
      if (prepare) {
        setGuideOpen(true);
        setPreparedPath(await call<string>("export_browser_extension", { browser }));
        await call("open_browser_extension_folder", { browser });
      }
      setStatus(await call<BrowserBridgeStatus>("get_browser_bridge_status"));
    } catch (failure) { setError(errorText(failure)); }
    finally { setWorking(false); }
  };
  return <section className="settings-section">
    <h2>{t("YouTube / Bilibili")}</h2>
    <SettingRow title={t("浏览器播放方式")} description={t("启用复用播放页时，会为本程序单独启用页面")}>
      <Select
        label={t("浏览器播放方式")}
        value={settings.browser_playback_mode}
        onChange={(value) => setSettings((current) => ({ ...current, browser_playback_mode: value as DesktopPreferences["browser_playback_mode"] }))}
        options={[
          { value: "extension", label: t("浏览器扩展（复用播放页）") },
          { value: "direct", label: t("直接打开链接（新页面）") },
        ]}
      />
    </SettingRow>
    {settings.browser_playback_mode === "extension" && <SettingRow title={t("浏览器")}>
      <Select label={t("浏览器")} value={browser} disabled={working}
        onChange={(value) => { browserChosen.current = true; setBrowser(value as "chromium" | "firefox"); setPreparedPath(""); }}
        options={[{ value: "chromium", label: "Chrome / Edge" }, { value: "firefox", label: "Firefox" }]} />
    </SettingRow>}
    <div className="extension-settings">
      {settings.browser_playback_mode === "extension" ? <>
        <div className="extension-actions"><button className="button secondary" disabled={working} onClick={() => action(true)}>{t("准备浏览器扩展")}</button>
          <button className="button secondary" disabled={working} onClick={() => action(false)}>{t("检查连接")}</button></div>
        {status && <p role="status">{connected ? t("扩展已连接") : t("扩展未连接")}</p>}
        {status?.connected && status.connectedBrowser && status.connectedBrowser !== browser &&
          <p role="status">{t("{p0} 扩展已连接，切换前请先停用它", {
            p0: status.connectedBrowser === "firefox" ? "Firefox" : "Chrome / Edge",
          })}</p>}
        <InstallationGuide title={t("浏览器扩展安装教程")} open={guideOpen} onOpenChange={setGuideOpen}>
          <ol>
            <li>{t("点击“准备浏览器扩展”。打开的文件夹就是稍后要选择的扩展文件夹，不需要运行里面的文件。")}
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
            <li>{t("重新打开或刷新 YouTube / Bilibili 页面，保持 DiscoAS 运行，再点击“检查连接”。看到“扩展已连接”即可开始选歌；首次播放如被拦截，在网页点一次播放。")}</li>
          </ol>
          {browser === "firefox" && <>
            <p>{t("Firefox 需115或更新版本")}</p>
            <p>{t("普通版 Firefox 重启后会移除临时扩展，需要重新加载；永久安装需要 Mozilla 签名。")}</p>
          </>}
          <p>{t("应用更新后，在浏览器扩展页重新加载 DiscoAS 扩展，再刷新视频页面。")}</p>
          {!connected && <p>{t("仍未连接时，确认扩展已启用；切换浏览器前，先在原浏览器停用此扩展。")}</p>}
        </InstallationGuide>
      </> : null}
      {error && <p className="inline-error" role="alert">{error}</p>}
    </div>
  </section>;
}
