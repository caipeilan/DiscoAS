//! Fully offline example of the same discovery service used by the desktop application.
use discoas_core::{
    core::{cache::Cache, playlist::SourceKind},
    discovery_service::DiscoveryService,
    settings::music_setting::{MusicSetting, SourceConfig},
    storage::LibraryStore,
};
use serde_json::json;
use std::path::PathBuf;

struct ExampleData(PathBuf);
impl Drop for ExampleData {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Only this temporary fixture is written; existing desktop user data is never accessed.
    let data = ExampleData(std::env::temp_dir().join(format!(
        "discoas-example-{}-{}",
        std::process::id(),
        rand::random::<u64>()
    )));
    let store = LibraryStore::new(&data.0);
    let source = SourceConfig {
        platform: "Spotify".into(),
        source_id: "offline_demo".into(),
        source_kind: "playlist".into(),
        title: "Offline demo".into(),
        remark: String::new(),
        updated_at: String::new(),
        enabled: true,
    };
    store.save_playlist_json(
        "Spotify",
        "offline_demo",
        SourceKind::Playlist,
        &json!({
            "playlist_album_name":"Offline demo", "song_ids":["first", "second"],
            "tracks_info":[
                {"id":"first", "name":"First track", "artists":["Artist A"]},
                {"id":"second", "name":"Second track", "artists":["Artist B"]}
            ],
        }),
    )?;
    let settings = MusicSetting {
        normal_song_count: 1,
        mystery_song_count: 1,
        preload_batch_count: 0,
        sources: vec![source],
        ..Default::default()
    };
    settings.save_to_path(&store.root().join("settings/music_setting.json"))?;
    let service = DiscoveryService::new(&data.0, Cache::new());
    let batch = service
        .discover(false)
        .await
        .map_err(std::io::Error::other)?;
    println!("{}", serde_json::to_string_pretty(&batch.songs)?);
    Ok(())
}
