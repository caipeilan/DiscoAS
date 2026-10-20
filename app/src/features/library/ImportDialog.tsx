import { useEffect, useRef, useState } from "react";
import { Icon } from "../../Icon";
import { platforms } from "../../types";
import { platformInfo } from "../../platforms";
import { t, errorText, sourceKind } from "../../i18n";
import type { LibraryProgress } from "../../types";
import { OperationProgress } from "./OperationProgress";
import { extractSourceLink, identifySource, sourceKinds } from "./sourceInput";
import { PlatformIcon } from "../../components/PlatformIcon";
import { call } from "../../services/desktop";
import { Reveal } from "../../components/Motion";

export function ImportDialog({
  busy,
  close,
  submit,
  progress,
  cancelling,
  cancel,
  initialPaths = [],
}: {
  busy: boolean;
  close: () => void;
  submit: (data: Record<string, unknown>) => Promise<void>;
  progress: LibraryProgress | null;
  cancelling: boolean;
  cancel: () => void;
  initialPaths?: string[];
}) {
  const [platform, setPlatform] = useState("NeteaseCloudMusic");
  const [paths, setPaths] = useState<string[]>(initialPaths);
  const [picking, setPicking] = useState(false);
  useEffect(() => { if (initialPaths.length) { setPaths(initialPaths); setPlatform("LocalMusic"); setType("playlist"); } }, [initialPaths]);
  const [typename, setType] = useState("playlist");
  const [source, setSource] = useState("");
  const [remark, setRemark] = useState("");
  const [error, setError] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    inputRef.current?.focus();
  }, []);
  const onSource = (value: string) => {
    const extracted = extractSourceLink(value);
    setSource(extracted);
    setError("");
    const identified = identifySource(extracted);
    if (identified) {
      setPlatform(identified.platform);
      setType(identified.kind);
    }
  };
  const recognized = identifySource(source);
  const automaticallyDetected = recognized?.platform === platform && recognized.kind === typename;
  return (
    <div className="modal-backdrop" onClick={() => !busy && close()}>
      <form
        className="dialog import-dialog"
        role="dialog"
        tabIndex={-1}
        aria-modal="true"
        aria-labelledby="import-title"
        onClick={(e) => e.stopPropagation()}
        onSubmit={async (e) => {
          e.preventDefault();
          setError("");
          try {
            await submit(platform === "LocalMusic" ? { platform, paths, remark } : { platform, typename, source, remark });
          } catch (error) {
            setError(errorText(error));
          }
        }}
      >
        <div className="dialog-heading">
          <div>
            <h2 id="import-title">{t("添加歌单")}</h2>
          </div>
          <button
            type="button"
            className="icon-button"
            aria-label={t("关闭导入窗口")}
            disabled={busy}
            onClick={close}
          >
            <Icon name="close" />
          </button>
        </div>
        <fieldset className="motion-fields" disabled={busy || platform === "LocalMusic"}>
        <Reveal open={platform !== "LocalMusic"}>
        <label className="field-label import-link-field">
          {t("分享链接或 ID")}
          <input ref={inputRef} required value={source} onChange={(e) => onSource(e.target.value)}
            placeholder={t("粘贴分享链接，自动识别平台")}
            aria-describedby="import-link-hint" disabled={busy} autoComplete="off" />
        </label>
        <p className="import-recognition" id="import-link-hint" role="status">
          {automaticallyDetected ? <><Icon name="check" size={15} />{t("已识别：{p0} · {p1}", { p0: t(platformInfo(platform).label), p1: sourceKind(typename) })}</> : t("也可输入 ID，再选择对应平台与类型。")}
        </p>
        </Reveal></fieldset>
        <fieldset className="import-platform-field" disabled={busy}>
          <legend className="field-label">{t("音乐平台")}</legend>
          <div className="import-platforms">{platforms.map(p => <button type="button" key={p.id}
            aria-pressed={platform === p.id} className={platform === p.id ? "selected" : ""}
            onClick={() => { setPlatform(p.id); setError(""); if (!sourceKinds(p.id).includes(typename)) setType(sourceKinds(p.id)[0]); }}>
            <PlatformIcon id={p.id} /><span>{t(p.label)}</span>{platform === p.id && <Icon name="check" size={13} className="platform-check" />}
          </button>)}</div>
        </fieldset>
        {["AppleMusic", "YouTube", "Bilibili"].includes(platform) && (
          <div className="import-platform-note" role="note">
            <Icon name="info" size={16} />
            <span>{t("由于软件限制，Apple Music、YouTube、Bilibili 只可在网页端运行，请在设置中进行调整。")}</span>
          </div>
        )}
        <fieldset className="motion-fields" disabled={busy || platform === "LocalMusic"}>
        <Reveal open={platform !== "LocalMusic"}><div className="segmented import-kinds" role="group" aria-label={t("内容类型")}>
          {sourceKinds(platform).map((kind) => <button key={kind}
            type="button"
            disabled={busy}
            aria-pressed={typename === kind}
            className={typename === kind ? "selected" : ""}
            onClick={() => setType(kind)}
          >
            {sourceKind(kind)}
          </button>)}
        </div>
        </Reveal></fieldset>
        <fieldset className="motion-fields" disabled={busy || platform !== "LocalMusic"}>
        <Reveal open={platform === "LocalMusic"}><div className="local-audio-input">
          <div className="local-audio-choices">{(["files", "folder", "playlist"] as const).map((kind) => <button key={kind} type="button" className="button secondary" disabled={busy || picking} onClick={async () => {
            setPicking(true); setError("");
            try { const selected = await call<string[]>("choose_local_audio", { kind }); if (selected.length) setPaths(selected); }
            catch (error) { setError(errorText(error)); }
            finally { setPicking(false); }
          }}>{t({ files: "选择音频文件", folder: "选择文件夹", playlist: "选择 M3U 歌单" }[kind])}</button>)}</div>
          <div className="local-audio-paths" role="status">{paths.length ? <><strong>{t("已选择 {p0} 个路径", { p0: paths.length })}</strong><ul>{paths.slice(0, 8).map((path) => <li key={path} title={path}>{path}</li>)}</ul>{paths.length > 8 && <span>{t("另有 {p0} 个路径", { p0: paths.length - 8 })}</span>}</> : <span>{t("也可将音频文件、文件夹或 M3U 歌单拖入窗口。")}</span>}</div>
        </div></Reveal></fieldset>
        <label className="field-label">
          {t("备注")}
          <span className="optional">{t("可选")}</span>
          <input
            value={remark}
            maxLength={160}
            onChange={(e) => setRemark(e.target.value)}
            placeholder={t("例如：通勤、二次元、最近喜欢")}
            disabled={busy}
          />
        </label>
        <p className="form-hint">
          <Icon name="info" size={15} />
          {platform === "LocalMusic" ? t("支持 MP3、FLAC、WAV、M4A、OGG；文件夹包含子目录。播放使用系统默认播放器。") : t("支持公开卡池。私有、已删除或受限内容可能无法读取。")}
          {platform === "Bilibili" && t("多 P 视频按分 P 分别发现。")}
        </p>
        {error && (
          <div className="form-error" role="alert">
            {error}
          </div>
        )}
        {busy && (
          <OperationProgress progress={progress} cancelling={cancelling} />
        )}
        <div className="dialog-actions">
          {busy && <button className="button secondary" type="button" onClick={close}>{t("后台继续")}</button>}
          <button
            className="button secondary"
            type="button"
            onClick={busy ? cancel : close}
            disabled={cancelling || (busy && (progress?.phase === "saving" || progress?.phase === "done"))}
          >
            {cancelling ? t("正在取消") : t("取消")}
          </button>
          <button
            className="button primary"
            disabled={busy || picking || (platform === "LocalMusic" ? !paths.length : !source.trim())}
            type="submit"
          >
            <Icon
              name={busy ? "refresh" : "import"}
              className={busy ? "spin" : ""}
            />
            {busy ? t("添加中") : t("添加歌单")}
          </button>
        </div>
      </form>
    </div>
  );
}
