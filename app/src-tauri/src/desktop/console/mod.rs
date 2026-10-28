//! Native console and local API share the same semantic dispatcher.
mod api;
mod dispatch;
mod events;
mod music;
mod settings;
mod sources;
mod weights;
mod windows;

use crate::services::console::{
    self as model, ConsoleError, ConsoleReply, ConsoleRequest, Invocation, Result,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use tauri::{Emitter, Listener, Manager};
use tokio::sync::{oneshot, Notify};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord {
    pub operation_id: String,
    pub request_id: String,
    pub library_request_id: String,
    pub method: String,
    pub status: String,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub progress: Option<Value>,
    pub data: Option<Value>,
    pub error: Option<ConsoleError>,
    pub cancellable: bool,
    pub cancel_requested: bool,
    #[serde(skip)]
    cancelled: Arc<AtomicBool>,
}
#[derive(Clone)]
pub(super) struct Operation {
    pub id: String,
    pub library_request: String,
    pub cancelled: Arc<AtomicBool>,
}
impl Operation {
    pub fn check(&self) -> Result<()> {
        if self.cancelled.load(Ordering::Acquire) {
            Err(ConsoleError::new("CANCELLED", "任务已取消。"))
        } else {
            Ok(())
        }
    }
}
#[derive(Default)]
pub struct ConsoleService {
    next: AtomicU64,
    history: Mutex<VecDeque<String>>,
    tasks: Mutex<BTreeMap<String, TaskRecord>>,
    changed: Notify,
    ui: Mutex<HashMap<String, oneshot::Sender<Result<Value>>>>,
    subscriptions: Mutex<HashMap<String, Arc<events::Subscription>>>,
    api: api::ServerState,
    playback: Mutex<Value>,
}
pub(super) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
impl ConsoleService {
    pub(super) fn id(&self, prefix: &str) -> String {
        format!("{prefix}-{}", self.next.fetch_add(1, Ordering::Relaxed) + 1)
    }
    pub fn start(app: &tauri::AppHandle) {
        let observed = app.clone();
        app.listen("library-progress", move |event| {
            let Ok(progress) = serde_json::from_str::<Value>(event.payload()) else {
                return;
            };
            let Some(request) = progress["requestId"].as_str() else {
                return;
            };
            let service = observed.state::<ConsoleService>();
            let next = {
                let mut tasks = service.tasks.lock().unwrap();
                tasks
                    .values_mut()
                    .find(|task| task.library_request_id == request && task.finished_at.is_none())
                    .map(|task| {
                        task.progress = Some(progress.clone());
                        task.clone()
                    })
            };
            if let Some(task) = next {
                if task.cancel_requested {
                    super::library::cancel_library_operation(
                        observed.clone(),
                        task.library_request_id.clone(),
                    );
                }
                let _ = observed.emit("console-task-changed", task);
                service.changed.notify_waiters();
            }
        });
        let observed = app.clone();
        app.listen("playback-result", move |event| {
            if let Ok(result) = serde_json::from_str::<Value>(event.payload()) {
                *observed.state::<ConsoleService>().playback.lock().unwrap() =
                    model::wire::decode(result);
            }
        });
        if let Err(error) = api::resume(app) {
            super::diagnostics::log_event(app, "console_api", &error.code);
        }
    }
    pub fn stop(&self, app: &tauri::AppHandle) {
        self.api.shutdown();
        for (_, subscription) in self.subscriptions.lock().unwrap().drain() {
            subscription.close(app);
        }
        self.ui.lock().unwrap().clear();
    }
    pub(super) async fn ui_action(
        app: &tauri::AppHandle,
        action: &str,
        params: Value,
    ) -> Result<Value> {
        if app.get_webview_window("main").is_none() {
            return Err(ConsoleError::new("UI_UNAVAILABLE", "主界面不可用。"));
        }
        let service = app.state::<Self>();
        let id = service.id("ui");
        let (send, receive) = oneshot::channel();
        service.ui.lock().unwrap().insert(id.clone(), send);
        let sent = app.emit_to(
            "main",
            "console-ui-request",
            json!({"id":id,"action":action,"params":params}),
        );
        if sent.is_err() {
            service.ui.lock().unwrap().remove(&id);
            return Err(ConsoleError::new("UI_UNAVAILABLE", "无法连接主界面。"));
        }
        let result = tokio::time::timeout(std::time::Duration::from_secs(8), receive).await;
        service.ui.lock().unwrap().remove(&id);
        result
            .map_err(|_| ConsoleError::new("UI_TIMEOUT", "主界面未及时回复。"))?
            .map_err(|_| ConsoleError::new("UI_UNAVAILABLE", "主界面连接已结束。"))?
    }
    fn record_history(&self, line: &str) {
        let mut history = self.history.lock().unwrap();
        if history.back().is_none_or(|old| old != line) {
            history.push_back(line.into());
        }
        while history.len() > 200 {
            history.pop_front();
        }
    }
    pub(super) fn history(&self, limit: usize) -> Value {
        let history = self.history.lock().unwrap();
        json!(history
            .iter()
            .skip(history.len().saturating_sub(limit.min(200)))
            .collect::<Vec<_>>())
    }
    fn create_task(&self, id: &str, invocation: &Invocation) -> Result<(TaskRecord, Operation)> {
        let mut tasks = self.tasks.lock().unwrap();
        if tasks.len() >= 128 {
            let completed = tasks
                .iter()
                .find(|(_, task)| task.finished_at.is_some())
                .map(|(id, _)| id.clone());
            if let Some(id) = completed {
                tasks.remove(&id);
            } else {
                return Err(ConsoleError::new(
                    "TOO_MANY_TASKS",
                    "正在执行的任务过多，请等待已有任务结束。",
                ));
            }
        }
        let operation_id = self.id("task");
        let cancelled = Arc::new(AtomicBool::new(false));
        let supplied = model::optional_text(&invocation.params, "requestId")?;
        if supplied
            .as_ref()
            .is_some_and(|id| id.is_empty() || id.len() > 128)
        {
            return Err(ConsoleError::argument(
                "requestId 应为 1–128 字节的字符串。",
            ));
        }
        let library_request_id = supplied.unwrap_or_else(|| format!("console-{operation_id}"));
        if tasks
            .values()
            .any(|task| task.library_request_id == library_request_id && task.finished_at.is_none())
        {
            return Err(ConsoleError::new(
                "DUPLICATE_REQUEST",
                "此 requestId 的任务仍在运行。",
            ));
        }
        let task = TaskRecord {
            operation_id: operation_id.clone(),
            request_id: id.into(),
            library_request_id: library_request_id.clone(),
            method: invocation.method.clone(),
            status: "running".into(),
            started_at: now(),
            finished_at: None,
            progress: None,
            data: None,
            error: None,
            cancellable: invocation.method.starts_with("source.")
                && invocation.method != "source.choose-local",
            cancel_requested: false,
            cancelled: cancelled.clone(),
        };
        tasks.insert(operation_id.clone(), task.clone());
        Ok((
            task,
            Operation {
                library_request: library_request_id,
                id: operation_id,
                cancelled,
            },
        ))
    }
    pub(super) fn task(&self, id: &str) -> Result<TaskRecord> {
        self.tasks
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| ConsoleError::new("TASK_NOT_FOUND", format!("找不到任务：{id}")))
    }
    pub(super) fn tasks(&self, status: Option<&str>) -> Value {
        json!(self
            .tasks
            .lock()
            .unwrap()
            .values()
            .filter(|task| status.is_none_or(|s| s == task.status))
            .collect::<Vec<_>>())
    }
    pub(super) async fn wait_task(&self, id: &str, timeout: u64) -> Result<Value> {
        let deadline =
            tokio::time::Instant::now() + std::time::Duration::from_millis(timeout.min(120_000));
        loop {
            let changed = self.changed.notified();
            let task = self.task(id)?;
            if task.finished_at.is_some() {
                return model::to_value(task);
            }
            if tokio::time::timeout_at(deadline, changed).await.is_err() {
                return model::to_value(self.task(id)?);
            }
        }
    }
    pub(super) fn cancel_task(&self, app: &tauri::AppHandle, id: &str) -> Result<Value> {
        let mut tasks = self.tasks.lock().unwrap();
        let task = tasks
            .get_mut(id)
            .ok_or_else(|| ConsoleError::new("TASK_NOT_FOUND", format!("找不到任务：{id}")))?;
        if task.finished_at.is_some() {
            return Ok(json!({"requested":false,"reason":"finished"}));
        }
        if !task.cancellable {
            return Ok(json!({"requested":false,"reason":"unsupported"}));
        }
        task.cancel_requested = true;
        task.cancelled.store(true, Ordering::Release);
        let interrupted =
            super::library::cancel_library_operation(app.clone(), task.library_request_id.clone());
        let next = task.clone();
        drop(tasks);
        let _ = app.emit("console-task-changed", next);
        self.changed.notify_waiters();
        Ok(json!({"requested":true,"interrupted":interrupted}))
    }
}
async fn run_task(
    app: tauri::AppHandle,
    invocation: Invocation,
    operation: Operation,
) -> Result<Value> {
    let mut result = dispatch::execute(&app, &invocation, Some(&operation)).await;
    clean_result(&mut result, &invocation);
    let service = app.state::<ConsoleService>();
    let next = {
        let mut tasks = service.tasks.lock().unwrap();
        tasks.get_mut(&operation.id).map(|task| {
            task.finished_at = Some(now());
            match &result {
                Ok(value) => {
                    task.status = "succeeded".into();
                    task.data = Some(value.clone());
                }
                Err(error) => {
                    task.status = if error.code == "CANCELLED" {
                        "cancelled"
                    } else {
                        "failed"
                    }
                    .into();
                    task.error = Some(error.clone());
                }
            }
            task.clone()
        })
    };
    if let Some(task) = next {
        let _ = app.emit("console-task-changed", task);
    }
    service.changed.notify_waiters();
    result
}
pub async fn execute(app: tauri::AppHandle, request: ConsoleRequest) -> ConsoleReply {
    let service = app.state::<ConsoleService>();
    let id = request.id.clone().unwrap_or_else(|| service.id("request"));
    let invocation = match request.invocation() {
        Ok(value) => value,
        Err(error) => return ConsoleReply::result(id, Err(error), false),
    };
    if let Some(line) = &request.command {
        service.record_history(line);
    }
    let json = invocation.json;
    if model::spec(&invocation.method).is_some_and(|item| item.asynchronous) {
        let (task, operation) = match service.create_task(&id, &invocation) {
            Ok(value) => value,
            Err(error) => return ConsoleReply::result(id, Err(error), json),
        };
        let _ = app.emit("console-task-changed", &task);
        if invocation.asynchronous {
            tauri::async_runtime::spawn(run_task(app.clone(), invocation, operation));
            ConsoleReply::result(id, model::to_value(task), json)
        } else {
            ConsoleReply::result(id, run_task(app.clone(), invocation, operation).await, json)
        }
    } else {
        let mut result = dispatch::execute(&app, &invocation, None).await;
        clean_result(&mut result, &invocation);
        ConsoleReply::result(id, result, json)
    }
}
fn clean_result(result: &mut Result<Value>, invocation: &Invocation) {
    if matches!(invocation.method.as_str(), "cover.get" | "history.covers")
        || invocation.params.get("includeCovers") == Some(&Value::Bool(true))
    {
        return;
    }
    match result {
        Ok(value) => model::wire::without_covers(value),
        Err(error) => {
            if let Some(value) = &mut error.details {
                model::wire::without_covers(value);
            }
        }
    }
}
#[tauri::command]
pub async fn console_execute(app: tauri::AppHandle, request: ConsoleRequest) -> ConsoleReply {
    execute(app, request).await
}
#[tauri::command]
pub fn console_catalog(app: tauri::AppHandle) -> Value {
    json!({"version":app.package_info().version.to_string(),"apiVersion":1,"commands":model::catalog(),"settings":model::settings::catalog(),"events":events::PUBLIC_EVENTS,"api":api::info(&app)})
}
#[tauri::command]
pub fn console_ui_reply(
    window: tauri::WebviewWindow,
    id: String,
    data: Option<Value>,
    error: Option<ConsoleError>,
) -> bool {
    if window.label() != "main" {
        return false;
    }
    let service = window.state::<ConsoleService>();
    let sent = service
        .ui
        .lock()
        .unwrap()
        .remove(&id)
        .is_some_and(|sender| {
            sender
                .send(error.map_or_else(|| Ok(data.unwrap_or(Value::Null)), Err))
                .is_ok()
        });
    sent
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_results_and_partial_errors_omit_binary_covers() {
        let invocation = model::parse::command("source list").unwrap();
        let mut result = Ok(
            json!({"rows":[{"coverDataUri":"data:image/png;base64,binary","coverUrl":"https://example.invalid/cover"}]}),
        );
        clean_result(&mut result, &invocation);
        assert!(result.as_ref().unwrap()["rows"][0]
            .get("coverDataUri")
            .is_none());
        assert!(result.as_ref().unwrap()["rows"][0]["coverUrl"].is_string());
        let mut error = ConsoleError::new("CANCELLED", "已取消");
        error.details = Some(json!({"coverDataUri":"data"}));
        let mut result = Err(error);
        clean_result(&mut result, &invocation);
        assert!(result
            .unwrap_err()
            .details
            .unwrap()
            .get("coverDataUri")
            .is_none());
        let invocation =
            model::parse::command("source get QQMusic:playlist:1 --include-covers true").unwrap();
        let mut result = Ok(json!({"coverDataUri":"data"}));
        clean_result(&mut result, &invocation);
        assert_eq!(result.unwrap()["coverDataUri"], "data");
    }
    #[tokio::test]
    async fn custom_request_ids_and_cancel_flags_belong_to_the_created_task() {
        let service = ConsoleService::default();
        let invocation = model::parse::command(
            "source import-local [\"D:/test.wav\"] --request-id local-test --async",
        )
        .unwrap();
        let (task, operation) = service.create_task("client-1", &invocation).unwrap();
        assert_eq!(task.library_request_id, "local-test");
        assert_eq!(operation.library_request, "local-test");
        assert_eq!(
            service
                .create_task("client-2", &invocation)
                .err()
                .unwrap()
                .code,
            "DUPLICATE_REQUEST"
        );
        assert_eq!(
            service.wait_task(&task.operation_id, 0).await.unwrap()["status"],
            "running"
        );
        operation.cancelled.store(true, Ordering::Release);
        assert_eq!(operation.check().unwrap_err().code, "CANCELLED");
    }
}
