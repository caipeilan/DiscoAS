//! The existing desktop/disk names and public console names share one mapping.
use serde_json::{Map, Value};
use std::{collections::HashMap, sync::OnceLock};

struct Names {
    fields: HashMap<String, String>,
    sources: HashMap<String, String>,
    encoded_fields: HashMap<String, String>,
    encoded_sources: HashMap<String, String>,
}
fn names() -> &'static Names {
    static NAMES: OnceLock<Names> = OnceLock::new();
    NAMES.get_or_init(|| {
        let value: Value =
            serde_json::from_str(include_str!("../../../../src/services/desktopFields.json"))
                .expect("bundled desktop field names");
        let read = |key| {
            value[key]
                .as_object()
                .unwrap()
                .iter()
                .map(|(key, value)| (key.clone(), value.as_str().unwrap().to_string()))
                .collect::<HashMap<_, _>>()
        };
        let fields = read("fields");
        let sources = read("sources");
        let invert = |map: &HashMap<String, String>| {
            map.iter()
                .map(|(key, value)| (value.clone(), key.clone()))
                .collect()
        };
        Names {
            encoded_fields: invert(&fields),
            encoded_sources: invert(&sources),
            fields,
            sources,
        }
    })
}
pub fn decode(value: Value) -> Value {
    map(value, false)
}
pub fn encode(value: Value) -> Value {
    map(value, true)
}
fn map(value: Value, encode: bool) -> Value {
    match value {
        Value::Array(values) => {
            Value::Array(values.into_iter().map(|value| map(value, encode)).collect())
        }
        Value::Object(values) => {
            let has = |key| values.contains_key(key);
            let source = if encode {
                has("sourceId")
                    && has("sourceKind")
                    && has("enabled")
                    && !has("songId")
                    && !has("songCount")
            } else {
                has("playlist_album_id") && has("typename") && has("name")
            };
            let identity = !source
                && if encode {
                    has("sourceId") && has("sourceKind") && !has("songId")
                } else {
                    has("platform") && has("id") && has("kind")
                };
            let song = has("songId");
            let auto = has("token") && has("busy");
            let music = has("normalSongCount");
            let snapshot = has("version") && has("settings");
            let hand = has("edgeOffsetPx") || has("edge_distance");
            let n = names();
            let dictionary = match (source, encode) {
                (true, true) => &n.encoded_sources,
                (true, false) => &n.sources,
                (false, true) => &n.encoded_fields,
                (false, false) => &n.fields,
            };
            Value::Object(
                values
                    .into_iter()
                    .map(|(key, value)| {
                        let mut mapped =
                            dictionary.get(&key).cloned().unwrap_or_else(|| key.clone());
                        if !source && encode && key == "sources" {
                            mapped = if music {
                                "playlist_albums"
                            } else if snapshot {
                                "playlists"
                            } else {
                                "sources"
                            }
                            .into();
                        }
                        if !encode && key == "playlists" {
                            mapped = "sources".into();
                        }
                        if identity && key == if encode { "sourceId" } else { "id" } {
                            mapped = if encode { "id" } else { "sourceId" }.into();
                        }
                        if identity && key == if encode { "sourceKind" } else { "kind" } {
                            mapped = if encode { "kind" } else { "sourceKind" }.into();
                        }
                        if song && key == if encode { "title" } else { "name" } {
                            mapped = if encode { "name" } else { "title" }.into();
                        }
                        if song && key == if encode { "sourceId" } else { "playlistId" } {
                            mapped = if encode { "playlistId" } else { "sourceId" }.into();
                        }
                        if auto && key == if encode { "active" } else { "enabled" } {
                            mapped = if encode { "enabled" } else { "active" }.into();
                        }
                        if !hand && matches!(key.as_str(), "position" | "overlap") {
                            mapped = key;
                        }
                        (mapped, map(value, encode))
                    })
                    .collect::<Map<_, _>>(),
            )
        }
        value => value,
    }
}
pub fn without_covers(value: &mut Value) {
    match value {
        Value::Array(values) => values.iter_mut().for_each(without_covers),
        Value::Object(values) => {
            values.remove("coverDataUri");
            values.values_mut().for_each(without_covers);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn canonical_settings_roundtrip_the_old_storage_keys() {
        let wire = json!({"number_of_discovered_songs":3,"hand":{"auto_play":false,"edge_distance":0,"position":50,"tilt":12},
            "playlist_albums":[{"name":"QQMusic","playlist_album_id":"0001","typename":"album","enabled":false}]});
        let canonical = decode(wire.clone());
        assert_eq!(canonical["normalSongCount"], 3);
        assert_eq!(canonical["sources"][0]["sourceId"], "0001");
        assert_eq!(canonical["hand"]["autoPlayEnabled"], false);
        assert_eq!(encode(canonical), wire);
    }
    #[test]
    fn songs_sources_geometry_and_temporary_auto_stay_distinct() {
        let wire = json!({"playlists":[{"platform":"QQMusic","id":"1","kind":"album","songCount":2,"enabled":true}],
            "song":{"songId":"01","name":"name","playlistId":"1","typename":"album"},
            "auto":{"token":1,"busy":false,"enabled":true},"position":{"x":1,"y":2},"version":"2.1.0","settings":{}});
        let decoded = decode(wire.clone());
        assert_eq!(decoded["song"]["title"], "name");
        assert_eq!(decoded["sources"][0]["sourceKind"], "album");
        assert_eq!(decoded["auto"]["active"], true);
        assert_eq!(encode(decoded), wire);
    }
}
