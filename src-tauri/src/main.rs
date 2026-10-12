// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(feature = "browser-probe"))]
fn main() {
    jarvis_lib::run()
}

#[cfg(feature = "browser-probe")]
fn main() {
    if std::env::var("JARVIS_COMPANION_OBSERVE").is_ok_and(|value| value == "1") {
        jarvis_lib::run()
    } else {
        jarvis_lib::run_browser_probe()
    }
}
