//! Installs a pinned, offline Spicetify distribution and appends one extension.
//! Paths are supplied by the desktop adapter. This module never resolves Tauri
//! paths, downloads scripts, installs Marketplace, or clears somebody's backup.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};

pub const SPICETIFY_VERSION: &str = "2.45.3";
pub const ARCHIVE_NAME: &str = "spicetify-2.45.3-windows-x64.zip";
pub const ARCHIVE_SHA256: &str = "5d641d4db9caa3891b6a7cc2eb239667af9bb40252a8b413856f9be1e09ff5c1";
const EXTENSION_NAME: &str = "discoas-bridge.js";

#[derive(Clone, Debug)]
pub struct SpotifySetupPaths {
    pub app_root: PathBuf,
    pub local_app_data: PathBuf,
    pub roaming_app_data: PathBuf,
    pub user_profile: PathBuf,
    pub bundled_dir: PathBuf,
    pub search_path: Vec<PathBuf>,
    pub config_override: Option<PathBuf>,
    pub state_override: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotifySetupStatus {
    pub phase: String,
    pub message: String,
    pub tool_version: Option<String>,
    pub tool_path: Option<String>,
    pub managed: bool,
}

impl Default for SpotifySetupStatus {
    fn default() -> Self {
        Self {
            phase: "not_installed".into(),
            message: "尚未安装 / 配置 Spicetify".into(),
            tool_version: None,
            tool_path: None,
            managed: false,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Installation {
    status: SpotifySetupStatus,
    config_dir: PathBuf,
    state_dir: PathBuf,
    spotify_dir: Option<PathBuf>,
    spotify_version: Option<String>,
    extension_hash: String,
    original_extension: Option<Vec<u8>>,
    original_registered: bool,
    #[serde(default)]
    removed: bool,
}

#[derive(Default)]
pub struct SpotifySetupService {
    operation: Mutex<()>,
    busy: AtomicBool,
    last_status: Mutex<Option<SpotifySetupStatus>>,
}

trait Runner {
    fn execute(
        &self,
        tool: &Path,
        args: &[&str],
        paths: &SpotifySetupPaths,
        config: &Path,
        state: &Path,
        timeout: Duration,
    ) -> Result<String, String>;
    fn unpack(&self, paths: &SpotifySetupPaths, destination: &Path) -> Result<(), String>;
}

struct NativeRunner;

impl Runner for NativeRunner {
    fn execute(
        &self,
        tool: &Path,
        args: &[&str],
        paths: &SpotifySetupPaths,
        config: &Path,
        state: &Path,
        timeout: Duration,
    ) -> Result<String, String> {
        run_bounded(
            spicetify_command(tool, args, paths, config, state)?,
            timeout,
        )
    }

    fn unpack(&self, paths: &SpotifySetupPaths, destination: &Path) -> Result<(), String> {
        // A system component, with arguments as values rather than shell text.
        let system_root =
            std::env::var_os("SystemRoot").ok_or("错误：无法找到 Windows 系统目录")?;
        let mut command = Command::new(
            PathBuf::from(system_root).join("System32/WindowsPowerShell/v1.0/powershell.exe"),
        );
        command
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(paths.bundled_dir.join("extract.ps1"))
            .arg("-Archive")
            .arg(paths.bundled_dir.join(ARCHIVE_NAME))
            .arg("-Destination")
            .arg(destination);
        run_bounded(command, Duration::from_secs(45)).map(|_| ())
    }
}

fn spicetify_command(
    tool: &Path,
    args: &[&str],
    paths: &SpotifySetupPaths,
    config: &Path,
    state: &Path,
) -> Result<Command, String> {
    // In the pinned upstream version, SPICETIFY_STATE bypasses the requested
    // subfolder name: both Backup and Extracted would become the same folder.
    // Use the Windows fallback layout instead. Private configurations get an
    // application-owned APPDATA root; real Spotify paths are set explicitly.
    // https://github.com/spicetify/cli/blob/v2.45.3/src/utils/path-utils.go
    if state.file_name().is_none_or(|name| name != "spicetify") {
        return Err("错误：Spicetify 状态目录不兼容".into());
    }
    let roaming = state.parent().ok_or("错误：Spicetify 状态目录不兼容")?;
    let mut command = Command::new(tool);
    command
        .args(args)
        .env("SPICETIFY_CONFIG", config)
        .env_remove("SPICETIFY_STATE")
        .env("APPDATA", roaming)
        .env("LOCALAPPDATA", &paths.local_app_data)
        .env("USERPROFILE", &paths.user_profile);
    Ok(command)
}

fn run_bounded(mut command: Command, timeout: Duration) -> Result<String, String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "错误：无法运行 Spotify 配置工具")?;
    // Drain both pipes, including after the retained output reaches its limit.
    // Spicetify progress output must never deadlock a silent installer.
    fn reader(mut input: impl Read + Send + 'static) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut kept = Vec::new();
            let mut buffer = [0u8; 4096];
            while let Ok(count) = input.read(&mut buffer) {
                if count == 0 {
                    break;
                }
                let retain = count.min(256 * 1024usize - kept.len());
                kept.extend_from_slice(&buffer[..retain]);
            }
            kept
        })
    }
    let output = reader(
        child
            .stdout
            .take()
            .ok_or("错误：无法运行 Spotify 配置工具")?,
    );
    let error = reader(
        child
            .stderr
            .take()
            .ok_or("错误：无法运行 Spotify 配置工具")?,
    );
    let start = Instant::now();
    let result = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(80)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break Err("错误：Spotify 配置超时");
            }
        }
    };
    let mut bytes = output.join().unwrap_or_default();
    bytes.extend(error.join().unwrap_or_default());
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let status = result.map_err(str::to_string)?;
    if !status.success() {
        return Err(short_error(&text));
    }
    Ok(text)
}

fn short_error(output: &str) -> String {
    let lower = output.to_ascii_lowercase();
    if lower.contains("administrator") || lower.contains("root privileges") {
        "错误：请以普通用户运行 Spotify 配置".into()
    } else if lower.contains("permission") || lower.contains("access is denied") {
        "错误：Spotify 文件无法写入".into()
    } else if lower.contains("extension not found")
        || lower.contains("cannot find extension")
        || lower.contains("no such file") && lower.contains(EXTENSION_NAME)
    {
        "错误：Spotify 扩展文件缺失".into()
    } else if lower.contains("outdated")
        || lower.contains("mismatched")
        || lower.contains("restore backup")
    {
        "错误：现有 Spicetify 备份需要更新".into()
    } else if lower.contains("spotify version") && lower.contains("not supported")
        || lower.contains("unsupported spotify")
    {
        "错误：Spotify 版本暂不兼容".into()
    } else if lower.contains("backup") && (lower.contains("empty") || lower.contains("not found")) {
        "错误：缺少 Spotify 原始备份".into()
    } else {
        "错误：Spotify 扩展配置失败".into()
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn verify_archive(path: &Path) -> Result<(), String> {
    let file = fs::read(path).map_err(|_| "错误：缺少 Spotify 安装资源")?;
    if sha256(&file) != ARCHIVE_SHA256 {
        return Err("错误：Spotify 安装资源校验失败".into());
    }
    Ok(())
}

fn support_dir(paths: &SpotifySetupPaths) -> PathBuf {
    paths.app_root.join("spotify-support")
}
fn record_path(paths: &SpotifySetupPaths) -> PathBuf {
    support_dir(paths).join("installation.json")
}

fn read_installation(paths: &SpotifySetupPaths) -> Option<Installation> {
    fs::read(record_path(paths))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

fn write_installation(paths: &SpotifySetupPaths, record: &Installation) -> Result<(), String> {
    fs::create_dir_all(support_dir(paths)).map_err(|_| "错误：无法保存 Spotify 配置状态")?;
    let bytes = serde_json::to_vec_pretty(record).map_err(|_| "错误：无法保存 Spotify 配置状态")?;
    crate::platforms::storage::atomic_write(&record_path(paths), &bytes)
        .map_err(|_| "错误：无法保存 Spotify 配置状态".into())
}

/// Existing installations have priority; their binaries and configuration are
/// not upgraded, relocated, erased or added to a system-wide PATH.
fn external_tool(paths: &SpotifySetupPaths) -> Option<PathBuf> {
    let mut candidates = vec![
        paths.local_app_data.join("spicetify/spicetify.exe"),
        paths.user_profile.join("spicetify-cli/spicetify.exe"),
    ];
    candidates.extend(
        paths
            .search_path
            .iter()
            .map(|path| path.join("spicetify.exe")),
    );
    candidates
        .into_iter()
        .find(|path| path.is_file() && !path.starts_with(support_dir(paths)))
}

fn managed_tool(paths: &SpotifySetupPaths) -> PathBuf {
    support_dir(paths)
        .join(format!("tool-{SPICETIFY_VERSION}"))
        .join("spicetify.exe")
}

fn config_locations(
    paths: &SpotifySetupPaths,
    previous: Option<&Installation>,
) -> (PathBuf, PathBuf) {
    let previous = previous.filter(|record| !record.removed);
    let global = paths
        .config_override
        .clone()
        .unwrap_or_else(|| paths.roaming_app_data.join("spicetify"));
    let config = if let Some(previous) = previous {
        previous.config_dir.clone()
    } else if paths.config_override.is_some() || global.join("config-xpui.ini").is_file() {
        global
    } else {
        support_dir(paths).join("config")
    };
    let normal_state = paths.roaming_app_data.join("spicetify");
    let state = paths.state_override.clone().unwrap_or_else(|| {
        if let Some(previous) = previous.filter(|record| record.state_dir != record.config_dir) {
            previous.state_dir.clone()
        } else if config == support_dir(paths).join("config") {
            support_dir(paths).join("runtime-profile/spicetify")
        } else {
            normal_state
        }
    });
    (config, state)
}

fn ensure_safe_state_layout(config: &Path, state: &Path) -> Result<(), String> {
    if state.file_name().is_none_or(|name| name != "spicetify") {
        return Err("错误：Spicetify 状态目录不兼容".into());
    }
    let mixed_packages = fs::read_dir(config).ok().is_some_and(|entries| {
        entries.flatten().any(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "spa")
        })
    });
    // Older DiscoAS versions placed .spa packages, Raw and Themed beside the
    // configuration. Never run upstream backup/restore against that folder,
    // and never turn an already patched client into a supposed clean backup.
    if mixed_packages || config.join("Raw").exists() || config.join("Themed").exists() {
        return Err("错误：Spicetify 旧版备份目录异常".into());
    }
    // Upstream silently migrates these folders and deletes their source when
    // configuration and state differ. Leave such existing files for an explicit
    // repair instead of triggering that migration from an automatic resume.
    if config != state && (config.join("Backup").exists() || config.join("Extracted").exists()) {
        return Err("错误：Spicetify 旧版备份目录异常".into());
    }
    Ok(())
}

fn ini_value(text: &str, section: &str, key: &str) -> Option<String> {
    let mut current = "";
    for line in text.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.starts_with('[') && line.ends_with(']') {
            current = &line[1..line.len() - 1];
            continue;
        }
        if !current.eq_ignore_ascii_case(section) || line.starts_with([';', '#']) {
            continue;
        }
        if let Some((name, value)) = line.split_once('=') {
            if name.trim().eq_ignore_ascii_case(key) {
                return Some(value.trim().to_string());
            }
        }
    }
    None
}

fn spotify_paths(paths: &SpotifySetupPaths, config: &str) -> (PathBuf, PathBuf) {
    let expand = |value: String| {
        let value = value
            .replace("$LOCALAPPDATA", &paths.local_app_data.to_string_lossy())
            .replace("$USERPROFILE", &paths.user_profile.to_string_lossy())
            .replace("$APPDATA", &paths.roaming_app_data.to_string_lossy());
        PathBuf::from(value)
    };
    let spotify = ini_value(config, "Setting", "spotify_path")
        .filter(|value| !value.is_empty())
        .map(expand)
        .unwrap_or_else(|| paths.roaming_app_data.join("Spotify"));
    let prefs = ini_value(config, "Setting", "prefs_path")
        .filter(|value| !value.is_empty())
        .map(expand)
        .unwrap_or_else(|| paths.roaming_app_data.join("Spotify/prefs"));
    (spotify, prefs)
}

fn store_installation(paths: &SpotifySetupPaths, spotify: &Path, prefs: &Path) -> bool {
    let store_path = |path: &Path| {
        path.to_string_lossy()
            .to_ascii_lowercase()
            .contains("spotifyab.spotifymusic")
    };
    store_path(spotify)
        || store_path(prefs)
        || (!spotify.join("Spotify.exe").is_file()
            && fs::read_dir(paths.local_app_data.join("Packages"))
                .ok()
                .is_some_and(|entries| {
                    entries.flatten().any(|entry| {
                        entry
                            .file_name()
                            .to_string_lossy()
                            .starts_with("SpotifyAB.SpotifyMusic_")
                    })
                }))
}

fn spotify_version(prefs: &Path) -> Option<String> {
    fs::read_to_string(prefs).ok().and_then(|text| {
        text.lines().find_map(|line| {
            line.strip_prefix("app.last-launched-version=")
                .map(|value| value.trim_matches('"').to_string())
        })
    })
}

fn registered(config: &str) -> bool {
    ini_value(config, "AdditionalOptions", "extensions")
        .is_some_and(|value| value.split('|').any(|name| name.trim() == EXTENSION_NAME))
}

fn has_backup(state: &Path) -> bool {
    fs::read_dir(state.join("Backup"))
        .ok()
        .is_some_and(|entries| {
            entries
                .flatten()
                .any(|entry| entry.path().extension().is_some_and(|ext| ext == "spa"))
        })
}

fn has_stock_client(spotify: &Path) -> bool {
    let apps = spotify.join("Apps");
    apps.join("xpui.spa").is_file()
        && fs::read_dir(apps)
            .ok()
            .is_some_and(|entries| !entries.flatten().any(|entry| entry.path().is_dir()))
}

impl SpotifySetupService {
    /// An existing DiscoAS ownership record is the opt-in for future repairs.
    /// A system Spicetify binary alone is never permission to modify Spotify.
    pub fn can_resume(&self, paths: &SpotifySetupPaths) -> bool {
        read_installation(paths).is_some_and(|record| !record.removed)
    }

    pub fn status(&self, paths: &SpotifySetupPaths) -> SpotifySetupStatus {
        if self.busy.load(Ordering::SeqCst) {
            return SpotifySetupStatus {
                phase: "configuring".into(),
                message: "正在配置 Spotify 扩展".into(),
                ..Default::default()
            };
        }
        if let Ok(last) = self.last_status.lock() {
            if let Some(status) = &*last {
                return status.clone();
            }
        }
        read_installation(paths)
            .map(|record| record.status)
            .unwrap_or_default()
    }

    pub fn configure(
        &self,
        paths: &SpotifySetupPaths,
        paired_extension: &Path,
    ) -> Result<SpotifySetupStatus, String> {
        self.run_configuration(paths, paired_extension, false)
            .map(|status| status.unwrap_or_default())
    }

    pub fn resume(
        &self,
        paths: &SpotifySetupPaths,
        paired_extension: &Path,
    ) -> Result<Option<SpotifySetupStatus>, String> {
        self.run_configuration(paths, paired_extension, true)
    }

    fn run_configuration(
        &self,
        paths: &SpotifySetupPaths,
        paired_extension: &Path,
        resume_only: bool,
    ) -> Result<Option<SpotifySetupStatus>, String> {
        let _operation = self
            .operation
            .lock()
            .map_err(|_| "错误：Spotify 配置不可用")?;
        if resume_only && !self.can_resume(paths) {
            return Ok(None);
        }
        self.busy.store(true, Ordering::SeqCst);
        let result = configure_with(paths, paired_extension, &NativeRunner).map(Some);
        if let Ok(mut last) = self.last_status.lock() {
            *last = Some(match &result {
                Ok(Some(status)) => status.clone(),
                Ok(None) => SpotifySetupStatus::default(),
                Err(error) => SpotifySetupStatus {
                    phase: "error".into(),
                    message: error.clone(),
                    ..Default::default()
                },
            });
        }
        self.busy.store(false, Ordering::SeqCst);
        result
    }

    pub fn remove(&self, paths: &SpotifySetupPaths) -> Result<(), String> {
        let _operation = self
            .operation
            .lock()
            .map_err(|_| "错误：Spotify 配置不可用")?;
        remove_with(paths, &NativeRunner)
    }
}

fn configure_with(
    paths: &SpotifySetupPaths,
    paired_extension: &Path,
    runner: &dyn Runner,
) -> Result<SpotifySetupStatus, String> {
    let expected_extension = paths
        .app_root
        .join("spotify-extension")
        .join(EXTENSION_NAME);
    if paired_extension != expected_extension {
        return Err("错误：Spotify 扩展路径无效".into());
    }
    let extension = fs::read(paired_extension).map_err(|_| "错误：缺少 Spotify 配对扩展")?;
    if extension.len() > 256 * 1024 {
        return Err("错误：Spotify 扩展文件无效".into());
    }
    let extension_hash = sha256(&extension);
    let previous = read_installation(paths);
    let (config_dir, state_dir) = config_locations(paths, previous.as_ref());
    ensure_safe_state_layout(&config_dir, &state_dir)?;
    let tool = if let Some(existing) = external_tool(paths) {
        existing
    } else {
        let tool = managed_tool(paths);
        if !tool.is_file() {
            verify_archive(&paths.bundled_dir.join(ARCHIVE_NAME))?;
            fs::create_dir_all(support_dir(paths))
                .map_err(|_| "错误：无法安装 Spotify 配置工具")?;
            let destination = tool.parent().ok_or("错误：Spotify 工具路径无效")?;
            if destination.exists() {
                return Err("错误：Spotify 配置工具安装不完整".into());
            }
            // A failed extract leaves a unique, app-owned staging directory.
            // A later retry can continue without deleting arbitrary files.
            let staging = support_dir(paths).join(format!("staging-{}", rand::random::<u64>()));
            runner.unpack(paths, &staging)?;
            if !staging.join("spicetify.exe").is_file() {
                return Err("错误：Spotify 配置工具安装不完整".into());
            }
            fs::rename(&staging, destination).map_err(|_| "错误：无法保存 Spotify 配置工具")?;
            if !tool.is_file() {
                return Err("错误：Spotify 配置工具安装不完整".into());
            }
        }
        tool
    };
    let managed = tool == managed_tool(paths);
    let config = fs::read_to_string(config_dir.join("config-xpui.ini")).unwrap_or_default();
    let extension_path = config_dir.join("Extensions").join(EXTENSION_NAME);
    let old_bytes = fs::read(&extension_path).ok();
    if old_bytes
        .as_ref()
        .is_some_and(|bytes| bytes.len() > 256 * 1024)
    {
        return Err("错误：已有 Spotify 扩展文件过大".into());
    }
    let continuing = previous.as_ref().filter(|record| !record.removed);
    let mut record = Installation {
        status: SpotifySetupStatus {
            phase: "pending".into(),
            message: "等待配置 Spotify 扩展".into(),
            tool_version: Some(if managed {
                SPICETIFY_VERSION.to_string()
            } else {
                "existing".into()
            }),
            tool_path: Some(tool.to_string_lossy().into_owned()),
            managed,
            ..Default::default()
        },
        config_dir: config_dir.clone(),
        state_dir: state_dir.clone(),
        spotify_dir: None,
        spotify_version: None,
        extension_hash: extension_hash.clone(),
        original_extension: continuing
            .map(|record| record.original_extension.clone())
            .unwrap_or(old_bytes.clone()),
        original_registered: continuing
            .map(|record| record.original_registered)
            .unwrap_or_else(|| registered(&config)),
        removed: false,
    };
    // Persist ownership before modifying the extension or invoking the tool.
    write_installation(paths, &record)?;
    fs::create_dir_all(config_dir.join("Extensions")).map_err(|_| "错误：无法安装 Spotify 扩展")?;
    crate::platforms::storage::atomic_write(&extension_path, &extension)
        .map_err(|_| "错误：无法安装 Spotify 扩展")?;
    let (spotify, prefs) = spotify_paths(paths, &config);
    record.spotify_dir = Some(spotify.clone());
    record.spotify_version = spotify_version(&prefs);
    let pending = if paths.user_profile.join(".spicetify").exists() {
        // Upstream automatically deletes the old folder during migration. Do
        // not trigger that operation against a pre-existing legacy installation.
        Some("已有旧版 Spicetify 配置，请按官方指南迁移后重试")
    } else if !spotify.join("Spotify.exe").is_file() || !spotify.join("Apps").is_dir() {
        Some(if store_installation(paths, &spotify, &prefs) {
            "Microsoft Store 版 Spotify 仅部分兼容，请使用桌面安装版后重试"
        } else {
            "Spicetify 已就绪，安装 Spotify 后将继续配置"
        })
    } else if !prefs.is_file() || record.spotify_version.is_none() {
        Some("请先打开 Spotify 并登录至少 60 秒，然后重试")
    } else if store_installation(paths, &spotify, &prefs) {
        Some("Microsoft Store 版 Spotify 仅部分兼容，请使用桌面安装版后重试")
    } else if has_backup(&state_dir)
        && ini_value(&config, "Preprocesses", "expose_apis").as_deref() == Some("0")
    {
        Some("已有 Spicetify 未启用接口，请启用并更新备份后重试")
    } else {
        None
    };
    if let Some(message) = pending {
        record.status.message = message.into();
        write_installation(paths, &record)?;
        return Ok(record.status);
    }
    // Skip repeated apply on every application launch: retain version, injected
    // file, private pairing and registered extension as the activation signature.
    let injected = spotify.join("Apps/xpui/extensions").join(EXTENSION_NAME);
    let already_ready = continuing.is_some_and(|previous| {
        previous.status.phase == "ready"
            && previous.spotify_version == record.spotify_version
            && previous.extension_hash == extension_hash
            && registered(&config)
            && fs::read(&injected)
                .ok()
                .is_some_and(|bytes| sha256(&bytes) == extension_hash)
            && old_bytes.as_ref() == Some(&extension)
    });
    if already_ready {
        record.status.phase = "ready".into();
        record.status.message = "Spotify 扩展已配置".into();
        write_installation(paths, &record)?;
        return Ok(record.status);
    }
    if !has_backup(&state_dir) && !has_stock_client(&spotify) {
        record.status.phase = "error".into();
        record.status.message = "错误：缺少 Spotify 原始备份".into();
        write_installation(paths, &record)?;
        return Ok(record.status);
    }

    let activation = (|| {
        let version = runner.execute(
            &tool,
            &["--version"],
            paths,
            &config_dir,
            &state_dir,
            Duration::from_secs(10),
        )?;
        record.status.tool_version = Some(version.trim().to_string());
        // The private APPDATA used for correct state subfolders must not change
        // where upstream looks for the real client or its prefs. Set the paths
        // already validated above; list updates keep all existing extensions.
        let spotify_value = spotify.to_string_lossy();
        let prefs_value = prefs.to_string_lossy();
        runner.execute(
            &tool,
            &[
                "config",
                "extensions",
                EXTENSION_NAME,
                "expose_apis",
                "1",
                "spotify_path",
                &spotify_value,
                "prefs_path",
                &prefs_value,
            ],
            paths,
            &config_dir,
            &state_dir,
            Duration::from_secs(15),
        )?;
        let args: &[&str] = if has_backup(&state_dir) {
            &["-n", "apply"]
        } else {
            &["-n", "backup", "apply"]
        };
        runner.execute(
            &tool,
            args,
            paths,
            &config_dir,
            &state_dir,
            Duration::from_secs(90),
        )?;
        // Some upstream copy/configuration failures are warnings with exit 0.
        // Check the expected result before reporting a successful installation.
        let installed_config =
            fs::read_to_string(config_dir.join("config-xpui.ini")).unwrap_or_default();
        if !registered(&installed_config)
            || fs::read(&injected)
                .ok()
                .is_none_or(|bytes| sha256(&bytes) != extension_hash)
        {
            return Err("错误：Spotify 扩展未正确写入".into());
        }
        Ok::<(), String>(())
    })();
    match activation {
        Ok(()) => {
            record.status.phase = "ready".into();
            record.status.message = "Spotify 扩展已配置，重新打开 Spotify 后连接".into();
        }
        Err(error) => {
            record.status.phase = "error".into();
            record.status.message = error;
        }
    }
    write_installation(paths, &record)?;
    Ok(record.status)
}

fn remove_with(paths: &SpotifySetupPaths, runner: &dyn Runner) -> Result<(), String> {
    let Some(mut record) = read_installation(paths).filter(|record| !record.removed) else {
        return Ok(());
    };
    let (_, state_dir) = config_locations(paths, Some(&record));
    ensure_safe_state_layout(&record.config_dir, &state_dir)?;
    record.state_dir = state_dir;
    // Configuration must still refer to the expected current-user location or
    // an application-owned directory. Never recursively remove shared folders.
    let expected_global = paths
        .config_override
        .clone()
        .unwrap_or_else(|| paths.roaming_app_data.join("spicetify"));
    if record.config_dir != expected_global
        && record.config_dir != support_dir(paths).join("config")
    {
        return Err("错误：Spotify 配置路径已变化，请手动移除 DiscoAS 扩展".into());
    }
    let path = record.config_dir.join("Extensions").join(EXTENSION_NAME);
    let bytes = fs::read(&path).ok();
    if bytes
        .as_ref()
        .is_some_and(|bytes| sha256(bytes) != record.extension_hash)
    {
        return Err("错误：DiscoAS 扩展已被修改，请手动移除".into());
    }
    let tool = record
        .status
        .tool_path
        .as_ref()
        .map(PathBuf::from)
        .filter(|tool| tool == &managed_tool(paths) || external_tool(paths).as_ref() == Some(tool))
        .ok_or("错误：Spotify 配置工具已变化，请手动移除 DiscoAS 扩展")?;
    let config = fs::read_to_string(record.config_dir.join("config-xpui.ini")).unwrap_or_default();
    if registered(&config) && !record.original_registered {
        runner.execute(
            &tool,
            &["config", "extensions", "discoas-bridge.js-"],
            paths,
            &record.config_dir,
            &record.state_dir,
            Duration::from_secs(15),
        )?;
    }
    if let Some(original) = &record.original_extension {
        crate::platforms::storage::atomic_write(&path, original)
            .map_err(|_| "错误：无法还原原有 DiscoAS 扩展")?;
    } else if path.is_file() {
        fs::remove_file(&path).map_err(|_| "错误：无法移除 DiscoAS 扩展")?;
    }
    // Reapply remaining extensions only. `restore` would remove everybody's
    // customisation, and clearing backup could make recovery impossible.
    if record.status.phase == "ready" && has_backup(&record.state_dir) {
        runner.execute(
            &tool,
            &["-n", "apply"],
            paths,
            &record.config_dir,
            &record.state_dir,
            Duration::from_secs(90),
        )?;
    }
    record.removed = true;
    record.status.phase = "removed".into();
    record.status.message = "DiscoAS 扩展已移除，原有 Spicetify 配置和备份保留".into();
    write_installation(paths, &record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    struct Fixture {
        paths: SpotifySetupPaths,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir()
                .join(format!("discoas-spotify-setup-{}", rand::random::<u64>()));
            fs::create_dir_all(root.join("app/spotify-extension")).unwrap();
            fs::write(
                root.join("app/spotify-extension/discoas-bridge.js"),
                "paired fixture",
            )
            .unwrap();
            let paths = SpotifySetupPaths {
                app_root: root.join("app"),
                local_app_data: root.join("local"),
                roaming_app_data: root.join("roaming"),
                user_profile: root.join("user"),
                bundled_dir: root.join("bundle"),
                search_path: vec![],
                config_override: None,
                state_override: None,
            };
            let tool = managed_tool(&paths);
            fs::create_dir_all(tool.parent().unwrap()).unwrap();
            fs::write(tool, "fixture executable - never run").unwrap();
            Self { paths }
        }
        fn extension(&self) -> PathBuf {
            self.paths
                .app_root
                .join("spotify-extension/discoas-bridge.js")
        }
        fn spotify(&self) {
            let spotify = self.paths.roaming_app_data.join("Spotify");
            fs::create_dir_all(spotify.join("Apps")).unwrap();
            fs::write(spotify.join("Spotify.exe"), "fixture").unwrap();
            fs::write(spotify.join("Apps/xpui.spa"), "fixture stock package").unwrap();
            fs::write(
                spotify.join("prefs"),
                "app.last-launched-version=\"1.2.3\"\n",
            )
            .unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let root = self.paths.app_root.parent().unwrap();
            assert!(root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("discoas-spotify-setup-"));
            let _ = fs::remove_dir_all(root);
        }
    }
    #[derive(Default)]
    struct FakeRunner {
        calls: Mutex<Vec<Vec<String>>>,
        skip_injection: bool,
    }
    impl Runner for FakeRunner {
        fn execute(
            &self,
            _: &Path,
            args: &[&str],
            paths: &SpotifySetupPaths,
            config: &Path,
            state: &Path,
            _: Duration,
        ) -> Result<String, String> {
            self.calls
                .lock()
                .unwrap()
                .push(args.iter().map(|arg| arg.to_string()).collect());
            if args.first() == Some(&"config") {
                fs::create_dir_all(config).unwrap();
                let path = config.join("config-xpui.ini");
                let mut existing = fs::read_to_string(&path).unwrap_or_default();
                if !existing.contains("[AdditionalOptions]") {
                    existing.push_str("\n[AdditionalOptions]\nextensions = \n");
                }
                if args.get(2) == Some(&"discoas-bridge.js-") {
                    existing = existing
                        .replace("|discoas-bridge.js", "")
                        .replace("discoas-bridge.js", "");
                } else {
                    existing = existing
                        .replace(
                            "extensions = other.js",
                            "extensions = other.js|discoas-bridge.js",
                        )
                        .replace("extensions = \n", "extensions = discoas-bridge.js\n");
                }
                fs::write(path, existing).unwrap();
            }
            if args.contains(&"backup") {
                fs::create_dir_all(state.join("Backup")).unwrap();
                fs::create_dir_all(state.join("Extracted/Raw")).unwrap();
                fs::write(state.join("Backup/xpui.spa"), "fixture stock backup").unwrap();
            }
            if args.contains(&"apply") && !self.skip_injection {
                let contents = fs::read_to_string(config.join("config-xpui.ini")).unwrap();
                let (spotify, _) = spotify_paths(paths, &contents);
                let extensions = spotify.join("Apps/xpui/extensions");
                fs::create_dir_all(&extensions).unwrap();
                let source = config.join("Extensions").join(EXTENSION_NAME);
                let destination = extensions.join(EXTENSION_NAME);
                if source.is_file() {
                    fs::copy(source, destination).unwrap();
                } else if destination.exists() {
                    fs::remove_file(destination).unwrap();
                }
            }
            Ok("2.45.3\n".into())
        }
        fn unpack(&self, _: &SpotifySetupPaths, _: &Path) -> Result<(), String> {
            panic!("fixture must never unpack")
        }
    }
    #[test]
    fn automatic_resume_does_not_opt_in_or_deploy_to_a_new_profile() {
        let fixture = Fixture::new();
        let tool = managed_tool(&fixture.paths);
        fs::remove_file(&tool).unwrap();
        fs::remove_dir(tool.parent().unwrap()).unwrap();
        fs::remove_dir(support_dir(&fixture.paths)).unwrap();
        let service = SpotifySetupService::default();
        assert!(!service.can_resume(&fixture.paths));
        assert_eq!(service.status(&fixture.paths).phase, "not_installed");
        assert!(service
            .resume(&fixture.paths, &fixture.extension())
            .unwrap()
            .is_none());
        assert!(!support_dir(&fixture.paths).exists());
        assert!(!fixture.paths.roaming_app_data.exists());
    }

    #[test]
    fn unrelated_spicetify_and_a_corrupt_record_do_not_authorize_resume() {
        let fixture = Fixture::new();
        let external = fixture.paths.local_app_data.join("spicetify");
        fs::create_dir_all(&external).unwrap();
        fs::write(external.join("spicetify.exe"), "personal tool").unwrap();
        fs::write(record_path(&fixture.paths), "invalid JSON").unwrap();
        let service = SpotifySetupService::default();
        assert!(!service.can_resume(&fixture.paths));
        assert!(service
            .resume(&fixture.paths, &fixture.extension())
            .unwrap()
            .is_none());
        assert_eq!(
            fs::read_to_string(external.join("spicetify.exe")).unwrap(),
            "personal tool"
        );
        assert!(!external.join("Extensions").exists());
        assert_eq!(
            fs::read_to_string(record_path(&fixture.paths)).unwrap(),
            "invalid JSON"
        );
    }

    #[test]
    fn manual_pending_setup_can_resume_but_removed_setup_cannot() {
        let fixture = Fixture::new();
        let runner = FakeRunner::default();
        configure_with(&fixture.paths, &fixture.extension(), &runner).unwrap();
        let service = SpotifySetupService::default();
        assert!(service.can_resume(&fixture.paths));
        // No Spotify is present, so this writes only the isolated setup files
        // and must not invoke the fixture executable or an extraction process.
        assert_eq!(
            service
                .resume(&fixture.paths, &fixture.extension())
                .unwrap()
                .unwrap()
                .phase,
            "pending"
        );
        remove_with(&fixture.paths, &runner).unwrap();
        assert!(!service.can_resume(&fixture.paths));
        let extension = support_dir(&fixture.paths).join("config/Extensions/discoas-bridge.js");
        assert!(!extension.exists());
        assert!(service
            .resume(&fixture.paths, &fixture.extension())
            .unwrap()
            .is_none());
        assert!(!extension.exists());
        assert_eq!(
            read_installation(&fixture.paths).unwrap().status.phase,
            "removed"
        );
    }

    #[test]
    fn manually_configured_external_tool_is_an_opt_in_too() {
        let fixture = Fixture::new();
        let external = fixture.paths.local_app_data.join("spicetify");
        fs::create_dir_all(&external).unwrap();
        fs::write(external.join("spicetify.exe"), "personal tool").unwrap();
        let runner = FakeRunner::default();
        let status = configure_with(&fixture.paths, &fixture.extension(), &runner).unwrap();
        assert!(!status.managed);
        assert!(SpotifySetupService::default().can_resume(&fixture.paths));
    }

    #[test]
    fn private_command_uses_separate_state_subfolders_without_global_side_effects() {
        let fixture = Fixture::new();
        let (config, state) = config_locations(&fixture.paths, None);
        let command = spicetify_command(
            &managed_tool(&fixture.paths),
            &["-n", "apply"],
            &fixture.paths,
            &config,
            &state,
        )
        .unwrap();
        let env = command.get_envs().collect::<Vec<_>>();
        assert!(env
            .iter()
            .any(|(name, value)| *name == "SPICETIFY_STATE" && value.is_none()));
        let roaming = env
            .iter()
            .find_map(|(name, value)| (*name == "APPDATA").then_some(*value).flatten())
            .unwrap();
        // This is upstream v2.45.3's Windows fallback when STATE is unset.
        let upstream_backup = Path::new(roaming).join("spicetify/Backup");
        let upstream_extract = Path::new(roaming).join("spicetify/Extracted");
        assert_eq!(upstream_backup, state.join("Backup"));
        assert_eq!(upstream_extract, state.join("Extracted"));
        assert_ne!(upstream_backup, config);
        assert_ne!(upstream_backup, upstream_extract);
        assert!(state.starts_with(support_dir(&fixture.paths)));
        assert!(!fixture.paths.roaming_app_data.join("spicetify").exists());
    }

    #[test]
    fn existing_global_command_retains_normal_user_state_and_backups() {
        let fixture = Fixture::new();
        let config = fixture.paths.roaming_app_data.join("spicetify");
        fs::create_dir_all(config.join("Backup")).unwrap();
        fs::write(config.join("Backup/xpui.spa"), "existing backup").unwrap();
        fs::write(
            config.join("config-xpui.ini"),
            "[Setting]\ncurrent_theme=personal\n",
        )
        .unwrap();
        let (resolved_config, state) = config_locations(&fixture.paths, None);
        assert_eq!(resolved_config, config);
        assert_eq!(state, config);
        ensure_safe_state_layout(&config, &state).unwrap();
        let command = spicetify_command(
            &managed_tool(&fixture.paths),
            &[],
            &fixture.paths,
            &config,
            &state,
        )
        .unwrap();
        assert!(command.get_envs().any(|(name, value)| {
            name == "APPDATA" && value == Some(fixture.paths.roaming_app_data.as_os_str())
        }));
        assert_eq!(
            fs::read_to_string(config.join("Backup/xpui.spa")).unwrap(),
            "existing backup"
        );
    }

    #[test]
    fn mixed_legacy_state_is_rejected_before_any_extension_record_or_tool_changes() {
        for shared in [false, true] {
            let fixture = Fixture::new();
            let runner = FakeRunner::default();
            let config = if shared {
                fixture.paths.roaming_app_data.join("spicetify")
            } else {
                support_dir(&fixture.paths).join("config")
            };
            fs::create_dir_all(config.join("Extensions")).unwrap();
            fs::write(config.join("config-xpui.ini"), "personal configuration").unwrap();
            fs::write(
                config.join("Extensions/discoas-bridge.js"),
                "existing extension",
            )
            .unwrap();
            fs::write(config.join("xpui.spa"), "old mixed backup").unwrap();
            fs::create_dir_all(config.join("Raw")).unwrap();
            assert_eq!(
                configure_with(&fixture.paths, &fixture.extension(), &runner).unwrap_err(),
                "错误：Spicetify 旧版备份目录异常"
            );
            assert!(runner.calls.lock().unwrap().is_empty());
            assert_eq!(
                fs::read_to_string(config.join("config-xpui.ini")).unwrap(),
                "personal configuration"
            );
            assert_eq!(
                fs::read_to_string(config.join("Extensions/discoas-bridge.js")).unwrap(),
                "existing extension"
            );
            assert_eq!(
                fs::read_to_string(config.join("xpui.spa")).unwrap(),
                "old mixed backup"
            );
            assert!(!record_path(&fixture.paths).exists());
        }
    }

    #[test]
    fn legacy_record_automatic_resume_preserves_mixed_backup_without_recovery() {
        let fixture = Fixture::new();
        configure_with(&fixture.paths, &fixture.extension(), &FakeRunner::default()).unwrap();
        let mut record = read_installation(&fixture.paths).unwrap();
        record.state_dir = record.config_dir.clone();
        record.status.phase = "error".into();
        write_installation(&fixture.paths, &record).unwrap();
        fs::write(record.config_dir.join("xpui.spa"), "mixed original").unwrap();
        fs::create_dir_all(record.config_dir.join("Themed")).unwrap();
        let before = fs::read(record_path(&fixture.paths)).unwrap();
        let service = SpotifySetupService::default();
        assert!(service.can_resume(&fixture.paths));
        assert_eq!(
            service
                .resume(&fixture.paths, &fixture.extension())
                .unwrap_err(),
            "错误：Spicetify 旧版备份目录异常"
        );
        assert_eq!(fs::read(record_path(&fixture.paths)).unwrap(), before);
        assert_eq!(
            fs::read_to_string(record.config_dir.join("xpui.spa")).unwrap(),
            "mixed original"
        );
        assert!(record.config_dir.join("Themed").exists());
        assert!(!support_dir(&fixture.paths).join("runtime-profile").exists());
    }

    #[test]
    fn separate_existing_backups_are_not_automatically_migrated_by_upstream() {
        let fixture = Fixture::new();
        let config = support_dir(&fixture.paths).join("config");
        fs::create_dir_all(config.join("Backup")).unwrap();
        fs::write(config.join("Backup/xpui.spa"), "separate original").unwrap();
        let runner = FakeRunner::default();
        assert_eq!(
            configure_with(&fixture.paths, &fixture.extension(), &runner).unwrap_err(),
            "错误：Spicetify 旧版备份目录异常"
        );
        assert!(runner.calls.lock().unwrap().is_empty());
        assert_eq!(
            fs::read_to_string(config.join("Backup/xpui.spa")).unwrap(),
            "separate original"
        );
    }

    #[test]
    fn already_patched_client_without_clean_backup_is_not_backed_up_as_stock() {
        let fixture = Fixture::new();
        fixture.spotify();
        fs::remove_file(fixture.paths.roaming_app_data.join("Spotify/Apps/xpui.spa")).unwrap();
        fs::create_dir_all(fixture.paths.roaming_app_data.join("Spotify/Apps/xpui")).unwrap();
        let runner = FakeRunner::default();
        let status = configure_with(&fixture.paths, &fixture.extension(), &runner).unwrap();
        assert_eq!(status.phase, "error");
        assert_eq!(status.message, "错误：缺少 Spotify 原始备份");
        assert!(runner.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn mixed_client_packages_without_clean_backup_are_not_treated_as_stock() {
        let fixture = Fixture::new();
        fixture.spotify();
        fs::create_dir_all(fixture.paths.roaming_app_data.join("Spotify/Apps/xpui")).unwrap();
        let runner = FakeRunner::default();
        let status = configure_with(&fixture.paths, &fixture.extension(), &runner).unwrap();
        assert_eq!(status.phase, "error");
        assert_eq!(status.message, "错误：缺少 Spotify 原始备份");
        assert!(runner.calls.lock().unwrap().is_empty());
        assert_eq!(
            fs::read_to_string(fixture.paths.roaming_app_data.join("Spotify/Apps/xpui.spa"))
                .unwrap(),
            "fixture stock package"
        );
    }

    #[test]
    fn exit_zero_without_injected_extension_does_not_claim_configuration_succeeded() {
        let fixture = Fixture::new();
        fixture.spotify();
        let runner = FakeRunner {
            skip_injection: true,
            ..Default::default()
        };
        let status = configure_with(&fixture.paths, &fixture.extension(), &runner).unwrap();
        assert_eq!(status.phase, "error");
        assert_eq!(status.message, "错误：Spotify 扩展未正确写入");
    }

    #[test]
    fn actual_roaming_paths_expand_before_the_child_profile_is_overridden() {
        let fixture = Fixture::new();
        let (spotify, prefs) = spotify_paths(
            &fixture.paths,
            "[Setting]\nspotify_path=$APPDATA/Spotify\nprefs_path=$APPDATA/Spotify/prefs\n",
        );
        assert_eq!(spotify, fixture.paths.roaming_app_data.join("Spotify"));
        assert_eq!(prefs, fixture.paths.roaming_app_data.join("Spotify/prefs"));
    }

    #[test]
    fn upstream_failures_remain_specific_without_exposing_output() {
        assert_eq!(
            short_error("error extension not found: discoas-bridge.js /private/path"),
            "错误：Spotify 扩展文件缺失"
        );
        assert_eq!(
            short_error(
                "Preprocessed Spotify data is outdated. Please run spicetify restore backup apply"
            ),
            "错误：现有 Spicetify 备份需要更新"
        );
        assert_eq!(
            short_error("fatal Access is denied /private/path"),
            "错误：Spotify 文件无法写入"
        );
        assert_eq!(
            short_error("error Spotify version is not supported"),
            "错误：Spotify 版本暂不兼容"
        );
        assert_eq!(
            short_error("error backup is empty"),
            "错误：缺少 Spotify 原始备份"
        );
    }

    #[test]
    fn no_spotify_installs_extension_but_defers_activation() {
        let fixture = Fixture::new();
        let runner = FakeRunner::default();
        let status = configure_with(&fixture.paths, &fixture.extension(), &runner).unwrap();
        assert_eq!(status.phase, "pending");
        assert!(status.message.contains("安装 Spotify"));
        assert!(runner.calls.lock().unwrap().is_empty());
        assert!(support_dir(&fixture.paths)
            .join("config/Extensions/discoas-bridge.js")
            .is_file());
    }
    #[test]
    fn new_profile_uses_backup_apply_without_marketplace() {
        let fixture = Fixture::new();
        fixture.spotify();
        let runner = FakeRunner::default();
        assert_eq!(
            configure_with(&fixture.paths, &fixture.extension(), &runner)
                .unwrap()
                .phase,
            "ready"
        );
        let calls = runner.calls.lock().unwrap();
        assert_eq!(calls[2], ["-n", "backup", "apply"]);
        assert!(calls[1].windows(2).any(|arguments| {
            arguments[0] == "spotify_path"
                && arguments[1]
                    == fixture
                        .paths
                        .roaming_app_data
                        .join("Spotify")
                        .to_string_lossy()
        }));
        assert!(calls[1].windows(2).any(|arguments| {
            arguments[0] == "prefs_path"
                && arguments[1]
                    == fixture
                        .paths
                        .roaming_app_data
                        .join("Spotify/prefs")
                        .to_string_lossy()
        }));
        assert!(support_dir(&fixture.paths)
            .join("runtime-profile/spicetify/Backup/xpui.spa")
            .is_file());
        assert!(!fixture.paths.roaming_app_data.join("spicetify").exists());
        assert!(!calls
            .iter()
            .flatten()
            .any(|arg| ["restore", "clear", "upgrade", "marketplace"].contains(&arg.as_str())));
    }
    #[test]
    fn existing_config_and_backup_survive_and_other_extensions_remain() {
        let fixture = Fixture::new();
        fixture.spotify();
        let config = fixture.paths.roaming_app_data.join("spicetify");
        fs::create_dir_all(config.join("Backup")).unwrap();
        fs::write(config.join("Backup/xpui.spa"), "original").unwrap();
        let original =
            "[Setting]\ncurrent_theme = personal\n[AdditionalOptions]\nextensions = other.js\n";
        fs::write(config.join("config-xpui.ini"), original).unwrap();
        let runner = FakeRunner::default();
        configure_with(&fixture.paths, &fixture.extension(), &runner).unwrap();
        assert_eq!(runner.calls.lock().unwrap()[2], ["-n", "apply"]);
        assert_eq!(
            fs::read_to_string(config.join("Backup/xpui.spa")).unwrap(),
            "original"
        );
        let updated = fs::read_to_string(config.join("config-xpui.ini")).unwrap();
        assert!(updated.contains("current_theme = personal"));
        assert!(updated.contains("other.js|discoas-bridge.js"));
        remove_with(&fixture.paths, &runner).unwrap();
        assert!(fs::read_to_string(config.join("config-xpui.ini"))
            .unwrap()
            .contains("other.js"));
        assert!(config.join("Backup/xpui.spa").is_file());
    }
    #[test]
    fn uninitialised_spotify_does_not_apply() {
        let fixture = Fixture::new();
        fixture.spotify();
        fs::remove_file(fixture.paths.roaming_app_data.join("Spotify/prefs")).unwrap();
        let runner = FakeRunner::default();
        let status = configure_with(&fixture.paths, &fixture.extension(), &runner).unwrap();
        assert_eq!(status.phase, "pending");
        assert!(status.message.contains("60 秒"));
        assert!(runner.calls.lock().unwrap().is_empty());
    }
    #[test]
    fn legacy_profile_is_preserved_without_invoking_upstream_migration() {
        let fixture = Fixture::new();
        fixture.spotify();
        fs::create_dir_all(fixture.paths.user_profile.join(".spicetify")).unwrap();
        let runner = FakeRunner::default();
        let status = configure_with(&fixture.paths, &fixture.extension(), &runner).unwrap();
        assert_eq!(status.phase, "pending");
        assert!(status.message.contains("迁移"));
        assert!(runner.calls.lock().unwrap().is_empty());
        assert!(fixture.paths.user_profile.join(".spicetify").exists());
    }
    #[test]
    fn modified_extension_is_not_erased_during_uninstall() {
        let fixture = Fixture::new();
        let runner = FakeRunner::default();
        configure_with(&fixture.paths, &fixture.extension(), &runner).unwrap();
        let path = support_dir(&fixture.paths).join("config/Extensions/discoas-bridge.js");
        fs::write(&path, "user edits").unwrap();
        assert!(remove_with(&fixture.paths, &runner)
            .unwrap_err()
            .contains("修改"));
        assert_eq!(fs::read_to_string(path).unwrap(), "user edits");
    }
    #[test]
    fn same_setup_does_not_repeat_apply_on_startup() {
        let fixture = Fixture::new();
        fixture.spotify();
        let runner = FakeRunner::default();
        configure_with(&fixture.paths, &fixture.extension(), &runner).unwrap();
        let injected = fixture
            .paths
            .roaming_app_data
            .join("Spotify/Apps/xpui/extensions");
        fs::create_dir_all(&injected).unwrap();
        fs::write(injected.join(EXTENSION_NAME), "paired fixture").unwrap();
        configure_with(&fixture.paths, &fixture.extension(), &runner).unwrap();
        assert_eq!(runner.calls.lock().unwrap().len(), 3);
    }
    #[test]
    fn store_version_is_reported_as_partial_compatibility() {
        let fixture = Fixture::new();
        fs::create_dir_all(
            fixture
                .paths
                .local_app_data
                .join("Packages/SpotifyAB.SpotifyMusic_example"),
        )
        .unwrap();
        let runner = FakeRunner::default();
        let status = configure_with(&fixture.paths, &fixture.extension(), &runner).unwrap();
        assert!(status.message.contains("仅部分兼容"));
        assert!(runner.calls.lock().unwrap().is_empty());
    }
    #[test]
    fn fixture_archive_mismatch_is_rejected() {
        let fixture = Fixture::new();
        fs::create_dir_all(&fixture.paths.bundled_dir).unwrap();
        let path = fixture.paths.bundled_dir.join(ARCHIVE_NAME);
        fs::write(&path, "corrupted archive").unwrap();
        assert!(verify_archive(&path).unwrap_err().contains("校验失败"));
    }

    #[test]
    fn desktop_spotify_is_used_when_store_version_is_also_present() {
        let fixture = Fixture::new();
        fixture.spotify();
        fs::create_dir_all(
            fixture
                .paths
                .local_app_data
                .join("Packages/SpotifyAB.SpotifyMusic_example"),
        )
        .unwrap();
        let status =
            configure_with(&fixture.paths, &fixture.extension(), &FakeRunner::default()).unwrap();
        assert_eq!(status.phase, "ready");
    }

    #[test]
    fn existing_same_named_extension_is_restored_on_uninstall() {
        let fixture = Fixture::new();
        fixture.spotify();
        let config = fixture.paths.roaming_app_data.join("spicetify");
        fs::create_dir_all(config.join("Extensions")).unwrap();
        fs::write(
            config.join("config-xpui.ini"),
            "[AdditionalOptions]\nextensions = other.js|discoas-bridge.js\n",
        )
        .unwrap();
        let path = config.join("Extensions/discoas-bridge.js");
        fs::write(&path, "previous personally configured bridge").unwrap();
        let runner = FakeRunner::default();
        configure_with(&fixture.paths, &fixture.extension(), &runner).unwrap();
        remove_with(&fixture.paths, &runner).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "previous personally configured bridge"
        );
        assert!(registered(
            &fs::read_to_string(config.join("config-xpui.ini")).unwrap()
        ));
        assert!(!runner
            .calls
            .lock()
            .unwrap()
            .iter()
            .flatten()
            .any(|arg| arg == "discoas-bridge.js-"));
    }

    #[test]
    fn stale_injected_extension_is_reapplied_even_when_version_did_not_change() {
        let fixture = Fixture::new();
        fixture.spotify();
        let runner = FakeRunner::default();
        configure_with(&fixture.paths, &fixture.extension(), &runner).unwrap();
        let injected = fixture
            .paths
            .roaming_app_data
            .join("Spotify/Apps/xpui/extensions");
        fs::create_dir_all(&injected).unwrap();
        fs::write(injected.join(EXTENSION_NAME), "unrelated old extension").unwrap();
        configure_with(&fixture.paths, &fixture.extension(), &runner).unwrap();
        assert_eq!(runner.calls.lock().unwrap().len(), 6);
    }

    #[test]
    #[ignore = "extracts the fixed offline distribution into an isolated fixture; no Spotify execution"]
    fn offline_distribution_deploys_into_fixture_only() {
        let mut fixture = Fixture::new();
        let tool = managed_tool(&fixture.paths);
        fs::remove_file(&tool).unwrap();
        fs::remove_dir(tool.parent().unwrap()).unwrap();
        fixture.paths.bundled_dir =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("installer/spicetify");
        let status = configure_with(&fixture.paths, &fixture.extension(), &NativeRunner).unwrap();
        assert_eq!(status.phase, "pending");
        assert!(status.message.contains("安装 Spotify"));
        assert!(tool.is_file());
        assert!(tool
            .parent()
            .unwrap()
            .join("jsHelper/spicetifyWrapper.js")
            .is_file());
        assert!(tool.parent().unwrap().join("css-map.json").is_file());
        assert!(!fixture.paths.roaming_app_data.join("Spotify").exists());
    }
}
