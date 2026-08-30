//! Standalone daemon binary (M2 smoke): boots the loopback WebSocket server
//! with a fresh token and an echo handler. The Tauri shell (M7) embeds the
//! same server with the real core handler.

use std::sync::Arc;

use async_trait::async_trait;
use zhiyu_daemon::{serve, EventBus, RequestHandler};
use zhiyu_protocol::{Command, ErrorInfo};

struct EchoHandler;

#[async_trait]
impl RequestHandler for EchoHandler {
    async fn handle(&self, command: Command) -> Result<serde_json::Value, ErrorInfo> {
        Ok(serde_json::to_value(command).unwrap_or_default())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("zhiyu=debug").init();

    let token = zhiyu_daemon::auth::generate_token();
    let bus = Arc::new(EventBus::new());
    let handler: Arc<dyn RequestHandler> = Arc::new(EchoHandler);
    let addr: std::net::SocketAddr = format!("127.0.0.1:{}", zhiyu_daemon::default_port()).parse()?;

    println!("zhiyu-daemon listening on ws://{addr} token={token}");
    serve(addr, token, handler, bus).await
}
