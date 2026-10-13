//! One immutable rendering policy shared by every WebView in this process.
use tauri::Manager;

pub struct ActiveRendering(pub String);

pub fn browser_args(app: &tauri::AppHandle) -> String {
    arguments(&app.state::<ActiveRendering>().0)
}

fn arguments(mode: &str) -> String {
    // Preserve Wry's default disabled browser-only UI when adding GPU switches.
    let defaults = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";
    let mode = match mode {
        "integrated" => " --force_low_power_gpu",
        "discrete" => " --force_high_performance_gpu",
        "cpu" => " --disable-gpu",
        _ => "",
    };
    format!("{defaults}{mode}")
}

