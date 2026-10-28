use super::{now, ConsoleService};
use crate::services::console::{self as model, ConsoleError, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri::{Listener, Manager};
use tokio::sync::Notify;

pub const PUBLIC_EVENTS: &[&str] = &[
    "library-changed",
    "library-progress",
    "discovery-state-changed",
    "discovery-status-changed",
    "discovery-history-changed",
    "hand-state-changed",
    "hand-visibility-changed",
    "hand-error",
    "auto-hand-state-changed",
    "playback-result",
    "preferences-changed",
    "gui-changed",
    "desktop-changed",
    "onboarding-changed",
    "client-window-result",
];
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamEvent {
    sequence: u64,
    time: u64,
    event: String,
    payload: Value,
}
#[derive(Default)]
struct Buffer {
    sequence: u64,
    cursor: u64,
    rows: VecDeque<StreamEvent>,
}
pub(super) struct Subscription {
    handlers: Mutex<Vec<tauri::EventId>>,
    buffer: Mutex<Buffer>,
    changed: Notify,
    closed: AtomicBool,
    filter: Option<Value>,
    log: Option<(PathBuf, Option<String>, Mutex<model::logs::Cursor>)>,
}
impl Subscription {
    fn push(&self, event: &str, mut payload: Value) {
        model::wire::without_covers(&mut payload);
        if self
            .filter
            .as_ref()
            .and_then(Value::as_object)
            .is_some_and(|filter| {
                filter
                    .iter()
                    .any(|(key, value)| payload.get(key) != Some(value))
            })
        {
            return;
        }
        let mut buffer = self.buffer.lock().unwrap();
        buffer.sequence += 1;
        let sequence = buffer.sequence;
        buffer.rows.push_back(StreamEvent {
            sequence,
            time: now(),
            event: event.into(),
            payload,
        });
        while buffer.rows.len() > 256 {
            buffer.rows.pop_front();
        }
        drop(buffer);
        self.changed.notify_waiters();
    }
    pub fn close(&self, app: &tauri::AppHandle) {
        self.closed.store(true, Ordering::Release);
        for id in self.handlers.lock().unwrap().drain(..) {
            app.unlisten(id);
        }
        self.changed.notify_waiters();
    }
}
pub fn watch(
    app: &tauri::AppHandle,
    event: Option<&str>,
    filter: Option<Value>,
    log_category: Option<String>,
    initial: usize,
) -> Result<Value> {
    let service = app.state::<ConsoleService>();
    let is_log = event == Some("log");
    let names = if is_log {
        Vec::new()
    } else if event.is_none() || event == Some("*") {
        PUBLIC_EVENTS.to_vec()
    } else if PUBLIC_EVENTS.contains(&event.unwrap()) {
        vec![event.unwrap()]
    } else {
        return Err(ConsoleError::argument(
            "未知公开事件。用 schema 查看事件目录。",
        ));
    };
    if filter.as_ref().is_some_and(|v| !v.is_object()) {
        return Err(ConsoleError::argument("filter 应为 JSON 对象。"));
    }
    let log = if is_log {
        let root = crate::paths::user_data_dir(app)
            .map_err(|e| ConsoleError::from(e.to_string()))?
            .join("logs");
        let cursor = model::logs::Cursor::at_end(&root)?;
        Some((root, log_category, Mutex::new(cursor)))
    } else {
        None
    };
    let subscription = Arc::new(Subscription {
        handlers: Mutex::new(Vec::new()),
        buffer: Mutex::new(Buffer::default()),
        changed: Notify::new(),
        closed: AtomicBool::new(false),
        filter,
        log,
    });
    let mut subscriptions = service.subscriptions.lock().unwrap();
    if subscriptions.len() >= 64 {
        return Err(ConsoleError::new(
            "TOO_MANY_SUBSCRIPTIONS",
            "事件订阅过多，请先停止不再使用的订阅。",
        ));
    }
    let id = service.id("watch");
    for name in &names {
        let stream = subscription.clone();
        let name = name.to_string();
        let observed = name.clone();
        let handler = app.listen_any(name, move |event| {
            let payload = serde_json::from_str(event.payload()).unwrap_or(Value::Null);
            stream.push(&observed, model::wire::decode(payload));
        });
        subscription.handlers.lock().unwrap().push(handler);
    }
    if let Some((root, category, _)) = &subscription.log {
        for line in model::logs::tail(root, initial, category.as_deref())? {
            subscription.push("log", json!(line));
        }
    }
    subscriptions.insert(id.clone(), subscription);
    Ok(json!({"subscriptionId":id,"events":if is_log {vec!["log"]} else {names},"after":0}))
}
pub fn unwatch(app: &tauri::AppHandle, id: &str) -> Result<Value> {
    let service = app.state::<ConsoleService>();
    let subscription = service
        .subscriptions
        .lock()
        .unwrap()
        .remove(id)
        .ok_or_else(|| ConsoleError::new("SUBSCRIPTION_NOT_FOUND", "找不到事件订阅。"))?;
    subscription.close(app);
    Ok(json!({"stopped":true,"subscriptionId":id}))
}
pub async fn poll(
    app: &tauri::AppHandle,
    id: &str,
    after: Option<u64>,
    timeout: u64,
) -> Result<Value> {
    let service = app.state::<ConsoleService>();
    let subscription = service
        .subscriptions
        .lock()
        .unwrap()
        .get(id)
        .cloned()
        .ok_or_else(|| ConsoleError::new("SUBSCRIPTION_NOT_FOUND", "找不到事件订阅。"))?;
    let deadline =
        tokio::time::Instant::now() + std::time::Duration::from_millis(timeout.min(30_000));
    loop {
        let changed = subscription.changed.notified();
        if let Some((root, category, cursor)) = &subscription.log {
            let lines = cursor.lock().unwrap().read(root, category.as_deref())?;
            for line in lines {
                subscription.push("log", json!(line));
            }
        }
        let result = {
            let mut buffer = subscription.buffer.lock().unwrap();
            let after = after.unwrap_or(buffer.cursor);
            let rows: Vec<_> = buffer
                .rows
                .iter()
                .filter(|row| row.sequence > after)
                .cloned()
                .collect();
            if let Some(last) = rows.last() {
                buffer.cursor = last.sequence;
            }
            let dropped_before = buffer
                .rows
                .front()
                .map_or(0, |row| row.sequence.saturating_sub(1));
            (!rows.is_empty() || subscription.closed.load(Ordering::Acquire) || tokio::time::Instant::now()>=deadline)
                .then(||json!({"subscriptionId":id,"events":rows,"after":buffer.cursor.max(after),"droppedBefore":dropped_before,"closed":subscription.closed.load(Ordering::Acquire)}))
        };
        if let Some(result) = result {
            return Ok(result);
        }
        let wake = if subscription.log.is_some() {
            (tokio::time::Instant::now() + std::time::Duration::from_millis(250)).min(deadline)
        } else {
            deadline
        };
        let _ = tokio::time::timeout_at(wake, changed).await;
    }
}
