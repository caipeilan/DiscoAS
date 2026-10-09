import type { AppState, LibraryEntry, LibraryProgress } from "../../types";
import { platforms } from "../../types";
import { Icon } from "../../Icon";
import { Cover } from "../../components/Cover";
import { PlatformTag, formatDate } from "../../components/PlatformTag";
import { t, errorText, sourceKind, itemCountUnit } from "../../i18n";
import { sourceKey } from "./libraryModel";
import type { LibrarySort } from "./libraryModel";
import { OperationProgress } from "./OperationProgress";
import { Select } from "../../components/Select";

export function LibraryView({ state, busy, startup, query, setQuery, filter, setFilter, sort, setSort, filtered, selected, toggleSelected, selectVisible, updateSelected, removeSelected, editRemark, progress, cancelling, cancelOperation, operationSummary, importDialogOpen, openImport, confirmRemove, migrate, enablePlaylist, updatePlaylist, importSample }: {
  state: AppState;
  busy: string;
  startup: boolean;
  query: string;
  setQuery: (value: string) => void;
  filter: string;
  setFilter: (value: string) => void;
  sort: LibrarySort;
  setSort: (value: LibrarySort) => void;
  selected: Set<string>;
  toggleSelected: (entry: LibraryEntry) => void;
  selectVisible: () => void;
  updateSelected: () => Promise<unknown>;
  removeSelected: () => void;
  editRemark: (entry: LibraryEntry) => void;
  progress: LibraryProgress | null;
  cancelling: boolean;
  cancelOperation: () => void;
  operationSummary: string;
  importDialogOpen: boolean;
  filtered: LibraryEntry[];
  openImport: () => void;
  confirmRemove: (entry: LibraryEntry) => void;
  migrate: () => Promise<void>;
  enablePlaylist: (entry: LibraryEntry) => Promise<unknown>;
  updatePlaylist: (entry: LibraryEntry) => Promise<unknown>;
  importSample: () => Promise<unknown>;
}) {
  const enabled = state.playlists.find((p) => p.enabled);
  const normalCount = state.settings.number_of_discovered_songs;
  const mysteryCount = state.settings.have_mystery_song ? state.settings.num_of_mystery_song : 0;
  return (
    <>
      {state.startupRefreshError && (
        <p className="inline-error" role="status">
          {errorText(state.startupRefreshError)}
        </p>
      )}
      <header className="page-header">
        <div>
          <h1>{t("歌单")}</h1>
        </div>
        <button
          className="button primary"
          disabled={!!busy}
          onClick={() => openImport()}
        >
          <Icon name="plus" />
          {t("添加歌单")}
        </button>
      </header>
      {state.legacyDataPath && state.playlists.length === 0 && (
        <div className="migration-banner">
          <Icon name="import" />
          <div>
            <strong>{t("找到旧版数据")}</strong>
            <span>{t("可迁移已有歌单和发现设置。")}</span>
          </div>
          <button
            className="text-button"
            disabled={!!busy}
            onClick={migrate}
          >
            {t("选择旧版文件夹")}
            <Icon name="arrow" size={15} />
          </button>
        </div>
      )}
      {enabled && (
        <section className="active-source">
          <Cover
            url={enabled.coverUrl}
            data={enabled.coverDataUri}
            prepared
            label={enabled.title}
          />
          <div className="active-details">
            <span className="eyebrow">
              <span className="status-dot" />
              {t("当前卡池")}
            </span>
            <h2>{enabled.title}</h2>
            <div className="row-meta">
              <PlatformTag id={enabled.platform} />
              <span>
                {enabled.songCount.toLocaleString()}
                {itemCountUnit(enabled.platform, true)}
              </span>
              <span>
                {normalCount} + {mysteryCount}
                {t("首 / 次")}
              </span>
            </div>
          </div>
        </section>
      )}
      <div className="library-toolbar">
        <div className="search-field">
          <Icon name="search" size={17} />
          <input
            aria-label={t("搜索歌单")}
            placeholder={t("搜索名称、备注或 ID")}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <Select
          label={t("筛选平台")}
          value={filter}
          onChange={setFilter}
          options={[{ value: "", label: t("全部平台") }, ...platforms.map((p) => ({ value: p.id, label: t(p.label) }))]}
        />
        <Select label={t("歌单排序")} value={sort} onChange={(value) => setSort(value as LibrarySort)} options={[
          { value: "added", label: t("导入顺序") },
          { value: "name", label: t("名称排序") },
          { value: "updated", label: t("最近更新") },
        ]} />
        <span className="library-total">
          {filtered.length}
          {t("个来源")}
        </span>
      </div>
      {state.playlists.length > 0 && <div className="library-selection">
        <label><input type="checkbox" disabled={!!busy || !filtered.length} checked={filtered.length > 0 && filtered.every((entry) => selected.has(sourceKey(entry)))} onChange={selectVisible} />{t("选择当前列表")}</label>
        <span>{t("已选择 {p0} 个来源", { p0: selected.size })}</span>
        <div className="library-batch-actions">
          <button className="button compact secondary" disabled={!!busy || !selected.size} onClick={updateSelected}><Icon name="refresh" size={15} />{t("更新")}</button>
          <button className="button compact secondary delete-action" disabled={!!busy || !selected.size} onClick={removeSelected}><Icon name="remove" size={15} />{t("删除")}</button>
        </div>
      </div>}
      {progress && !importDialogOpen && <OperationProgress progress={progress} cancelling={cancelling} cancel={cancelOperation} summary={operationSummary} />}
      {startup ? (
        <div className="empty-state">
          <Icon name="refresh" className="spin" />
          <p>{t("正在读取本地歌单…")}</p>
        </div>
      ) : state.playlists.length === 0 ? (
        <div className="empty-state">
          <div className="empty-art">
            <Icon name="library" size={34} />
          </div>
          <h2>{t("从一个喜欢的歌单开始")}</h2>
          <p>
            {t("支持网易云、QQ 音乐、酷狗、酷我、汽水和 Spotify 的公开歌单与专辑，以及 YouTube 与 Bilibili 视频来源。")}
          </p>
          <button
            className="button primary"
            onClick={() => openImport()}
          >
            <Icon name="link" />
            {t("粘贴歌单链接")}
          </button>
          <button
            className="text-button"
            disabled={!!busy}
            onClick={migrate}
          >
            {t("迁移旧版数据")}
            <Icon name="arrow" size={15} />
          </button>
          <button
            className="text-button"
            disabled={!!busy}
            onClick={() =>
              importSample()
            }
          >
            {t("试用原版示例歌单")}
          </button>
        </div>
      ) : filtered.length === 0 ? (
        <div className="empty-state">
          <Icon name="search" size={32} />
          <h3>{t("没有匹配的歌单")}</h3>
          <button
            className="text-button"
            onClick={() => {
              setQuery("");
              setFilter("");
            }}
          >
            {t("清除筛选")}
          </button>
        </div>
      ) : (
        <div className="library-list">
          {filtered.map((p) => (
            <article
              className={`playlist-row ${p.enabled ? "enabled-row" : ""}`}
              data-selected={selected.has(sourceKey(p)) || undefined}
              key={sourceKey(p)}
            >
              <input className="source-checkbox" type="checkbox" aria-label={t("选择 {p0}", { p0: p.title })} checked={selected.has(sourceKey(p))} disabled={!!busy} onChange={() => toggleSelected(p)} />
              <Cover
                url={p.coverUrl}
                data={p.coverDataUri}
                prepared
                label={p.title}
              />
              <div className="playlist-details">
                <h3>
                  {p.title}
                  {p.enabled && (
                    <span className="enabled-badge">{t("已启用")}</span>
                  )}
                </h3>
                <div className="row-meta">
                  <PlatformTag id={p.platform} />
                  <span>{sourceKind(p.kind)}</span>
                  <span>
                    {p.songCount.toLocaleString()}
                    {itemCountUnit(p.platform)}
                  </span>
                  {p.updatedAt && <span>{formatDate(p.updatedAt)}</span>}
                </div>
                {p.remark && (
                  <p className="playlist-remark">{p.remark}</p>
                )}
                {p.cacheError && (
                  <p className="inline-error">
                    {errorText(p.cacheError)}
                  </p>
                )}
              </div>
              <div className="row-actions">
                <button
                  className="button compact secondary"
                  disabled={!!busy || p.enabled}
                  onClick={() =>
                    enablePlaylist(p)
                  }
                >
                  {p.enabled && <Icon name="check" size={15} />}
                  {p.enabled ? t("使用中") : t("启用")}
                </button>
                <button
                  className="icon-button"
                  aria-label={t("更新 {p0}", { p0: p.title })}
                  title={t("更新歌单")}
                  disabled={!!busy}
                  onClick={() =>
                    updatePlaylist(p)
                  }
                >
                  <Icon
                    name="refresh"
                    className={busy === sourceKey(p) ? "spin" : ""}
                  />
                </button>
                <button className="icon-button" title={t("编辑备注")} aria-label={t("编辑 {p0} 的备注", { p0: p.title })} disabled={!!busy} onClick={() => editRemark(p)}><Icon name="edit" size={17} /></button>
                <button
                  className="icon-button delete-action"
                  aria-label={t("删除 {p0}", { p0: p.title })}
                  title={t("删除歌单")}
                  disabled={!!busy}
                  onClick={() => confirmRemove(p)}
                >
                  <Icon name="remove" size={17} />
                </button>
              </div>
            </article>
          ))}
        </div>
      )}
      <div className="library-footnote">
        <Icon name="info" size={15} />
        <span>
          {t("同一时间能且只能启用一个歌单")}
        </span>
      </div>
    </>
  );
}
