import type { HistoryEntry } from "../../types";

export type HistoryFilter = "all" | "discovered" | "selected";
export const HISTORY_PAGE_SIZE = 25;
const HISTORY_COVER_CACHE_SIZE = 100;
const HISTORY_COVER_CACHE_CHARACTERS = 16 * 1024 * 1024;
export const historyKey = (entry: Pick<HistoryEntry, "platform" | "songId">) => JSON.stringify([entry.platform, entry.songId]);
export const historyIdentity = (entry: Pick<HistoryEntry, "platform" | "songId">) => ({ platform: entry.platform, songId: entry.songId });

export function historySearchText(entry: HistoryEntry, labels: Record<string, string> = {}) {
  return [entry.name, ...entry.artistNames, entry.songId, entry.platform, labels[entry.platform] || ""].join(" ").toLocaleLowerCase();
}

export function filterHistory(entries: HistoryEntry[], query: string, filter: HistoryFilter, labels: Record<string, string> = {}, searchIndex?: ReadonlyMap<HistoryEntry, string>) {
  const terms = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  return entries.filter((entry) => {
    if (filter === "selected" && entry.selectedAt === null) return false;
    if (filter === "discovered" && entry.discoveredAt === null) return false;
    if (!terms.length) return true;
    const haystack = searchIndex?.get(entry) ?? historySearchText(entry, labels);
    return terms.every((term) => haystack.includes(term));
  });
}

export function historyPage(entries: HistoryEntry[], page: number) {
  const pageCount = Math.max(1, Math.ceil(entries.length / HISTORY_PAGE_SIZE));
  const currentPage = Math.min(Math.max(0, Math.floor(page)), pageCount - 1);
  return { pageCount, currentPage, rows: entries.slice(currentPage * HISTORY_PAGE_SIZE, (currentPage + 1) * HISTORY_PAGE_SIZE) };
}

export function needsHistoryMetadata(entry: HistoryEntry) {
  return !entry.name || /^[?？]+$/.test(entry.name) || entry.name === "神秘歌曲"
    || entry.artistNames.some((name) => /^[?？]+$/.test(name));
}

export function retainedHistorySelection(entries: HistoryEntry[], selected: Set<string>) {
  const existing = new Set(entries.map(historyKey));
  return new Set([...selected].filter((key) => existing.has(key)));
}

export function historyCoverKey(entry: HistoryEntry) {
  return JSON.stringify([entry.platform, entry.songId, entry.coverKey || entry.coverUrl || ""]);
}

/** Keep recent artwork bounded while preserving every cover on the displayed page. */
export function cacheHistoryCovers(previous: Map<string, string | null>, incoming: Map<string, string | null>, visible: Set<string>) {
  const next = new Map(previous);
  for (const [key, cover] of incoming) { next.delete(key); next.set(key, cover); }
  for (const key of visible) {
    if (!next.has(key)) continue;
    const cover = next.get(key)!;
    next.delete(key); next.set(key, cover);
  }
  let characters = [...next.values()].reduce((total, cover) => total + (cover?.length || 0), 0);
  for (const [key, cover] of next) {
    if (next.size <= HISTORY_COVER_CACHE_SIZE && characters <= HISTORY_COVER_CACHE_CHARACTERS) break;
    if (visible.has(key)) continue;
    next.delete(key); characters -= cover?.length || 0;
  }
  return next;
}
