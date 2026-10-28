use super::{execute, ConsoleService};
use crate::services::console::{ConsoleError, ConsoleReply, ConsoleRequest, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::{Emitter, Manager};

#[derive(Default)]
pub(super) struct ServerState {
    live: Mutex<Option<Server>>,
    error: Mutex<Option<String>>,
}
struct Server {
    address: String,
    stop: Arc<AtomicBool>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Configuration {
    api_version: u32,
    enabled: bool,
    address: String,
    token: String,
}
impl ServerState {
    pub fn shutdown(&self) {
        if let Some(server) = self.live.lock().unwrap().take() {
            server.stop.store(true, Ordering::Release);
        }
    }
}
fn path(app: &tauri::AppHandle) -> Result<PathBuf> {
    crate::paths::user_data_dir(app)
        .map(|root| root.join("settings/console_api.json"))
        .map_err(|e| ConsoleError::from(e.to_string()))
}
fn load(app: &tauri::AppHandle) -> Result<Configuration> {
    let path = path(app)?;
    if !path.exists() {
        let token = rand::random::<[u8; 32]>()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        return Ok(Configuration {
            api_version: 1,
            enabled: false,
            address: String::new(),
            token,
        });
    }
    let bytes =
        std::fs::read(path).map_err(|e| ConsoleError::new("API_CONFIG_FAILED", e.to_string()))?;
    let config: Configuration = serde_json::from_slice(&bytes)
        .map_err(|e| ConsoleError::new("API_CONFIG_FAILED", e.to_string()))?;
    if config.api_version != 1
        || config.token.len() != 64
        || !config.token.bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err(ConsoleError::new(
            "API_CONFIG_FAILED",
            "本机接口连接配置无效。",
        ));
    }
    Ok(config)
}
fn save(app: &tauri::AppHandle, config: &Configuration) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(config)
        .map_err(|e| ConsoleError::new("API_CONFIG_FAILED", e.to_string()))?;
    discoas_core::storage::atomic_write(&path(app)?, &bytes)
        .map_err(|e| ConsoleError::new("API_CONFIG_FAILED", e.to_string()))
}
pub(super) fn info(app: &tauri::AppHandle) -> Value {
    let service = app.state::<ConsoleService>();
    let live = service.api.live.lock().unwrap();
    json!({"enabled":live.is_some(),"address":live.as_ref().map(|server|server.address.clone()),"configPath":path(app).ok().map(|p|p.to_string_lossy().into_owned()),"apiVersion":1,
        "authentication":"Bearer","error":service.api.error.lock().unwrap().clone()})
}
pub(super) fn resume(app: &tauri::AppHandle) -> Result<()> {
    match load(app) {
        Ok(config) if config.enabled => {
            start(app)?;
            Ok(())
        }
        Ok(_) => Ok(()),
        Err(error) => {
            *app.state::<ConsoleService>().api.error.lock().unwrap() = Some(error.message.clone());
            Err(error)
        }
    }
}
pub(super) fn start(app: &tauri::AppHandle) -> Result<Value> {
    let service = app.state::<ConsoleService>();
    let mut live = service.api.live.lock().unwrap();
    if live.is_some() {
        drop(live);
        return Ok(info(app));
    }
    let mut config = load(app)?;
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .map_err(|e| ConsoleError::new("API_START_FAILED", e.to_string()))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| ConsoleError::new("API_START_FAILED", e.to_string()))?;
    config.enabled = true;
    config.address = format!(
        "http://{}",
        listener
            .local_addr()
            .map_err(|e| ConsoleError::new("API_START_FAILED", e.to_string()))?
    );
    save(app, &config)?;
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let running = app.clone();
    let token = config.token.clone();
    std::thread::Builder::new().name("discoas-console-api".into()).spawn(move || {
        let clients=Arc::new(AtomicUsize::new(0));
        while !stopping.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((mut stream,peer))=>{
                    if !peer.ip().is_loopback() {continue;}
                    if clients.fetch_add(1,Ordering::AcqRel)>=8 {clients.fetch_sub(1,Ordering::AcqRel);let _=respond(&mut stream,503,&json!({"error":"busy"}));continue;}
                    struct Connection(Arc<AtomicUsize>);impl Drop for Connection {fn drop(&mut self){self.0.fetch_sub(1,Ordering::AcqRel);}}
                    let connection=Connection(clients.clone());let app=running.clone();let token=token.clone();
                    let _=std::thread::Builder::new().name("discoas-console-request".into()).spawn(move || {
                        let _connection=connection;
                        let _=stream.set_read_timeout(Some(Duration::from_secs(5)));let _=stream.set_write_timeout(Some(Duration::from_secs(5)));
                        let result=handle(&app,&mut stream,&token);
                        if let Err((status,message))=result {let _=respond(&mut stream,status,&json!({"ok":false,"error":{"code":"HTTP_REQUEST_FAILED","message":message}}));}
                    });
                },
                Err(error) if error.kind()==std::io::ErrorKind::WouldBlock=>std::thread::sleep(Duration::from_millis(40)),
                Err(_)=>break,
            }
        }
    }).map_err(|e|ConsoleError::new("API_START_FAILED",e.to_string()))?;
    *live = Some(Server {
        address: config.address,
        stop,
    });
    *service.api.error.lock().unwrap() = None;
    drop(live);
    let value = info(app);
    let _ = app.emit("console-api-changed", &value);
    Ok(value)
}
pub(super) fn stop(app: &tauri::AppHandle) -> Result<Value> {
    let service = app.state::<ConsoleService>();
    let mut live = service.api.live.lock().unwrap();
    let mut config = load(app)?;
    config.enabled = false;
    config.address.clear();
    save(app, &config)?;
    if let Some(server) = live.take() {
        server.stop.store(true, Ordering::Release);
    }
    drop(live);
    let value = info(app);
    let _ = app.emit("console-api-changed", &value);
    Ok(value)
}
struct Request {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}
type HttpResult<T> = std::result::Result<T, (u16, String)>;
fn read_request(reader: &mut impl BufRead) -> HttpResult<Request> {
    let mut first = String::new();
    (&mut *reader)
        .take(8193)
        .read_line(&mut first)
        .map_err(|e| (400, e.to_string()))?;
    if first.len() > 8192 {
        return Err((413, "请求行过长。".into()));
    }
    let pieces: Vec<_> = first.split_whitespace().collect();
    if pieces.len() != 3 || !pieces[2].starts_with("HTTP/1.") {
        return Err((400, "HTTP 请求无效。".into()));
    }
    let method = pieces[0].to_string();
    let path = pieces[1].to_string();
    let mut headers = HashMap::new();
    let mut total = first.len();
    loop {
        let mut line = String::new();
        let count = (&mut *reader)
            .take(8193)
            .read_line(&mut line)
            .map_err(|e| (400, e.to_string()))?;
        total += count;
        if total > 16384 {
            return Err((413, "请求头过大。".into()));
        }
        if count == 0 {
            return Err((400, "HTTP 请求头不完整。".into()));
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| (400, "HTTP 请求头无效。".into()))?;
        let name = name.trim().to_ascii_lowercase();
        if headers.insert(name, value.trim().to_string()).is_some() {
            return Err((400, "重复的 HTTP 请求头。".into()));
        }
    }
    if headers.contains_key("transfer-encoding") {
        return Err((400, "请使用 Content-Length 发送 JSON。".into()));
    }
    let length = headers
        .get("content-length")
        .map(|s| {
            s.parse::<usize>()
                .map_err(|_| (400, "Content-Length 无效。".into()))
        })
        .transpose()?
        .unwrap_or(0);
    if length > 8 * 1024 * 1024 {
        return Err((413, "请求内容过大。".into()));
    }
    let mut body = vec![0; length];
    reader
        .read_exact(&mut body)
        .map_err(|e| (400, e.to_string()))?;
    Ok(Request {
        method,
        path,
        headers,
        body,
    })
}
fn authorized(headers: &HashMap<String, String>, token: &str) -> HttpResult<()> {
    if headers.contains_key("origin") {
        return Err((403, "浏览器页面不能直接访问本机控制接口。".into()));
    }
    let supplied = headers
        .get("authorization")
        .and_then(|s| s.strip_prefix("Bearer "))
        .unwrap_or("");
    let valid = supplied.len() == token.len()
        && supplied
            .bytes()
            .zip(token.bytes())
            .fold(0u8, |diff, (a, b)| diff | (a ^ b))
            == 0;
    if !valid {
        return Err((401, "需要有效的 Bearer 连接令牌。".into()));
    }
    Ok(())
}
fn handle(app: &tauri::AppHandle, stream: &mut TcpStream, token: &str) -> HttpResult<()> {
    let request = read_request(&mut BufReader::new(&mut *stream))?;
    authorized(&request.headers, token)?;
    let reply = match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/v1/info") => ConsoleReply::result("info".into(), Ok(info(app)), true),
        ("GET", "/v1/catalog") => ConsoleReply::result(
            "catalog".into(),
            Ok(super::console_catalog(app.clone())),
            true,
        ),
        ("POST", "/v1/execute") => {
            if !request.headers.get("content-type").is_some_and(|s| {
                s.split(';')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .eq_ignore_ascii_case("application/json")
            }) {
                return Err((415, "Content-Type 应为 application/json。".into()));
            }
            let parsed: ConsoleRequest =
                serde_json::from_slice(&request.body).map_err(|e| (400, e.to_string()))?;
            tauri::async_runtime::block_on(execute(app.clone(), parsed))
        }
        _ => return Err((404, "接口路径不存在。".into())),
    };
    respond(
        stream,
        200,
        &serde_json::to_value(reply).map_err(|e| (500, e.to_string()))?,
    )
    .map_err(|e| (500, e.to_string()))
}
fn respond(stream: &mut TcpStream, status: u16, value: &Value) -> std::io::Result<()> {
    let body = serde_json::to_vec(value)?;
    write!(stream,"HTTP/1.1 {status}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",body.len())?;
    stream.write_all(&body)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocol_reads_utf8_json_and_rejects_ambiguous_lengths() {
        let body = r#"{"command":"source list --search 测试"}"#;
        let raw=format!("POST /v1/execute HTTP/1.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",body.len());
        let request = read_request(&mut std::io::Cursor::new(raw)).unwrap();
        assert_eq!(request.body, body.as_bytes());
        assert!(read_request(&mut std::io::Cursor::new(
            "POST /v1/execute HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\nx"
        ))
        .is_err());
        assert!(read_request(&mut std::io::Cursor::new(
            "POST /v1/execute HTTP/1.1\r\nContent-Length: 99999999\r\n\r\n"
        ))
        .is_err());
    }
    #[test]
    fn local_control_requires_authentication_and_rejects_web_origins() {
        assert!(authorized(&HashMap::new(), "secret").is_err());
        let mut headers = HashMap::from([("authorization".into(), "Bearer secret".into())]);
        assert!(authorized(&headers, "secret").is_ok());
        headers.insert("origin".into(), "https://example.invalid".into());
        assert_eq!(authorized(&headers, "secret").unwrap_err().0, 403);
    }
}
