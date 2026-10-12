//! File dialogs and desktop folder opening.
use crate::paths;
use std::path::PathBuf;
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub async fn choose_local_audio(
    app: tauri::AppHandle,
    kind: String,
) -> Result<Vec<PathBuf>, String> {
    let selected = tauri::async_runtime::spawn_blocking(move || {
        let picker = app.dialog().file().set_title(crate::i18n::native_text(
            &app,
            [
                "选择本地音频来源",
                "選擇本機音訊來源",
                "Choose local audio source",
            ],
        ));
        let files = match kind.as_str() {
            "folder" => picker.blocking_pick_folder().map(|p| vec![p]),
            "files" => picker
                .add_filter("Audio", &["mp3", "flac", "wav", "m4a", "ogg"])
                .blocking_pick_files(),
            "playlist" => picker
                .add_filter("M3U / M3U8", &["m3u", "m3u8"])
                .blocking_pick_files(),
            _ => return Err("错误：本地来源类型无效".into()),
        };
        files
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.into_path().map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, String>>()
    })
    .await
    .map_err(|e| e.to_string())?;
    selected
}

pub async fn pick_legacy_folder(app: &tauri::AppHandle) -> Result<Option<PathBuf>, String> {
    let picker = app.clone();
    let selected = tauri::async_runtime::spawn_blocking(move || {
        picker
            .dialog()
            .file()
            .set_title(crate::i18n::native_text(
                &picker,
                [
                    "选择旧版 user_data 文件夹",
                    "選擇舊版 user_data 資料夾",
                    "Choose legacy user_data folder",
                ],
            ))
            .blocking_pick_folder()
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(selected) = selected else {
        return Ok(None);
    };
    let path = selected.into_path().map_err(|e| e.to_string())?;
    Ok(Some(path))
}

pub fn open_data_folder(app: tauri::AppHandle) -> Result<(), String> {
    tauri_plugin_opener::open_path(
        paths::user_data_dir(&app)
            .map_err(|e| e.to_string())?
            .display()
            .to_string(),
        None::<&str>,
    )
    .map_err(|e| e.to_string())
}

pub async fn choose_mystery_cover(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let picker = app.clone();
    let selected = tauri::async_runtime::spawn_blocking(move || {
        picker
            .dialog()
            .file()
            .set_title(crate::i18n::native_text(
                &picker,
                [
                    "选择神秘歌曲封面",
                    "選擇神秘歌曲封面",
                    "Choose mystery cover",
                ],
            ))
            .add_filter(
                crate::i18n::native_text(&picker, ["图片", "圖片", "Images"]),
                &["png", "jpg", "jpeg", "webp", "gif"],
            )
            .blocking_pick_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(selected) = selected else {
        return Ok(None);
    };
    let path = selected.into_path().map_err(|e| e.to_string())?;
    let cover_dir = paths::cover_pic_dir(&app).map_err(|e| e.to_string())?;
    crate::services::covers::copy_mystery_cover(&path, &cover_dir).map(Some)
}
