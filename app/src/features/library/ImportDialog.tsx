import { useEffect, useRef, useState } from "react";
import { Icon } from "../../Icon";
import { platforms } from "../../types";
import { platformInfo } from "../../platforms";
import { t, errorText, sourceKind } from "../../i18n";
import type { LibraryProgress } from "../../types";
import { OperationProgress } from "./OperationProgress";
import { identifySource, sourceKinds } from "./sourceInput";
import { Select } from "../../components/Select";

export function ImportDialog({
  busy,
  close,
  submit,
  progress,
  cancelling,
  cancel,
}: {
  busy: boolean;
  close: () => void;
  submit: (data: Record<string, unknown>) => Promise<void>;
  progress: LibraryProgress | null;
  cancelling: boolean;
  cancel: () => void;
}) {
  const [platform, setPlatform] = useState(platforms[0].id);
  const [typename, setType] = useState("playlist");
  const [source, setSource] = useState("");
  const [remark, setRemark] = useState("");
  const [error, setError] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    inputRef.current?.focus();
  }, []);
  const onSource = (value: string) => {
    setSource(value);
    setError("");
    const identified = identifySource(value);
    if (identified) {
      setPlatform(identified.platform);
      setType(identified.kind);
    }
  };
  return (
    <div className="modal-backdrop" onClick={() => !busy && close()}>
      <form
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="import-title"
        onClick={(e) => e.stopPropagation()}
        onSubmit={async (e) => {
          e.preventDefault();
          setError("");
          try {
            await submit({ platform, typename, source, remark });
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
        <label className="field-label">
          {t("音乐平台")}
          <Select label={t("音乐平台")}
            value={platform}
            onChange={(value) => {
              setPlatform(value);
              if (!sourceKinds(value).includes(typename)) setType(sourceKinds(value)[0]);
            }}
            disabled={busy}
            options={platforms.map((p) => ({ value: p.id, label: t(p.label) }))}
          />
        </label>
        <div className="segmented">
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
        <label className="field-label">
          {t("分享链接或 ID")}
          <input
            ref={inputRef}
            required
            value={source}
            onChange={(e) => onSource(e.target.value)}
            placeholder={t(platformInfo(platform).hint)}
            disabled={busy}
          />
        </label>
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
          {t("支持公开来源。私有、已删除或受限内容可能无法读取。")}
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
            disabled={busy || !source.trim()}
            type="submit"
          >
            <Icon
              name={busy ? "refresh" : "import"}
              className={busy ? "spin" : ""}
            />
            {busy ? t("导入中") : t("导入")}
          </button>
        </div>
      </form>
    </div>
  );
}
