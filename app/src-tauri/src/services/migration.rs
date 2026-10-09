//! Legacy import is preflighted completely before atomic destination writes.
use super::{covers::image_extension, preferences::validate_preferences, source::normalize_source};
use crate::{
    core::playlist::TypeName,
    platforms,
    settings::{
        gui_setting::GuiSetting,
        music_setting::{MusicSetting, PlaylistAlbum},
    },
};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn legacy_candidate() -> Option<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        roots.extend(cwd.ancestors().take(4).map(Path::to_path_buf));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            roots.push(parent.to_path_buf());
        }
    }
    roots
        .into_iter()
        .map(|p| p.join("user_data"))
        .find(|p| p.join("settings/music_setting.json").is_file())
}

struct LegacyCover {
    filename: String,
    bytes: Vec<u8>,
}

pub struct LegacyImportPlan {
    settings: MusicSetting,
    gui_settings: Option<GuiSetting>,
    playlists: Vec<(PlaylistAlbum, TypeName, Value)>,
    cover: Option<LegacyCover>,
}

impl LegacyImportPlan {
    /// Execute only after all input files and settings were validated by preflight.
    /// Each file is replaced atomically; this preserves the former import write order.
    pub fn commit(
        mut self,
        destination: &super::library::LibraryRepository,
    ) -> Result<bool, String> {
        for (entry, kind, raw) in &self.playlists {
            destination.save_source(&entry.name, &entry.playlist_album_id, *kind, raw)?;
        }
        if let Some(cover) = self.cover {
            let path = destination.root().join("pic").join(cover.filename);
            platforms::storage::atomic_write(&path, &cover.bytes).map_err(|e| e.to_string())?;
            self.settings.mystery_song_cover = path.display().to_string();
        }
        destination.save_settings(&self.settings)?;
        let gui_changed = self.gui_settings.is_some();
        if let Some(gui) = self.gui_settings {
            gui.save_to_path(&destination.gui_path())
                .map_err(|e| e.to_string())?;
        }
        Ok(gui_changed)
    }
}

pub fn resolve_legacy_root(selected: &Path, current_data: &Path) -> Result<PathBuf, String> {
    let root = if selected.join("settings/music_setting.json").is_file() {
        selected.to_path_buf()
    } else {
        selected.join("user_data")
    };
    if !root.join("settings/music_setting.json").is_file() {
        return Err("所选文件夹未找到旧版 settings/music_setting.json".into());
    }
    if root.canonicalize().map_err(|e| e.to_string())?
        == current_data.canonicalize().map_err(|e| e.to_string())?
    {
        return Err("请选择旧版数据目录，当前新版数据目录无需迁移".into());
    }
    Ok(root)
}

fn read_legacy_cover(root: &Path, reference: &str) -> Result<Option<LegacyCover>, String> {
    if reference.trim().is_empty()
        || reference.starts_with("https://")
        || reference.starts_with("http://")
    {
        return Ok(None);
    }
    let reference = Path::new(reference);
    let mut candidates = if reference.is_absolute() {
        vec![reference.to_path_buf()]
    } else {
        // Python resolved src/... and user_data/pic/... from the old application root.
        vec![
            root.parent().unwrap_or(root).join(reference),
            root.join(reference),
        ]
    };
    if !reference.is_absolute()
        && reference
            .to_string_lossy()
            .replace('\\', "/")
            .trim_start_matches("./")
            .eq_ignore_ascii_case("src/question.png")
    {
        // The repository's default artwork moved to assets/. Prefer an actual legacy
        // file, and fall back only for this known default, never an arbitrary custom image.
        candidates.push(root.parent().unwrap_or(root).join("assets/question.png"));
        candidates.push(root.join("assets/question.png"));
    }
    let source = candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or("旧版神秘歌曲封面不存在，请检查旧版图片路径后重试")?;
    if std::fs::metadata(&source).map_err(|e| e.to_string())?.len() > 5 * 1024 * 1024 {
        return Err("旧版封面图片超过 5 MB".into());
    }
    let bytes = std::fs::read(source).map_err(|e| format!("旧版封面无法读取：{e}"))?;
    let extension = image_extension(&bytes)?;
    Ok(Some(LegacyCover {
        filename: format!("mystery-{:x}.{extension}", md5::compute(&bytes)),
        bytes,
    }))
}

fn validate_legacy_cache(
    entry: &PlaylistAlbum,
    kind: TypeName,
    mut raw: Value,
) -> Result<Value, String> {
    if !raw.is_object() {
        return Err(format!(
            "旧歌单 {} 的 JSON 结构无效",
            entry.playlist_album_id
        ));
    }
    if let Some(saved_id) = raw.get("playlist_album_id") {
        let saved_id = saved_id
            .as_str()
            .map(str::to_string)
            .or_else(|| saved_id.as_u64().map(|id| id.to_string()))
            .ok_or("旧歌单缓存的 ID 格式无效")?;
        if saved_id != entry.playlist_album_id {
            return Err("旧歌单缓存的 ID 与设置不一致".into());
        }
    }
    if let Some(saved_kind) = raw.get("playlist_album_type") {
        if saved_kind
            .as_str()
            .is_none_or(|saved| !saved.is_empty() && saved != kind.as_str())
        {
            return Err("旧歌单缓存的歌单/专辑类型与设置不一致".into());
        }
    }
    if raw
        .get("playlist_album_name")
        .is_some_and(|name| !name.is_string())
    {
        return Err("旧歌单名称格式无效".into());
    }
    let songs = raw["song_ids"]
        .as_array()
        .filter(|songs| !songs.is_empty())
        .ok_or_else(|| format!("旧歌单 {} 没有有效歌曲列表", entry.playlist_album_id))?;
    for song in songs {
        let id = song
            .as_str()
            .map(str::to_string)
            .or_else(|| song.as_u64().filter(|id| *id > 0).map(|id| id.to_string()))
            .ok_or("旧歌单包含无效歌曲标识")?;
        platforms::storage::validate_id(&id).map_err(|_| "旧歌单包含无效歌曲标识")?;
    }
    // Older Kugou caches omitted the common type field; fill only missing metadata.
    raw["playlist_album_id"] = Value::String(entry.playlist_album_id.clone());
    raw["playlist_album_type"] = Value::String(kind.as_str().into());
    if raw["playlist_album_name"]
        .as_str()
        .is_none_or(str::is_empty)
    {
        raw["playlist_album_name"] = Value::String(if entry.playlist_album_name.is_empty() {
            entry.playlist_album_id.clone()
        } else {
            entry.playlist_album_name.clone()
        });
    }
    // Validate the exact schema subsequently read by Playlist::load, including covers/timestamps.
    serde_json::from_value::<crate::core::playlist::PlaylistJson>(raw.clone())
        .map_err(|e| format!("旧歌单 {} 的缓存字段无效：{e}", entry.playlist_album_id))?;
    Ok(raw)
}

fn prepare_legacy_gui(root: &Path, current: &GuiSetting) -> Result<Option<GuiSetting>, String> {
    let bytes = match std::fs::read(root.join("settings/gui_setting.json")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("无法读取旧版外观设置：{error}")),
    };
    let mut incoming: GuiSetting =
        serde_json::from_slice(&bytes).map_err(|e| format!("旧版外观设置无效：{e}"))?;
    incoming
        .validate()
        .map_err(|e| format!("旧版外观设置无效：{e}"))?;
    if current.user_configured || current != &GuiSetting::default() {
        return Ok(None);
    }
    // Migrating is an explicit choice too, even when the source appearance is default.
    incoming.user_configured = true;
    Ok(Some(incoming))
}

pub fn prepare_legacy_import(
    root: &Path,
    current: &MusicSetting,
    current_gui: &GuiSetting,
) -> Result<LegacyImportPlan, String> {
    // Do not use MusicSetting::load_from_path: it creates defaults for a missing source.
    let source = std::fs::read(root.join("settings/music_setting.json"))
        .map_err(|e| format!("无法读取旧版设置：{e}"))?;
    let mut incoming: MusicSetting =
        serde_json::from_slice(&source).map_err(|e| format!("旧版设置无效：{e}"))?;
    normalize_legacy_counts(&mut incoming)?;
    let gui_settings = prepare_legacy_gui(root, current_gui)?;
    // The incoming shortcut is intentionally not migrated; all migrated preference values are validated.
    let mut checked_preferences = incoming.clone();
    checked_preferences.shortcut_key.clear();
    validate_preferences(&checked_preferences)?;
    let migrate_preferences = current.playlist_albums.is_empty();
    let mut settings = if migrate_preferences {
        incoming.clone()
    } else {
        current.clone()
    };
    if migrate_preferences {
        settings.playlist_albums.clear();
    }
    settings.shortcut_key = current.shortcut_key.clone();
    let mut playlists = Vec::new();
    for mut entry in incoming.playlist_albums {
        let kind = TypeName::parse(&entry.typename).map_err(|e| e.to_string())?;
        platforms::fetcher_for(&entry.name).map_err(|e| e.to_string())?;
        platforms::storage::validate_id(&entry.playlist_album_id).map_err(|e| e.to_string())?;
        normalize_source(&entry.name, kind, &entry.playlist_album_id)?;
        if settings.playlist_albums.iter().any(|existing| {
            existing.name == entry.name
                && existing.typename == entry.typename
                && existing.playlist_album_id == entry.playlist_album_id
        }) {
            continue;
        }
        let file = root
            .join(&entry.name)
            .join(kind.as_str())
            .join(format!("{}.json", entry.playlist_album_id));
        let bytes = std::fs::read(file)
            .map_err(|e| format!("旧歌单 {} 无法读取：{e}", entry.playlist_album_id))?;
        let raw = serde_json::from_slice(&bytes)
            .map_err(|e| format!("旧歌单 {} 的 JSON 无效：{e}", entry.playlist_album_id))?;
        let raw = validate_legacy_cache(&entry, kind, raw)?;
        if !migrate_preferences {
            entry.enabled = false;
        }
        settings.playlist_albums.push(entry.clone());
        playlists.push((entry, kind, raw));
    }
    if playlists.is_empty() && gui_settings.is_none() {
        return Err("没有新的歌单或外观设置可迁移，已有设置不会被覆盖".into());
    }
    let cover = if migrate_preferences {
        read_legacy_cover(root, &settings.mystery_song_cover)?
    } else {
        None
    };
    // Keep even an invalid current shortcut so the settings screen can repair it later.
    let mut checked_settings = settings.clone();
    checked_settings.shortcut_key.clear();
    validate_preferences(&checked_settings)?;
    settings.normalize_enabled_exclusivity();
    Ok(LegacyImportPlan {
        settings,
        gui_settings,
        playlists,
        cover,
    })
}

fn normalize_legacy_counts(settings: &mut MusicSetting) -> Result<(), String> {
    if !(1..=999).contains(&settings.number_of_discovered_songs)
        || settings.num_of_mystery_song > 50
        || settings.cache_batches > 10
    {
        return Err("旧版每次发现数量超出原版允许范围".into());
    }
    settings.number_of_discovered_songs = settings.number_of_discovered_songs.min(15);
    settings.num_of_mystery_song = settings
        .num_of_mystery_song
        .min(15 - settings.number_of_discovered_songs);
    settings.cache_batches = settings.cache_batches.min(5);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TestLegacyRoot(PathBuf);
    impl TestLegacyRoot {
        fn new() -> Self {
            let root = std::env::temp_dir()
                .join(format!("discoas-legacy-fixture-{}", rand::random::<u64>()));
            std::fs::create_dir_all(&root).unwrap();
            Self(root)
        }
        fn write(&self, relative: &str, bytes: &[u8]) {
            let path = self.0.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
    }
    impl Drop for TestLegacyRoot {
        fn drop(&mut self) {
            if let (Ok(root), Ok(temp)) =
                (self.0.canonicalize(), std::env::temp_dir().canonicalize())
            {
                if root.parent() == Some(temp.as_path())
                    && root
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("discoas-legacy-fixture-"))
                {
                    let _ = std::fs::remove_dir_all(root);
                }
            }
        }
    }

    fn legacy_entry(id: &str) -> PlaylistAlbum {
        PlaylistAlbum {
            name: "NeteaseCloudMusic".into(),
            playlist_album_id: id.into(),
            typename: "playlist".into(),
            playlist_album_name: format!("Playlist {id}"),
            playlist_album_remark: String::new(),
            update_time: String::new(),
            enabled: true,
        }
    }
    fn write_legacy_settings(root: &TestLegacyRoot, setting: &MusicSetting) {
        root.write(
            "user_data/settings/music_setting.json",
            &serde_json::to_vec(setting).unwrap(),
        );
    }
    fn legacy_cache(id: &str) -> Value {
        serde_json::json!({"playlist_album_id":id,"playlist_album_name":format!("Playlist {id}"),"song_ids":[3352212988_u64,"3388568485"]})
    }

    #[test]
    fn valid_original_counts_migrate_with_new_limits_instead_of_blocking_the_library() {
        let mut s = MusicSetting::default();
        s.number_of_discovered_songs = 999;
        s.num_of_mystery_song = 50;
        s.cache_batches = 10;
        normalize_legacy_counts(&mut s).unwrap();
        assert_eq!(
            (
                s.number_of_discovered_songs,
                s.num_of_mystery_song,
                s.cache_batches
            ),
            (15, 0, 5)
        );
        assert!(validate_preferences(&s).is_ok());
        s.number_of_discovered_songs = 1000;
        assert!(normalize_legacy_counts(&mut s).is_err());
    }

    #[test]
    fn legacy_gui_is_optional_and_preserves_original_colors_language_and_full_scales() {
        let root = TestLegacyRoot::new();
        let data_root = root.0.join("user_data");
        assert!(prepare_legacy_gui(&data_root, &GuiSetting::default())
            .unwrap()
            .is_none());
        assert!(!data_root.exists());
        let source = br##"{"night_mode":true,"card_size":3.0,"cancel_button_size":2.75,"setting_size":0.5,"language":"en_US","card":{"background":"#112233","font_color":"#eeeeee"},"cancel_button_night_mode":{"background":"#400601"}}"##;
        root.write("user_data/settings/gui_setting.json", source);
        let gui = prepare_legacy_gui(&data_root, &GuiSetting::default())
            .unwrap()
            .unwrap();
        assert!(gui.night_mode);
        assert_eq!(gui.card_size, 3.0);
        assert_eq!(gui.cancel_button_size, 2.75);
        assert_eq!(gui.setting_size, 0.5);
        assert_eq!(gui.language, "en_US");
        assert_eq!(gui.card.background, "#112233");
        assert_eq!(gui.cancel_button_night_mode.background, "#400601");
        assert_eq!(gui.font_size, 14.0);
        assert_eq!(gui.font_family, "");
        assert!(gui.user_configured);
        assert_eq!(
            std::fs::read(data_root.join("settings/gui_setting.json")).unwrap(),
            source
        );
        let mut configured = GuiSetting::default();
        configured.user_configured = true;
        assert!(prepare_legacy_gui(&data_root, &configured)
            .unwrap()
            .is_none());
        configured.user_configured = false;
        configured.night_mode = true;
        assert!(prepare_legacy_gui(&data_root, &configured)
            .unwrap()
            .is_none());
    }

    #[test]
    fn legacy_gui_is_prevalidated_and_can_migrate_after_songs_were_already_copied() {
        let root = TestLegacyRoot::new();
        let mut settings = MusicSetting::default();
        settings.playlist_albums.push(legacy_entry("123"));
        write_legacy_settings(&root, &settings);
        let source = br#"{"night_mode":true,"card_size":3.0}"#;
        root.write("user_data/settings/gui_setting.json", source);
        // The existing playlist is skipped, so its old cache may already have moved away.
        let plan =
            prepare_legacy_import(&root.0.join("user_data"), &settings, &GuiSetting::default())
                .unwrap();
        assert!(plan.playlists.is_empty());
        assert!(plan.gui_settings.unwrap().night_mode);
        root.write(
            "user_data/settings/gui_setting.json",
            br#"{"font_size":999}"#,
        );
        let error =
            prepare_legacy_import(&root.0.join("user_data"), &settings, &GuiSetting::default())
                .err()
                .unwrap();
        assert!(error.contains("旧版外观设置无效"));
        assert!(!root.0.join("user_data/NeteaseCloudMusic").exists());
    }

    #[test]
    fn legacy_preflight_never_creates_missing_source_settings_or_writes_a_partial_batch() {
        let root = TestLegacyRoot::new();
        let data_root = root.0.join("user_data");
        assert!(prepare_legacy_import(
            &data_root,
            &MusicSetting::default(),
            &GuiSetting::default()
        )
        .is_err());
        assert!(!data_root.exists());
        let mut incoming = MusicSetting::default();
        incoming.playlist_albums = vec![legacy_entry("123"), legacy_entry("456")];
        write_legacy_settings(&root, &incoming);
        let first_bytes = serde_json::to_vec(&legacy_cache("123")).unwrap();
        root.write(
            "user_data/NeteaseCloudMusic/playlist/123.json",
            &first_bytes,
        );
        let original_settings =
            std::fs::read(data_root.join("settings/music_setting.json")).unwrap();
        assert!(prepare_legacy_import(
            &data_root,
            &MusicSetting::default(),
            &GuiSetting::default()
        )
        .is_err());
        assert_eq!(
            std::fs::read(data_root.join("NeteaseCloudMusic/playlist/123.json")).unwrap(),
            first_bytes
        );
        assert_eq!(
            std::fs::read(data_root.join("settings/music_setting.json")).unwrap(),
            original_settings
        );
        assert!(!data_root
            .join("NeteaseCloudMusic/playlist/456.json")
            .exists());
        root.write("user_data/NeteaseCloudMusic/playlist/456.json", b"{broken}");
        assert!(prepare_legacy_import(
            &data_root,
            &MusicSetting::default(),
            &GuiSetting::default()
        )
        .is_err());
        root.write(
            "user_data/NeteaseCloudMusic/playlist/456.json",
            &serde_json::to_vec(&legacy_cache("999")).unwrap(),
        );
        assert!(prepare_legacy_import(
            &data_root,
            &MusicSetting::default(),
            &GuiSetting::default()
        )
        .is_err());
    }

    #[test]
    fn legacy_preflight_validates_preferences_before_any_destination_write() {
        let root = TestLegacyRoot::new();
        let mut incoming = MusicSetting::default();
        incoming.number_of_discovered_songs = 1000;
        incoming.playlist_albums.push(legacy_entry("123"));
        write_legacy_settings(&root, &incoming);
        root.write(
            "user_data/NeteaseCloudMusic/playlist/123.json",
            &serde_json::to_vec(&legacy_cache("123")).unwrap(),
        );
        let error = prepare_legacy_import(
            &root.0.join("user_data"),
            &MusicSetting::default(),
            &GuiSetting::default(),
        )
        .err()
        .unwrap();
        assert!(error.contains("每次发现"));
        let entry = legacy_entry("123");
        let mut invalid_cover = legacy_cache("123");
        invalid_cover["coverUrl"] = serde_json::json!(123);
        assert!(validate_legacy_cache(&entry, TypeName::Playlist, invalid_cover).is_err());
    }

    #[test]
    fn relocated_default_cover_keeps_legacy_file_priority_without_remapping_custom_paths() {
        let root = TestLegacyRoot::new();
        let image = b"\x89PNG\r\n\x1a\nrelocated-default";
        let original = b"\x89PNG\r\n\x1a\noriginal-default";
        let data_root = root.0.join("user_data");
        root.write("assets/question.png", image);
        assert_eq!(
            read_legacy_cover(&data_root, "src/question.png")
                .unwrap()
                .unwrap()
                .bytes,
            image
        );
        assert!(read_legacy_cover(&data_root, "custom/question.png").is_err());
        root.write("src/question.png", original);
        assert_eq!(
            read_legacy_cover(&data_root, "src/question.png")
                .unwrap()
                .unwrap()
                .bytes,
            original
        );
        assert_eq!(
            std::fs::read(root.0.join("assets/question.png")).unwrap(),
            image
        );
    }

    #[test]
    fn legacy_relative_cover_is_captured_for_owned_pic_copy_and_existing_items_are_skipped() {
        let root = TestLegacyRoot::new();
        let mut incoming = MusicSetting::default();
        incoming.mystery_song_cover = "src/question.png".into();
        incoming.shortcut_key = "legacy-invalid-unused-key".into();
        incoming.playlist_albums = vec![legacy_entry("123")];
        write_legacy_settings(&root, &incoming);
        root.write(
            "user_data/NeteaseCloudMusic/playlist/123.json",
            &serde_json::to_vec(&legacy_cache("123")).unwrap(),
        );
        let image = b"\x89PNG\r\n\x1a\nfixture-public-image";
        root.write("src/question.png", image);
        let plan = prepare_legacy_import(
            &root.0.join("user_data"),
            &MusicSetting::default(),
            &GuiSetting::default(),
        )
        .unwrap();
        assert_eq!(plan.playlists.len(), 1);
        assert_eq!(plan.settings.shortcut_key, "Alt+D");
        let cover = plan.cover.unwrap();
        assert_eq!(cover.bytes, image);
        assert_eq!(
            cover.filename,
            format!("mystery-{:x}.png", md5::compute(image))
        );
        assert!(!root.0.join("user_data/pic").exists());
        assert_eq!(
            std::fs::read(root.0.join("src/question.png")).unwrap(),
            image
        );
        assert_eq!(plan.playlists[0].2["playlist_album_type"], "playlist");
        let mut current = MusicSetting::default();
        current.playlist_albums.push(legacy_entry("123"));
        current.number_of_discovered_songs = 5;
        incoming.playlist_albums.push(legacy_entry("456"));
        write_legacy_settings(&root, &incoming);
        root.write(
            "user_data/NeteaseCloudMusic/playlist/456.json",
            &serde_json::to_vec(&legacy_cache("456")).unwrap(),
        );
        let merged =
            prepare_legacy_import(&root.0.join("user_data"), &current, &GuiSetting::default())
                .unwrap();
        assert_eq!(merged.playlists.len(), 1);
        assert_eq!(merged.settings.number_of_discovered_songs, 5);
        assert!(!merged.settings.playlist_albums[1].enabled);
        assert!(merged.cover.is_none());
    }

    #[test]
    fn preflight_and_commit_work_with_explicit_source_and_destination_paths() {
        let fixture = TestLegacyRoot::new();
        let source = fixture.0.join("user_data");
        let mut incoming = MusicSetting::default();
        incoming.playlist_albums.push(legacy_entry("123"));
        incoming.mystery_song_cover = "src/question.png".into();
        write_legacy_settings(&fixture, &incoming);
        let image = b"\x89PNG\r\n\x1a\nfixture-image";
        fixture.write("src/question.png", image);
        fixture.write(
            "user_data/settings/gui_setting.json",
            br#"{"night_mode":true}"#,
        );
        fixture.write(
            "user_data/NeteaseCloudMusic/playlist/123.json",
            &serde_json::to_vec(&legacy_cache("123")).unwrap(),
        );
        let source_before = std::fs::read(source.join("settings/music_setting.json")).unwrap();
        let destination_root = fixture.0.join("new-user-data");
        let destination = super::super::library::LibraryRepository::new(&destination_root);
        assert_eq!(
            destination.read_gui_for_import().unwrap(),
            GuiSetting::default()
        );
        assert!(!destination_root.exists());
        let plan = prepare_legacy_import(&source, &MusicSetting::default(), &GuiSetting::default())
            .unwrap();
        assert!(!destination_root.exists());
        assert!(plan.commit(&destination).unwrap());
        let saved = destination.load_settings().unwrap();
        assert_eq!(saved.playlist_albums.len(), 1);
        assert!(saved.playlist_albums[0].enabled);
        assert_eq!(
            destination
                .cache_summary(&saved.playlist_albums[0])
                .unwrap()
                .0,
            2
        );
        assert_eq!(std::fs::read(&saved.mystery_song_cover).unwrap(), image);
        assert!(Path::new(&saved.mystery_song_cover).starts_with(destination_root.join("pic")));
        assert!(destination.load_gui().unwrap().night_mode);
        assert_eq!(
            std::fs::read(source.join("settings/music_setting.json")).unwrap(),
            source_before
        );
        assert_eq!(
            std::fs::read(fixture.0.join("src/question.png")).unwrap(),
            image
        );
    }
}
