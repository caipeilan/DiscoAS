use crate::services::updates::UpdateInfo;

#[tauri::command]
pub async fn check_for_updates(app: tauri::AppHandle) -> Result<UpdateInfo, String> {
    crate::services::updates::check_for_updates(&app.package_info().version.to_string()).await
}
