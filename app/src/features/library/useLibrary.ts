import { useEffect, useMemo, useRef, useState } from "react";
import type { LibraryPatch, LibraryEntry, LibraryProgress } from "../../types";
import type { AppStateController } from "../../hooks/useAppState";
import type { Notice } from "../../hooks/useNotice";
import { call, desktop, onDesktopEvent, listenFileDrop } from "../../services/desktop";
import { t, currentLocale, errorText, isCancelledError } from "../../i18n";
import { filterLibrary, sourceKey } from "./libraryModel";
import type { LibrarySort } from "./libraryModel";

/** Owns source editing and cancellable network operations, independently of discovery. */
export function useLibrary(app: AppStateController, notice: Notice) {
  const { state, busy, setBusy, applyLibraryPatch, action } = app;
  const [importing, setImporting] = useState(false);
  const [droppedPaths, setDroppedPaths] = useState<string[]>([]);
  const dropHandler = useRef<(paths: string[]) => void>(() => {});
  dropHandler.current = (paths) => {
    if (busy || operation.current) { notice(t("请等待当前操作完成后再添加音频。"), true); return; }
    setDroppedPaths(paths); setImporting(true);
  };
  useEffect(() => {
    let active = true, stop: (() => void) | undefined;
    void listenFileDrop((paths) => dropHandler.current(paths)).then((cleanup) => { if (active) stop = cleanup; else cleanup(); }).catch((error) => notice(errorText(error), true));
    return () => { active = false; stop?.(); };
  }, [notice]);
  const importDialogVisible = useRef(importing);
  importDialogVisible.current = importing;
  const [remove, setRemove] = useState<LibraryEntry[] | null>(null);
  const [editing, setEditing] = useState<LibraryEntry | null>(null);
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState("");
  const [sort, setSort] = useState<LibrarySort>("added");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [progress, setProgress] = useState<LibraryProgress | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const [operationSummary, setOperationSummary] = useState("");
  const operation = useRef<{ requestId: string; stopped: boolean } | null>(null);
  const retryUpdate = useRef<(entries: LibraryEntry[]) => void>(() => {});
  const locale = currentLocale();
  const filtered = useMemo(() => filterLibrary(state.playlists, query, filter, sort, locale), [state.playlists, query, filter, sort, locale]);
  useEffect(() => {
    const present = new Set(state.playlists.map(sourceKey));
    setSelected((current) => new Set([...current].filter((key) => present.has(key))));
  }, [state.playlists]);
  useEffect(() => {
    if (!desktop) return;
    let active = true;
    let removeListener: (() => void) | undefined;
    onDesktopEvent("library-progress", (payload) => {
      if (payload.requestId === operation.current?.requestId) setProgress(payload);
    }).then((remove) => { if (active) removeListener = remove; else remove(); })
      .catch((error) => notice(errorText(error), true));
    return () => { active = false; removeListener?.(); };
  }, [notice]);
  const startOperation = (key: string) => {
    if (busy || app.startup || operation.current) return null;
    const task = { requestId: crypto.randomUUID(), stopped: false };
    operation.current = task;
    setBusy(key);
    setCancelling(false);
    setProgress({ requestId: task.requestId, phase: "fetching", completed: 0, total: null });
    return task;
  };
  const finishOperation = () => {
    operation.current = null;
    setProgress(null);
    setOperationSummary("");
    setCancelling(false);
    setBusy("");
  };
  const cancelOperation = async () => {
    const task = operation.current;
    if (!task || task.stopped || progress?.phase === "saving" || progress?.phase === "done") return;
    task.stopped = true;
    setCancelling(true);
    try {
      const cancelled = await call<boolean>("cancel_library_operation", { requestId: task.requestId });
      if (!cancelled && operation.current === task) {
        task.stopped = false;
        setCancelling(false);
      }
    }
    catch (error) {
      task.stopped = false;
      setCancelling(false);
      notice(errorText(error), true);
    }
  };
  const importPlaylist = async (data: Record<string, unknown>) => {
    const task = startOperation("import");
    if (!task) return;
    try {
      await applyLibraryPatch(await call<LibraryPatch>(data.platform === "LocalMusic" ? "import_local_audio" : "import_playlist", { ...data, requestId: task.requestId }));
      setImporting(false);
      setDroppedPaths([]);
      notice(t("歌单已导入。"));
    } catch (error) {
      if (isCancelledError(error)) notice(t("已取消"));
      else if (!importDialogVisible.current) notice(errorText(error), true);
      else throw error;
    } finally { finishOperation(); }
  };
  const updateEntries = async (entries: LibraryEntry[], key: string) => {
    if (!entries.length) return;
    const task = startOperation(key);
    if (!task) return;
    let completed = 0;
    let failed = 0;
    let lastError: unknown;
    const failedEntries: LibraryEntry[] = [];
    try {
      for (const entry of entries) {
        if (task.stopped) break;
        // Each import has its own token, so late progress from another source is ignored.
        task.requestId = crypto.randomUUID();
        setProgress({ requestId: task.requestId, phase: "fetching", completed: 0, total: null });
        setOperationSummary(entries.length > 1 ? t("更新 {p0} / {p1}", { p0: completed + failed + 1, p1: entries.length }) : "");
        try {
          await applyLibraryPatch(await call<LibraryPatch>("import_playlist", {
            platform: entry.platform, typename: entry.kind, source: entry.id,
            remark: entry.remark, requestId: task.requestId,
          }));
          completed++;
        } catch (error) {
          if (isCancelledError(error)) { task.stopped = true; break; }
          failed++;
          failedEntries.push(entry);
          lastError = error;
        }
      }
      if (task.stopped) notice(t("已取消，已完成 {p0} 个卡池", { p0: completed }));
      else if (entries.length === 1) notice(failed ? errorText(lastError) : t("歌单已更新。"), failed > 0,
        failed ? { label: t("重试"), run: () => retryUpdate.current(failedEntries) } : undefined);
      else notice(`${t("更新完成：{p0} 个成功，{p1} 个失败", { p0: completed, p1: failed })}${failed ? ` · ${errorText(lastError)}` : ""}`, failed > 0,
        failed ? { label: t("重试失败项"), run: () => retryUpdate.current(failedEntries) } : undefined);
    } finally { finishOperation(); }
  };
  retryUpdate.current = entries => {
    const keys = new Set(entries.map(sourceKey));
    void updateEntries(state.playlists.filter(entry => keys.has(sourceKey(entry))), "bulk-update");
  };
  const enablePlaylist = (entry: LibraryEntry) => action(sourceKey(entry), "enable_playlist", {
    platform: entry.platform, id: entry.id, kind: entry.kind,
  }, t("已切换发现卡池。"));
  const updatePlaylist = (entry: LibraryEntry) => updateEntries([entry], sourceKey(entry));
  const updateSelected = () => updateEntries(state.playlists.filter((entry) => selected.has(sourceKey(entry))), "bulk-update");
  const importSample = async () => {
    const task = startOperation("sample");
    if (!task) return;
    try {
      await applyLibraryPatch(await call<LibraryPatch>("import_playlist", {
        platform: "NeteaseCloudMusic", typename: "playlist", source: "8285082830", remark: "", requestId: task.requestId,
      }));
      notice(t("示例歌单已导入。"));
    } catch (error) { notice(isCancelledError(error) ? t("已取消") : errorText(error), !isCancelledError(error)); }
    finally { finishOperation(); }
  };
  const removePlaylist = async () => {
    if (!remove?.length) return;
    const removedKeys = new Set(remove.map(sourceKey));
    const success = await action("remove", "remove_playlists", {
      sources: remove.map(({ platform, id, kind }) => ({ platform, id, kind })),
    }, t("已删除 {p0} 个卡池。", { p0: remove.length }));
    if (success) {
      setSelected((current) => new Set([...current].filter((key) => !removedKeys.has(key))));
      setRemove(null);
    }
  };
  const removeSelected = () => {
    if (busy) return;
    const entries = state.playlists.filter((entry) => selected.has(sourceKey(entry)));
    if (entries.length) setRemove(entries);
  };
  const saveRemark = async (remark: string) => {
    if (!editing || busy) return;
    setBusy("remark");
    try {
      await applyLibraryPatch(await call<LibraryPatch>("edit_playlist_remark", {
        platform: editing.platform, id: editing.id, kind: editing.kind, remark,
      }));
      setEditing(null);
      notice(t("备注已保存。"));
    } finally { setBusy(""); }
  };
  const toggleSelected = (entry: LibraryEntry) => setSelected((current) => {
    const next = new Set(current);
    const key = sourceKey(entry);
    if (next.has(key)) next.delete(key); else next.add(key);
    return next;
  });
  const selectVisible = () => setSelected((current) => {
    const next = new Set(current);
    const allSelected = filtered.every((entry) => current.has(sourceKey(entry)));
    for (const entry of filtered) {
      if (allSelected) next.delete(sourceKey(entry)); else next.add(sourceKey(entry));
    }
    return next;
  });
  return {
    query, setQuery, filter, setFilter, sort, setSort, filtered, selected, toggleSelected, selectVisible,
    importing, droppedPaths, remove, editing, progress, cancelling, operationSummary, cancelOperation,
    openImport: () => { setDroppedPaths([]); setImporting(true); }, closeImport: () => { setDroppedPaths([]); setImporting(false); },
    confirmRemove: (entry: LibraryEntry) => setRemove([entry]), closeRemove: () => setRemove(null), importPlaylist,
    editRemark: setEditing, closeRemark: () => setEditing(null), saveRemark,
    enablePlaylist, updatePlaylist, updateSelected, removeSelected, importSample, removePlaylist,
  };
}
