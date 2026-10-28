import { useEffect, useRef, useState } from "react";
import { Icon } from "../../Icon";
import { call } from "../../services/desktop";
import { t, errorText } from "../../i18n";
import { platformInfo } from "../../platforms";
import { PlatformIcon } from "../../components/PlatformIcon";
import "./handControls.css";

export function CreateHandDialog({ platform, close }: { platform: string; close: () => void }) {
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const field = useRef<HTMLInputElement>(null);
  useEffect(() => { field.current?.focus(); }, []);
  useEffect(() => {
    const escape = (event: KeyboardEvent) => { if (event.key === "Escape" && !busy) { event.stopImmediatePropagation(); close(); } };
    window.addEventListener("keydown", escape, true);
    return () => window.removeEventListener("keydown", escape, true);
  }, [busy, close]);
  return <div className="modal-backdrop" onClick={() => { if (!busy) close(); }}>
    <form className="dialog import-dialog" role="dialog" aria-modal="true" aria-labelledby="create-hand-title" tabIndex={-1}
      onClick={event => event.stopPropagation()} onSubmit={async event => {
        event.preventDefault(); if (busy || !platform) return;
        setBusy(true); setError("");
        try { await call("create_hand_card", { input, platform }); close(); }
        catch (error) { setError(errorText(error)); }
        finally { setBusy(false); }
      }}>
      <div className="dialog-heading"><h2 id="create-hand-title">{t("创建手牌")}</h2>
        <button type="button" className="icon-button" aria-label={t("关闭创建手牌窗口")} disabled={busy} onClick={close}><Icon name="close" /></button></div>
      <p className="create-hand-platform">{platform && <PlatformIcon id={platform} />}<span>{platform ? t(platformInfo(platform).label) : t("请先启用一个歌单")}</span></p>
      <label className="field-label">{t("歌曲链接或 ID")}<input ref={field} value={input} required disabled={busy || !platform}
        placeholder={t("粘贴当前平台的单曲链接或输入 ID")} onChange={event => { setInput(event.target.value); setError(""); }} /></label>
      {error && <p className="form-error" role="alert">{error}</p>}
      <div className="dialog-actions"><button type="button" className="button secondary" disabled={busy} onClick={close}>{t("取消")}</button>
        <button type="submit" className="button primary" disabled={busy || !platform || !input.trim()}><Icon name={busy ? "refresh" : "check"} className={busy ? "spin" : ""} />{t(busy ? "创建中" : "创建手牌")}</button></div>
    </form>
  </div>;
}
