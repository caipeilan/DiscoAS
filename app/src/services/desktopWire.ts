/** Existing Tauri payloads keep their field names; application models use canonical names. */
const fieldNames: Record<string, string> = {
  "number_of_discovered_songs": "normalSongCount",
  "have_mystery_song": "mysterySongsEnabled",
  "num_of_mystery_song": "mysterySongCount",
  "mystery_song_cover": "mysteryCover",
  "cache_batches": "preloadBatchCount",
  "preload_deduplication": "deduplicatePreloadedBatches",
  "discovery_weighting": "discoveryWeighting",
  "weighting_mode": "weightingMode",
  "ranked_weighting": "rankedWeighting",
  "replacement_limit": "replacementLimit",
  "refreshing_after_cancel": "refreshOnCancel",
  "shortcut_key": "discoveryShortcut",
  "history_exclusion": "recentExclusionMode",
  "history_limit": "recentExclusionLimit",
  "discovery_keybindings": "discoveryKeybindings",
  "hand_discovery": "handDiscovery",
  "playlist_albums": "sources",
  "reveal_mystery": "revealMystery",
  "keep_discovery_open": "keepDiscoveryOpen",
  "auto_play": "autoPlayEnabled",
  "create_cards": "createCards",
  "auto_play_position": "autoPlayPosition",
  "auto_play_detection": "autoPlayDetection",
  "play_shortcuts": "playShortcuts",
  "edge_distance": "edgeOffsetPx",
  "text_outline_width": "textOutlineWidth",
  "liquid_strength": "liquidStrength",
  "liquid_mode": "liquidMode",
  "base_weight": "baseWeight",
  "discovered_penalty": "discoveredPenalty",
  "selected_penalty": "selectedPenalty",
  "recovery_batches": "recoveryBatches",
  "boost_after_batches": "boostAfterBatches",
  "boost_per_batch": "boostPerBatch",
  "max_weight": "maxWeight",
  "soft_band": "softBand",
  "time_cooling": "timeCooling",
  "time_cooling_strength": "timeCoolingStrength",
  "time_cooling_batches": "timeCoolingBatches",
  "revival_gain": "revivalGain",
  "revival_wait": "revivalWait",
  "revival_span": "revivalSpan",
  "launch_at_login": "launchAtLogin",
  "start_hidden": "startHidden",
  "update_on_startup": "updateOnStartup",
  "pause_before_playback": "pauseBeforePlayback",
  "overlay_monitor": "overlayMonitor",
  "minimize_after_playback": "minimizeAfterPlayback",
  "minimize_delay_seconds": "temporaryHideSeconds",
  "client_window_mode": "clientWindowMode",
  "client_separate_desktop": "clientSeparateDesktop",
  "spotify_playback_mode": "spotifyPlaybackMode",
  "spotify_defaults_version": "spotifyDefaultsVersion",
  "browser_playback_mode": "browserPlaybackMode",
  "rendering_mode": "renderingMode",
  "background_hover": "backgroundHover",
  "font_color": "fontColor",
  "night_mode": "nightMode",
  "follow_system_theme": "followSystemTheme",
  "card_size": "discoveryCardScale",
  "cancel_button_size": "cancelButtonScale",
  "replacement_button_size": "replacementButtonScale",
  "discovery_bar_size": "discoveryBarScale",
  "setting_size": "mainWindowScale",
  "font_family": "fontFamily",
  "font_size": "fontSize",
  "user_configured": "userConfigured",
  "cancel_button": "cancelButton",
  "card_night_mode": "cardNightMode",
  "cancel_button_night_mode": "cancelButtonNightMode",
  "setting_night_mode": "settingNightMode",
  "typename": "sourceKind",
  "tilt": "tiltDegrees",
  "updatedPlaylists": "updatedSources",
  "position": "positionPercent",
  "overlap": "overlapPercent"
};

const sourceNames: Record<string, string> = {
  name: "platform", playlist_album_id: "sourceId", typename: "sourceKind",
  playlist_album_name: "title", playlist_album_remark: "remark", update_time: "updatedAt",
};
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
