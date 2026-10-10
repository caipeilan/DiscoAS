import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { HistoryEntry } from "../../types";
import { platforms } from "../../types";
import { call, getHistoryCovers, mutateDiscoveryHistory, onDesktopEvent } from "../../services/desktop";
import { currentLocale, errorText, t } from "../../i18n";
import { cacheHistoryCovers, filterHistory, historySearchText, historyCoverKey, historyIdentity, historyKey, historyPage, needsHistoryMetadata,
  retainedHistorySelection, type HistoryFilter } from "./historyModel";

type Identity = { platform: string; songId: string };
type Confirmation = { kind: "clear" } | { kind: "delete"; identities: Identity[] };
export type BatchAction = "discovered-on" | "discovered-off" | "selected-on" | "selected-off" | "delete";

/** Loads a page of covers while preserving snapshot isolation for edits and old metadata repair. */
export function useHistory() {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [filter, setFilter] = useState<HistoryFilter>("all");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(new Set<string>());
  const [batchAction, setBatchAction] = useState<BatchAction>("discovered-on");
  const [loading, setLoading] = useState(true);
  const [repairing, setRepairing] = useState(false);
  const [mutating, setMutating] = useState(false);
  const [confirming, setConfirming] = useState<Confirmation | null>(null);
  const [error, setError] = useState("");
  const [page, setPage] = useState(0);
  const [covers, setCovers] = useState<Map<string, string | null>>(new Map());
  const [snapshotVersion, setSnapshotVersion] = useState(0);
  const revision = useRef(0);
  const mounted = useRef(true);
  const busy = useRef(false);
  const refreshPending = useRef(false);
  const repairAttempted = useRef(false);
  const request = useRef(0);
  const changeSource = useRef(crypto.randomUUID());
  const selectAll = useRef<HTMLInputElement>(null);
  const locale = currentLocale();
  const labels = useMemo(() => Object.fromEntries(platforms.map((platform) => [platform.id, t(platform.label)])), [locale]);
  const searchIndex = useMemo(() => new Map(entries.map((entry) => [entry, historySearchText(entry, labels)])), [entries, labels]);
  const visible = useMemo(() => filterHistory(entries, query, filter, labels, searchIndex), [entries, query, filter, labels, searchIndex]);
  const { pageCount, currentPage, rows } = useMemo(() => historyPage(visible, page), [visible, page]);
  const selectedEntries = useMemo(() => selected.size ? entries.filter((entry) => selected.has(historyKey(entry))) : [], [entries, selected]);
  const visibleSelectedCount = useMemo(() => selected.size ? visible.filter((entry) => selected.has(historyKey(entry))).length : 0, [visible, selected]);
  const allVisibleSelected = visible.length > 0 && visibleSelectedCount === visible.length;

  const applyRecords = useCallback((records: HistoryEntry[]) => {
    setEntries(records);
    setSelected((previous) => retainedHistorySelection(records, previous));
    const keys = new Set(records.map(historyCoverKey));
    setCovers((previous) => new Map([...previous].filter(([key]) => keys.has(key))));
    // Even unchanged rows need a new cover effect after an older request is invalidated.
    setSnapshotVersion((previous) => previous + 1);
  }, []);
  const refresh: () => Promise<void> = useCallback(async () => {
    if (!mounted.current) return;
    // An event from this page's own edit may arrive before its command response.
    // Read the latest snapshot only once that edit has finished committing.
    if (busy.current) { refreshPending.current = true; return; }
    const stamp = ++revision.current;
    request.current++;
    try {
      const records = await call<HistoryEntry[]>("get_discovery_history");
      if (!mounted.current || stamp !== revision.current) return;
      applyRecords(records);
      setLoading(false);
      if (repairAttempted.current || !records.some(needsHistoryMetadata)) return;
      repairAttempted.current = true;
      setRepairing(true);
      try {
        const repaired = await call<HistoryEntry[]>("repair_discovery_history_metadata");
        if (mounted.current && stamp === revision.current) applyRecords(repaired);
        else if (mounted.current) void refresh();
      } finally {
        if (mounted.current) setRepairing(false);
      }
    } catch (failure) {
      if (mounted.current && stamp === revision.current) setError(errorText(failure));
    } finally {
      if (mounted.current && stamp === revision.current) setLoading(false);
    }
  }, [applyRecords]);

  useEffect(() => {
    mounted.current = true;
    let active = true;
    let unlisten: (() => void) | undefined;
    // Subscribe before reading, so writes during the initial request cannot be missed.
    onDesktopEvent("discovery-history-changed", (change) => {
      if (active && change?.source !== changeSource.current) void refresh();
    }).then((remove) => {
      if (!active) { remove(); return; }
      unlisten = remove;
      void refresh();
    }).catch(() => { if (active) void refresh(); });
    return () => { active = false; mounted.current = false; unlisten?.(); revision.current++; request.current++; };
  }, [applyRecords, refresh]);
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented || busy.current || !confirming) return;
      event.preventDefault(); setConfirming(null);
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [confirming]);
  useEffect(() => {
    if (selectAll.current) selectAll.current.indeterminate = visibleSelectedCount > 0 && !allVisibleSelected;
  }, [visibleSelectedCount, allVisibleSelected]);
  // Fetch only the current page. Old responses cannot restore deleted rows or overwrite a new snapshot.
  const pageCoverSignature = rows.map(historyCoverKey).join("\n");
  useEffect(() => {
    const visibleKeys = new Set(rows.map(historyCoverKey));
    setCovers((previous) => cacheHistoryCovers(previous, new Map(), visibleKeys));
    const uncached = rows.filter((entry) => !covers.has(historyCoverKey(entry)));
    if (!uncached.length || loading || mutating) return;
    const stamp = revision.current;
    const current = ++request.current;
    let active = true;
    getHistoryCovers(uncached.map(historyIdentity)).then((results) => {
      if (!active || !mounted.current || stamp !== revision.current || current !== request.current) return;
      const resolved = new Map(results.map((result) => [historyKey(result), result.coverDataUri]));
      setCovers((previous) => {
        const incoming = new Map(uncached.map((entry) => [historyCoverKey(entry), resolved.get(historyKey(entry)) ?? null]));
        return cacheHistoryCovers(previous, incoming, visibleKeys);
      });
    }).catch((failure) => {
      if (active && mounted.current && stamp === revision.current && current === request.current) setError(errorText(failure));
    });
    return () => { active = false; };
  }, [pageCoverSignature, loading, mutating, snapshotVersion]);

  const mutate = async (identities: Identity[], action: "delete" | "discovered" | "selected", value?: boolean) => {
    if (busy.current || !identities.length) return;
    busy.current = true;
    const stamp = ++revision.current;
    request.current++;
    setMutating(true); setRepairing(false); setError("");
    try {
      const records = await mutateDiscoveryHistory({ identities, action, value }, changeSource.current);
      if (!mounted.current || stamp !== revision.current) return;
      applyRecords(records);
      setConfirming(null);
    } catch (failure) {
      if (mounted.current && stamp === revision.current) setError(errorText(failure));
      refreshPending.current = true;
    } finally {
      busy.current = false;
      if (mounted.current && stamp === revision.current) setMutating(false);
      if (refreshPending.current) { refreshPending.current = false; void refresh(); }
    }
  };
  const clear = async () => {
    if (busy.current) return;
    busy.current = true;
    const stamp = ++revision.current;
    request.current++;
    setMutating(true); setRepairing(false); setError("");
    try {
      await call("clear_discovery_history", { source: changeSource.current });
      if (!mounted.current || stamp !== revision.current) return;
      setEntries([]); setSelected(new Set()); setCovers(new Map()); setPage(0); setConfirming(null);
    } catch (failure) {
      if (mounted.current && stamp === revision.current) setError(errorText(failure));
      refreshPending.current = true;
    } finally {
      busy.current = false;
      if (mounted.current && stamp === revision.current) setMutating(false);
      if (refreshPending.current) { refreshPending.current = false; void refresh(); }
    }
  };
  const changeFilter = (value: HistoryFilter) => { setFilter(value); setPage(0); setSelected(new Set()); };
  const changeQuery = (value: string) => { setQuery(value); setPage(0); setSelected(new Set()); };
  const toggleSelected = (entry: HistoryEntry, value: boolean) => setSelected((previous) => {
    const next = new Set(previous); if (value) next.add(historyKey(entry)); else next.delete(historyKey(entry)); return next;
  });
  const selectVisible = (value: boolean) => setSelected(value ? new Set(visible.map(historyKey)) : new Set());
  const runBatch = () => {
    const identities = selectedEntries.map(historyIdentity);
    if (batchAction === "delete") setConfirming({ kind: "delete", identities });
    else mutate(identities, batchAction.startsWith("selected") ? "selected" : "discovered", batchAction.endsWith("on"));
  };
  const confirmMutation = () => {
    if (confirming?.kind === "clear") return clear();
    if (confirming?.kind === "delete") return mutate(confirming.identities, "delete");
  };

  return { entries, filter, query, selected, batchAction, setBatchAction, loading, repairing, mutating, confirming,
    setConfirming, error, covers, selectAll, visible, allVisibleSelected, selectedCount: selectedEntries.length,
    pageCount, currentPage, rows, setPage, changeFilter, changeQuery, toggleSelected, selectVisible, runBatch,
    mutate, confirmMutation,
    failCover: (entry: HistoryEntry) => setCovers((previous) => new Map(previous).set(historyCoverKey(entry), null)),
  };
}
