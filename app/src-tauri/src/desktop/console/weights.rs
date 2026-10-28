use crate::services::console::{self as model, ConsoleError, Result};
use serde_json::{Map, Value};
use std::sync::Arc;
use tauri::Manager;

pub(super) async fn inspect(app: &tauri::AppHandle, params: &Map<String, Value>) -> Result<Value> {
    let root = crate::paths::user_data_dir(app).map_err(|e| e.to_string())?;
    let cache = app.state::<Arc<crate::core::cache::Cache>>();
    let identities = params
        .get("identities")
        .cloned()
        .map(model::from_value::<Vec<discoas_core::model::HistoryIdentity>>)
        .transpose()?;
    let ids = identities.as_ref().map(|rows| {
        rows.iter()
            .map(|row| row.song_id.clone())
            .collect::<Vec<_>>()
    });
    let _guard = cache.operation.lock().await;
    let service =
        discoas_core::discovery_service::DiscoveryService::new(root, cache.inner().clone());
    let mut value = model::to_value(service.inspect_weighting(ids.as_deref())?)?;
    if identities.as_ref().is_some_and(|rows| {
        rows.iter()
            .any(|row| value["source"]["platform"] != row.platform)
    }) {
        return Err(ConsoleError::argument("歌曲应属于当前启用来源的平台。"));
    }
    let limit = model::integer(params, "limit", 100)?.min(1000) as usize;
    if let Some(rows) = value["songs"].as_array_mut() {
        let total = rows.len();
        rows.truncate(limit);
        value["total"] = serde_json::json!(total);
    }
    Ok(value)
}
