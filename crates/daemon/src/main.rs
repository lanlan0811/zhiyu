//! Standalone daemon binary: boots the loopback WebSocket server with the
//! real core handler (sessions, knowledge base, workspace, browser).

use std::sync::Arc;

use zhiyu_daemon::{serve, AppState, CoreHandler, EventBus};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("zhiyu=info").init();

    let state = Arc::new(AppState::open(None)?);
    let bus = state.bus.clone();
    let handler: Arc<dyn zhiyu_daemon::RequestHandler> = Arc::new(CoreHandler { state });

    // load or generate the auth token
    let token = std::fs::read_to_string(zhiyu_daemon::token_path()).unwrap_or_else(|_| {
        let t = zhiyu_daemon::auth::generate_token();
        std::fs::create_dir_all(zhiyu_daemon::data_dir()).ok();
        let _ = std::fs::write(zhiyu_daemon::token_path(), &t);
        t
    });

    let addr: std::net::SocketAddr = format!("127.0.0.1:{}", zhiyu_daemon::default_port()).parse()?;
    println!("zhiyu-daemon listening on ws://{addr}");
    serve(addr, token, handler, bus).await
}
