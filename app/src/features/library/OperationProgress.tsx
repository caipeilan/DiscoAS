import { Icon } from "../../Icon";
import { t } from "../../i18n";
import type { LibraryProgress } from "../../types";

export function OperationProgress({ progress, cancelling, cancel, summary }: {
  progress: LibraryProgress | null;
  cancelling: boolean;
  cancel?: () => void;
  summary?: string;
}) {
  const labels = {
    fetching: t("正在读取内容"), cover: t("正在准备封面"),
    saving: t("正在保存"), done: t("已完成"),
  };
  return <div className="import-progress" role="status" aria-live="polite">
    <Icon name="refresh" className="spin" />
    <span>{summary && <small>{summary} · </small>}
      {cancelling ? t("正在取消") : labels[progress?.phase || "fetching"]}
      {progress?.phase === "fetching" && progress.completed > 0 &&
        ` · ${progress.completed.toLocaleString()}${progress.total == null ? "" : ` / ${progress.total.toLocaleString()}`} ${t("项")}`}
      {progress?.phase === "fetching" && !!progress.pages && ` · ${t("已读取 {p0} 页", { p0: progress.pages })}`}
    </span>
    {cancel && <button type="button" className="text-button" disabled={cancelling || progress?.phase === "saving" || progress?.phase === "done"} onClick={cancel}>{t("取消")}</button>}
  </div>;
}
