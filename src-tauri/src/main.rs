//! The Tauri shell: boots the local daemon (WS + JSON-RPC) on app start and
//! serves the web frontend. The frontend talks to the daemon over loopback
//! WebSocket (token injected via Tauri command).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use tauri::Manager;
use zhiyu_daemon::event_bus::EventBus;
use zhiyu_daemon::handler::{AppState, CoreHandler};

/// Returns the daemon token to the frontend (single command the web app
/// needs from the shell; everything else goes over the WS).
#[tauri::command]
fn daemon_token(state: tauri::State<String>) -> String {
    state.inner().clone()
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // open the core state under ~/.zhiyu
            let state = AppState::open(None).expect("open app state");
            let state = Arc::new(state);
            let bus = state.bus.clone();

            // token: generate once, persist under ~/.zhiyu/token
            let token = std::fs::read_to_string(zhiyu_core::paths::token_path())
                .unwrap_or_else(|_| {
                    let t = zhiyu_daemon::auth::generate_token();
                    std::fs::create_dir_all(zhiyu_core::paths::data_dir()).ok();
                    let _ = std::fs::write(zhiyu_core::paths::token_path(), &t);
                    t
                });

            app.manage(token.clone());

            // spawn the daemon on a random-ish loopback port
            let handler: Arc<dyn zhiyu_daemon::RequestHandler> = Arc::new(CoreHandler { state: state.clone() });
            let addr: std::net::SocketAddr =
                format!("127.0.0.1:{}", zhiyu_daemon::default_port()).parse().unwrap();
            let bus2 = bus.clone();
            let token2 = token.clone();
            tauri::async_runtime::spawn(async move {
                let _ = zhiyu_daemon::serve(addr, token2, handler, bus2).await;
            });

            // broadcast daemon address to the frontend via an event
            let _ = app.emit("daemon-ready", serde_json::json!({ "port": zhiyu_daemon::default_port() }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![daemon_token])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
