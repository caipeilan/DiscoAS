/** Existing Tauri payloads keep their field names; application models use canonical names. */
import desktopFields from "./desktopFields.json";
const fieldNames: Record<string, string> = desktopFields.fields;
const sourceNames: Record<string, string> = desktopFields.sources;

const invert = (names: Record<string, string>) => Object.fromEntries(Object.entries(names).map(([wire, model]) => [model, wire]));
const encodedNames = invert(fieldNames), encodedSources = invert(sourceNames);

function mapValue(value: unknown, encode: boolean): unknown {
  if (Array.isArray(value)) return value.map(item => mapValue(item, encode));
  if (value === null || typeof value !== "object") return value;
  const object = value as Record<string, unknown>;
  const source = encode
    ? "sourceId" in object && "sourceKind" in object && "enabled" in object && !("songId" in object) && !("songCount" in object)
    : "playlist_album_id" in object && "typename" in object && "name" in object;
  const sourceIdentity = !source && (encode
    ? "sourceId" in object && "sourceKind" in object && !("songId" in object)
    : "platform" in object && "id" in object && "kind" in object);
  const song = "songId" in object;
  const autoState = "token" in object && "busy" in object;
  const music = "normalSongCount" in object;
  const names = source ? (encode ? encodedSources : sourceNames) : encode ? encodedNames : fieldNames;
  return Object.fromEntries(Object.entries(object).map(([key, child]) => {
    let mapped = names[key] ?? key;
    if (!source && encode && key === "sources") mapped = music ? "playlist_albums" : "version" in object && "settings" in object ? "playlists" : key;
    if (!encode && key === "playlists") mapped = "sources";
    if (sourceIdentity && key === (encode ? "sourceId" : "id")) mapped = encode ? "id" : "sourceId";
    if (sourceIdentity && key === (encode ? "sourceKind" : "kind")) mapped = encode ? "kind" : "sourceKind";
    if (song && key === (encode ? "title" : "name")) mapped = encode ? "name" : "title";
    if (song && key === (encode ? "sourceId" : "playlistId")) mapped = encode ? "playlistId" : "sourceId";
    if (autoState && key === (encode ? "active" : "enabled")) mapped = encode ? "enabled" : "active";
    // Position/overlap belong to hand settings; window geometry uses its own coordinates.
    if (!("edgeOffsetPx" in object || "edge_distance" in object) && (key === "position" || key === "overlap")) mapped = key;
    return [mapped, mapValue(child, encode)];
  }));
}

export function decodeDesktopValue<T>(value: unknown): T {
  return mapValue(value, false) as T;
}

export function encodeDesktopArgs(command: string, args?: Record<string, unknown>): Record<string, unknown> | undefined {
  if (!args) return undefined;
  const encoded = mapValue(args, true) as Record<string, unknown>;
  if (command === "import_playlist" && "sourceInput" in encoded) {
    encoded.source = encoded.sourceInput;
    delete encoded.sourceInput;
  }
  return encoded;
}
