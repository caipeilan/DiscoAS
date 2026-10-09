import { Icon } from "../../Icon";
import { PlatformTag } from "../../components/PlatformTag";
import { Select } from "../../components/Select";
import { currentLocale, t } from "../../i18n";
import { historyCoverKey, historyIdentity, historyKey } from "./historyModel";
import { useHistory, type BatchAction } from "./useHistory";
import "./history.css";

/** History uses the main window's scroll area, rather than a second viewport-sized dialog. */
export function HistoryView() {
  const history = useHistory();
  const { entries, filter, query, selected, loading, repairing, mutating, confirming, error, rows, covers,
    allVisibleSelected, selectedCount, visible, pageCount, currentPage } = history;
  return <>
    <header className="page-header history-topbar">
      <div className="history-heading">
        <h1 id="history-title">{t("发现记录")}</h1>
        {!loading && <span className="history-total">{t("{p0} 条记录", { p0: entries.length.toLocaleString(currentLocale()) })}</span>}
      </div>
      <button className="button secondary compact delete-action" disabled={loading || mutating || !entries.length || !!confirming}
        onClick={() => history.setConfirming({ kind: "clear" })}><Icon name="remove" size={15} />{t("清空记录")}</button>
      {confirming && <div className="history-confirmation" role="alert">
        <span>{confirming.kind === "clear" ? t("确认清空全部记录？") : t("确认删除 {p0} 条记录？", { p0: confirming.identities.length })}</span>
        <div><button className="button secondary compact" disabled={mutating} onClick={() => history.setConfirming(null)}>{t("取消")}</button>
          <button className="button danger compact" disabled={mutating} onClick={history.confirmMutation}>{confirming.kind === "clear" ? t("清空") : t("删除")}</button></div>
      </div>}
    </header>
    <section className="history-page" aria-labelledby="history-title" aria-busy={loading || mutating}>
      <fieldset className="history-controls" disabled={mutating || loading || !!confirming}>
        <div className="history-toolbar">
          <label className="search-field history-search"><Icon name="search" size={17} /><input aria-label={t("搜索发现记录")}
            placeholder={t("搜索歌名、歌手、平台或 ID")} value={query} onChange={(event) => history.changeQuery(event.target.value)} /></label>
          <div className="segmented history-filters" role="group" aria-label={t("发现记录")}>
            <button type="button" aria-pressed={filter === "all"} className={filter === "all" ? "selected" : ""} onClick={() => history.changeFilter("all")}>{t("全部记录")}</button>
            <button type="button" aria-pressed={filter === "discovered"} className={filter === "discovered" ? "selected" : ""} onClick={() => history.changeFilter("discovered")}>{t("已入选")}</button>
            <button type="button" aria-pressed={filter === "selected"} className={filter === "selected" ? "selected" : ""} onClick={() => history.changeFilter("selected")}>{t("已选择")}</button>
          </div>
        </div>
        <div className="history-selection">
          <label><input ref={history.selectAll} type="checkbox" checked={allVisibleSelected} disabled={!visible.length}
            onChange={(event) => history.selectVisible(event.target.checked)} />{t("选择当前列表")}</label>
          <span className="history-selected-count">{t("已选 {p0} 条", { p0: selectedCount })}</span>
          <div className="history-batch-actions">
            <Select label={t("批量操作")} value={history.batchAction} onChange={(value) => history.setBatchAction(value as BatchAction)}
              disabled={!selectedCount || mutating || !!confirming} options={[
                { value: "discovered-on", label: t("标为入选") }, { value: "discovered-off", label: t("取消入选") },
                { value: "selected-on", label: t("标为选择") }, { value: "selected-off", label: t("取消选择") },
                { value: "delete", label: t("删除记录") },
              ]} />
            <button type="button" className={`button secondary compact${history.batchAction === "delete" ? " delete-action" : ""}`}
              disabled={!selectedCount} onClick={history.runBatch}>{t("应用")}</button>
          </div>
        </div>
      </fieldset>
      {error && <p className="form-error history-message" role="alert">{error}</p>}
      {repairing && <p className="history-message" role="status"><Icon name="refresh" size={14} className="spin" />{t("正在补全旧记录的信息")}</p>}
      {loading || !visible.length ? <div className="history-empty" role="status">
        <Icon name={loading ? "refresh" : query ? "search" : "history"} size={28} className={loading ? "spin" : ""} />
        <p>{loading ? t("正在读取记录") : entries.length ? t("没有匹配的记录") : t("暂无记录")}</p>
      </div> : <ol className="history-records">{rows.map((entry) => {
        const identity = historyIdentity(entry);
        const key = historyKey(entry);
        const cover = covers.get(historyCoverKey(entry));
        const date = entry.selectedAt ?? entry.discoveredAt;
        return <li key={key} data-selected={selected.has(key) || undefined}>
          <input type="checkbox" aria-label={t("选择记录 {p0}", { p0: entry.name || entry.songId })} checked={selected.has(key)} disabled={mutating || !!confirming}
            onChange={(event) => history.toggleSelected(entry, event.target.checked)} />
          <div className="history-cover">{cover ? <img src={cover} alt="" draggable={false} onError={() => history.failCover(entry)} /> : <Icon name="music" size={22} />}</div>
          <div className="history-record-content">
            <strong>{!entry.name || /^[?？]+$/.test(entry.name) || entry.name === "神秘歌曲" ? t("信息暂不可用") : entry.name}</strong>
            <span className="history-artists">{["YouTube", "Bilibili"].includes(entry.platform) ? t("频道 / UP 主：") : ""}{entry.artistNames.filter((name) => !/^[?？]+$/.test(name)).join(" / ")}</span>
            <div className="row-meta"><PlatformTag id={entry.platform} /><span className="history-song-id">{entry.songId}</span>
              {date !== null && <time dateTime={new Date(date).toISOString()}>{new Date(date).toLocaleString(currentLocale())}</time>}</div>
          </div>
          <fieldset className="history-row-status" disabled={mutating || !!confirming}>
            <label><input type="checkbox" checked={entry.discoveredAt !== null}
              onChange={(event) => history.mutate([identity], "discovered", event.target.checked)} />{t("已入选")}</label>
            <label><input type="checkbox" checked={entry.selectedAt !== null}
              onChange={(event) => history.mutate([identity], "selected", event.target.checked)} />{t("已选择")}</label>
          </fieldset>
          <button className="icon-button delete-action history-delete" aria-label={t("删除记录 {p0}", { p0: entry.name || entry.songId })} disabled={mutating || !!confirming}
            onClick={() => history.setConfirming({ kind: "delete", identities: [identity] })}><Icon name="remove" size={16} /></button>
        </li>;
      })}</ol>}
      {visible.length > 0 && <footer className="history-pagination">
        <span>{t("第 {p0} / {p1} 页", { p0: currentPage + 1, p1: pageCount })}</span>
        <div><button className="button compact secondary" disabled={mutating || !!confirming || currentPage === 0} onClick={() => history.setPage(currentPage - 1)}>{t("上一页")}</button>
          <button className="button compact secondary" disabled={mutating || !!confirming || currentPage + 1 >= pageCount} onClick={() => history.setPage(currentPage + 1)}>{t("下一页")}</button></div>
      </footer>}
    </section>
  </>;
}
