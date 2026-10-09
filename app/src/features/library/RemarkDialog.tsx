import { useState } from "react";
import type { LibraryEntry } from "../../types";
import { Icon } from "../../Icon";
import { t, errorText } from "../../i18n";

export function RemarkDialog({ entry, busy, close, save }: {
  entry: LibraryEntry;
  busy: boolean;
  close: () => void;
  save: (remark: string) => Promise<void>;
}) {
  const [remark, setRemark] = useState(entry.remark);
  const [error, setError] = useState("");
  return <div className="modal-backdrop" onClick={() => !busy && close()}>
    <form className="dialog" role="dialog" aria-modal="true" aria-labelledby="remark-title" onClick={(event) => event.stopPropagation()}
      onSubmit={async (event) => {
        event.preventDefault();
        setError("");
        try { await save(remark); } catch (failure) { setError(errorText(failure)); }
      }}>
      <div className="dialog-heading"><div><h2 id="remark-title">{t("编辑备注")}</h2><p>{entry.title}</p></div>
        <button type="button" className="icon-button" aria-label={t("关闭")} disabled={busy} onClick={close}><Icon name="close" /></button></div>
      <label className="field-label">{t("备注")}<input autoFocus maxLength={160} value={remark} disabled={busy} onChange={(event) => setRemark(event.target.value)} /></label>
      {error && <p className="form-error" role="alert">{error}</p>}
      <div className="dialog-actions"><button type="button" className="button secondary" disabled={busy} onClick={close}>{t("取消")}</button>
        <button className="button primary" disabled={busy || remark === entry.remark}>{busy ? t("保存中") : t("保存")}</button></div>
    </form>
  </div>;
}
