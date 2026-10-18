import type { LibraryEntry, LibraryPatch } from "../../types";

export type LibrarySort = "added" | "name" | "updated";

/** A playlist and an album with the same ID are different sources. */
export function sourceKey(entry: Pick<LibraryEntry, "platform" | "kind" | "id">): string {
  return JSON.stringify([entry.platform, entry.kind, entry.id]);
}

/** Source settings own order and labels; unchanged summaries keep their prepared covers. */
export function mergeLibraryEntries(previous: LibraryEntry[], patch: LibraryPatch): LibraryEntry[] | null {
  const entries = new Map(previous.map(entry => [sourceKey(entry), entry]));
  patch.updatedPlaylists.forEach(entry => entries.set(sourceKey(entry), entry));
  const merged: LibraryEntry[] = [];
  for (const source of patch.settings.playlist_albums) {
    const entry = entries.get(sourceKey({ platform: source.name, kind: source.typename, id: source.playlist_album_id }));
    if (!entry) return null;
    const title = source.playlist_album_name || source.playlist_album_id;
    merged.push(entry.title === title && entry.remark === source.playlist_album_remark && entry.enabled === source.enabled && entry.updatedAt === source.update_time
      ? entry : { ...entry, title, remark: source.playlist_album_remark, enabled: source.enabled, updatedAt: source.update_time });
  }
  return merged;
}

export function filterLibrary(entries: LibraryEntry[], query: string, platform: string, sort: LibrarySort, locale: string): LibraryEntry[] {
  const text = query.trim().toLocaleLowerCase(locale);
  const visible = entries.filter((entry) => (!platform || entry.platform === platform) &&
    `${entry.title} ${entry.remark} ${entry.id}`.toLocaleLowerCase(locale).includes(text));
  if (sort === "name") visible.sort((a, b) => a.title.localeCompare(b.title, locale));
  if (sort === "updated") visible.sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
  return visible;
}
