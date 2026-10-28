import type { SourceSummary, LibraryPatch } from "../../types";

export type LibrarySort = "added" | "name" | "updated";

/** A playlist and an album with the same ID are different sources. */
export function sourceKey(entry: Pick<SourceSummary, "platform" | "sourceKind" | "sourceId">): string {
  return JSON.stringify([entry.platform, entry.sourceKind, entry.sourceId]);
}

/** Source settings own order and labels; unchanged summaries keep their prepared covers. */
export function mergeLibraryEntries(previous: SourceSummary[], patch: LibraryPatch): SourceSummary[] | null {
  const entries = new Map(previous.map(entry => [sourceKey(entry), entry]));
  patch.updatedSources.forEach(entry => entries.set(sourceKey(entry), entry));
  const merged: SourceSummary[] = [];
  for (const source of patch.settings.sources) {
    const entry = entries.get(sourceKey({ platform: source.platform, sourceKind: source.sourceKind, sourceId: source.sourceId }));
    if (!entry) return null;
    const title = source.title || source.sourceId;
    merged.push(entry.title === title && entry.remark === source.remark && entry.enabled === source.enabled && entry.updatedAt === source.updatedAt
      ? entry : { ...entry, title, remark: source.remark, enabled: source.enabled, updatedAt: source.updatedAt });
  }
  return merged;
}

export function filterLibrary(entries: SourceSummary[], query: string, platform: string, sort: LibrarySort, locale: string): SourceSummary[] {
  const text = query.trim().toLocaleLowerCase(locale);
  const visible = entries.filter((entry) => (!platform || entry.platform === platform) &&
    `${entry.title} ${entry.remark} ${entry.sourceId}`.toLocaleLowerCase(locale).includes(text));
  if (sort === "name") visible.sort((a, b) => a.title.localeCompare(b.title, locale));
  if (sort === "updated") visible.sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
  return visible;
}
