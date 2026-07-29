#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod auth;
mod hytale;
mod net;
mod server;

use tauri::{Manager, State};

#[tauri::command]
fn detect_install() -> hytale::Install {
    hytale::detect()
}

#[tauri::command]
fn auth_status() -> auth::AuthStatus {
    auth::status()
}

/// Interactive sign-in blocks on a loopback socket, so keep it off the UI thread.
#[tauri::command]
async fn sign_in() -> Result<auth::AuthStatus, String> {
    tauri::async_runtime::spawn_blocking(auth::sign_in)
        .await
        .map_err(|e| format!("Sign-in task failed: {e}"))?
}

/// Launch the game, then get out of the way — one window at a time.
#[tauri::command]
fn launch_game(window: tauri::Window) -> Result<String, String> {
    let msg = hytale::launch()?;
    if let Some(main) = window.get_webview_window("main") {
        let _ = main.minimize();
    }
    Ok(msg)
}

// ---- hosting ----

#[tauri::command]
fn host_status(host: State<'_, server::Host>) -> server::HostStatus {
    host.status()
}

#[tauri::command]
fn host_start(host: State<'_, server::Host>, port: Option<u16>) -> Result<(), String> {
    host.start(port.unwrap_or(5520))
}

#[tauri::command]
fn host_stop(host: State<'_, server::Host>) -> Result<(), String> {
    host.stop()
}

#[tauri::command]
fn host_authorize(host: State<'_, server::Host>) -> Result<(), String> {
    host.authorize()
}

#[tauri::command]
fn host_send(host: State<'_, server::Host>, line: String) -> Result<(), String> {
    host.send(&line)
}

fn main() {
    tauri::Builder::default()
        .manage(server::Host::default())
        .invoke_handler(tauri::generate_handler![
            detect_install,
            auth_status,
            sign_in,
            launch_game,
            host_status,
            host_start,
            host_stop,
            host_authorize,
            host_send
        ])
        .run(tauri::generate_context!())
        .expect("failed to start HyPortal");
}
