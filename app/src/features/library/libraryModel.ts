import type { LibraryEntry } from "../../types";

export type LibrarySort = "added" | "name" | "updated";

/** A playlist and an album with the same ID are different sources. */
export function sourceKey(entry: Pick<LibraryEntry, "platform" | "kind" | "id">): string {
  return JSON.stringify([entry.platform, entry.kind, entry.id]);
}

export function filterLibrary(entries: LibraryEntry[], query: string, platform: string, sort: LibrarySort, locale: string): LibraryEntry[] {
  const text = query.trim().toLocaleLowerCase(locale);
  const visible = entries.filter((entry) => (!platform || entry.platform === platform) &&
    `${entry.title} ${entry.remark} ${entry.id}`.toLocaleLowerCase(locale).includes(text));
  if (sort === "name") visible.sort((a, b) => a.title.localeCompare(b.title, locale));
  if (sort === "updated") visible.sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
  return visible;
}
