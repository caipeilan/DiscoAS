// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(code) = discoas_lib::installer_setup_entrypoint() {
        std::process::exit(code);
    }
    discoas_lib::run()
}
