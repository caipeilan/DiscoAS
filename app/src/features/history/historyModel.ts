import type { HistoryEntry, HistoryCover, HistoryIdentity } from "../../types";

export type HistoryFilter = "all" | "discovered" | "selected";
export interface HistoryCoverPage {
  items: Array<HistoryIdentity & { imageIndex: number | null }>;
  images: string[];
}
export function expandHistoryCoverPage(page: HistoryCoverPage): HistoryCover[] {
  return page.items.map(item => ({ platform: item.platform, songId: item.songId,
    coverDataUri: item.imageIndex === null ? null : page.images[item.imageIndex] ?? null }));
}
export class HistoryReadCache {
  revision: number | undefined;
  private entries: HistoryEntry[] = [];
  accept(snapshot: { revision: number; entries: HistoryEntry[] | null }): HistoryEntry[] {
    if (this.revision === undefined || snapshot.revision >= this.revision) {
      this.revision = snapshot.revision;
      if (snapshot.entries !== null) this.entries = snapshot.entries;
    }
    return this.entries;
  }
}

export const HISTORY_PAGE_SIZE = 25;
const HISTORY_COVER_CACHE_SIZE = 100;
const HISTORY_COVER_CACHE_CHARACTERS = 16 * 1024 * 1024;
export const historyKey = (entry: Pick<HistoryEntry, "platform" | "songId">) => JSON.stringify([entry.platform, entry.songId]);
export const historyIdentity = (entry: Pick<HistoryEntry, "platform" | "songId">) => ({ platform: entry.platform, songId: entry.songId });

export function historySearchText(entry: HistoryEntry, labels: Record<string, string> = {}) {
  return [entry.title, ...entry.artistNames, entry.songId, entry.platform, labels[entry.platform] || ""].join(" ").toLocaleLowerCase();
}

let indexedEntries: HistoryEntry[] | undefined;
let indexedLabels = "";
let retainedIndex = new Map<HistoryEntry, string>();
export function historySearchIndex(entries: HistoryEntry[], labels: Record<string, string>) {
  if (!entries.length) return new Map<HistoryEntry, string>();
  const key = JSON.stringify(labels);
  if (entries !== indexedEntries || key !== indexedLabels) {
    retainedIndex = new Map(entries.map(entry => [entry, historySearchText(entry, labels)]));
    indexedEntries = entries; indexedLabels = key;
  }
  return retainedIndex;
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
  return !entry.title || /^[?？]+$/.test(entry.title) || entry.title === "神秘歌曲"
    || entry.artistNames.some((name) => /^[?？]+$/.test(name));
}

export function retainedHistorySelection(entries: HistoryEntry[], selected: Set<string>) {
  const existing = new Set(entries.map(historyKey));
  return new Set([...selected].filter((key) => existing.has(key)));
}

export function historyCoverKey(entry: HistoryEntry) {
  return JSON.stringify([entry.platform, entry.songId, entry.coverKey || "", entry.coverUrl || ""]);
}

/** Keep recent artwork bounded while preserving every cover on the displayed page. */
export function cacheHistoryCovers(previous: Map<string, string | null>, incoming: Map<string, string | null>, visible: Set<string>, limit = HISTORY_COVER_CACHE_CHARACTERS) {
  const next = new Map(previous);
  for (const [key, cover] of incoming) { next.delete(key); next.set(key, cover); }
  for (const key of visible) {
    if (!next.has(key)) continue;
    const cover = next.get(key)!;
    next.delete(key); next.set(key, cover);
  }
  let characters = [...next.values()].reduce((total, cover) => total + (cover?.length || 0), 0);
  for (const [key, cover] of next) {
    if (next.size <= HISTORY_COVER_CACHE_SIZE && characters <= limit) break;
    if (visible.has(key)) continue;
    next.delete(key); characters -= cover?.length || 0;
  }
  return next;
}

/** One bounded artwork cache survives history-page unmounts and shares in-flight reads. */
export class HistoryArtworkCache {
  private covers = new Map<string, string | null>();
  private allowed = new Set<string>();
  private pending = new Map<string, Promise<HistoryCover[]>>();
  private epoch = 0;
  private limit = HISTORY_COVER_CACHE_CHARACTERS;
  snapshot() { return new Map(this.covers); }
  retain(keys: Set<string>) {
    if (!keys.size) { indexedEntries = undefined; retainedIndex = new Map(); }
    this.allowed = keys;
    this.covers = new Map([...this.covers].filter(([key]) => keys.has(key)));
  }
  configure(memoryMb: number) {
    this.limit = memoryMb * 1024 * 1024 / 4;
    this.covers = cacheHistoryCovers(this.covers, new Map(), new Set(), this.limit);
  }
  invalidateRequests() { this.epoch++; this.pending.clear(); }
  clear() { this.invalidateRequests(); this.covers.clear(); this.allowed.clear(); indexedEntries = undefined; retainedIndex = new Map(); }
  load(rows: HistoryEntry[], read: (identities: HistoryIdentity[]) => Promise<HistoryCover[]>) {
    const key = rows.map(historyCoverKey).join("\n");
    const existing = this.pending.get(key);
    if (existing) return existing;
    const epoch = this.epoch;
    const task = read(rows.map(historyIdentity)).then(results => {
      if (epoch !== this.epoch) return results;
      const resolved = new Map(results.map(result => [historyKey(result), result.coverDataUri]));
      const incoming = new Map(rows.filter(row => this.allowed.has(historyCoverKey(row)) && resolved.get(historyKey(row)))
        .map(row => [historyCoverKey(row), resolved.get(historyKey(row))!]));
      this.covers = cacheHistoryCovers(this.covers, incoming, new Set(), this.limit);
      return results;
    }).finally(() => { if (this.pending.get(key) === task) this.pending.delete(key); });
    this.pending.set(key, task);
    return task;
  }
}
export const historyArtworkCache = new HistoryArtworkCache();
