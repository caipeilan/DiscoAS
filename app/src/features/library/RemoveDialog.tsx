import type { LibraryEntry } from "../../types";
import { t } from "../../i18n";
import { PlatformTag } from "../../components/PlatformTag";
import { sourceKey } from "./libraryModel";
import { sourceKind } from "../../i18n";

export function RemoveDialog({ remove, busy, close, confirm }: {
  remove: LibraryEntry[];
  busy: string;
  close: () => void;
  confirm: () => Promise<void>;
}) {
  return (
    <div
      className="modal-backdrop"
      onClick={() => !busy && close()}
    >
      <section
        className="dialog small-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="remove-title"
        onClick={(e) => e.stopPropagation()}
      >
        <h2 id="remove-title">{t("删除 {p0} 个来源？", { p0: remove.length })}</h2>
        <ul className="remove-source-list">
          {remove.map((entry) => <li key={sourceKey(entry)}>
            <strong>{entry.title}</strong>
            <span className="row-meta"><PlatformTag id={entry.platform} /><span>{sourceKind(entry.kind)}</span></span>
          </li>)}
        </ul>
        <p>{t("删除后不再用于发现。本地缓存会保留，可以重新添加。")}</p>
        <div className="dialog-actions">
          <button
            className="button secondary"
            disabled={!!busy}
            onClick={() => close()}
          >
            {t("取消")}
          </button>
          <button
            className="button danger"
            disabled={!!busy}
            onClick={confirm}
          >
            {t("删除")}
          </button>
        </div>
      </section>
    </div>

  );
}
