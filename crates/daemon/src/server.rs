//! The loopback WebSocket server: handshake, token auth, request dispatch
//! and sequenced event delivery.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::accept_async;
use tracing::{debug, info, warn};
use zhiyu_protocol::{
    Command, ErrorInfo, Hello, HelloReply, Inbound, Outbound, PROTOCOL_VERSION, Response,
};

use crate::auth::validate_token;
use crate::event_bus::EventBus;

/// The handshake must complete within this window.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Handles a JSON-RPC command and returns the `result` payload.
#[async_trait]
pub trait RequestHandler: Send + Sync {
    async fn handle(&self, command: Command) -> Result<serde_json::Value, ErrorInfo>;
}

/// Spawns the daemon listener on `addr`. The caller keeps the returned join
/// handle to shut the server down.
pub async fn serve(
    addr: std::net::SocketAddr,
    token: String,
    handler: Arc<dyn RequestHandler>,
    bus: Arc<EventBus>,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "daemon listening");
    loop {
        let (stream, peer) = listener.accept().await?;
        let token = token.clone();
        let handler = handler.clone();
        let bus = bus.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, peer, token, handler, bus).await {
                warn!(%peer, error = %e, "connection dropped");
            }
        });
    }
}

async fn handle_connection(
    stream: TcpStream,
    peer: std::net::SocketAddr,
    token: String,
    handler: Arc<dyn RequestHandler>,
    bus: Arc<EventBus>,
) -> anyhow::Result<()> {
    let ws = timeout(HANDSHAKE_TIMEOUT, accept_async(stream)).await??;
    debug!(%peer, "websocket accepted");
    let (mut sink, mut source) = ws.split();

    // ---- handshake: first frame must be a valid Hello with the right token
    let first = timeout(HANDSHAKE_TIMEOUT, source.next()).await?.ok_or_else(|| {
        anyhow::anyhow!("client closed before handshake")
    })??;
    let hello = parse_hello(first)?;
    if hello.protocol_version != PROTOCOL_VERSION {
        reject(&mut sink, -32001, "protocol version mismatch").await?;
        return Ok(());
    }
    if !validate_token(&token, &hello.token) {
        warn!(%peer, "rejected: bad token");
        reject(&mut sink, -32002, "authentication failed").await?;
        return Ok(());
    }

    // ---- reply + replay + subscribe
    let reply = Outbound::HelloReply(HelloReply {
        protocol_version: PROTOCOL_VERSION,
        server_name: "zhiyu-daemon".into(),
        last_seq: bus.last_seq(),
        now: chrono::Utc::now(),
    });
    sink.send(frame(&reply)).await?;

    let resume_from = hello.last_seq.unwrap_or_else(|| bus.last_seq());
    for event in bus.replay_from(resume_from) {
        sink.send(frame(&Outbound::Event(event))).await?;
    }

    // Live events and responses are relayed to the sink through one channel
    // so the sink is only ever borrowed from a single task.
    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<Outbound>();
    let mut live_rx = bus.subscribe();

    let sink_task: tokio::task::JoinHandle<anyhow::Result<()>> = tokio::spawn(async move {
        while let Some(o) = out_rx.recv().await {
            if sink.send(frame(&o)).await.is_err() {
                break;
            }
        }
        anyhow::Result::Ok(())
    });

    loop {
        tokio::select! {
            // inbound request
            msg = source.next() => {
                match msg {
                    Some(Ok(WsMessage::Text(text))) => {
                        let req = match serde_json::from_str::<Inbound>(&text) {
                            Ok(Inbound::Request(req)) => req,
                            Ok(Inbound::Hello(_)) => continue,
                            Err(_) => {
                                let _ = out_tx.send(Outbound::Response(Response {
                                    id: 0,
                                    result: None,
                                    error: Some(ErrorInfo { code: -32700, message: "parse error".into() }),
                                }));
                                continue;
                            }
                        };
                        let id = req.id;
                        let out_tx = out_tx.clone();
                        let handler = handler.clone();
                        // Handle concurrently so slow commands do not block the socket.
                        tokio::spawn(async move {
                            let out = match handler.handle(req.command).await {
                                Ok(result) => Outbound::Response(Response { id, result: Some(result), error: None }),
                                Err(e) => Outbound::Response(Response { id, result: None, error: Some(e) }),
                            };
                            let _ = out_tx.send(out);
                        });
                    }
                    Some(Ok(WsMessage::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        warn!(%peer, error = %e, "socket error");
                        break;
                    }
                }
            }
            // outbound event
            ev = live_rx.recv() => {
                let Some(ev) = ev else { break };
                if out_tx.send(Outbound::Event(ev)).is_err() {
                    break;
                }
            }
        }
    }

    drop(out_tx);
    let _ = sink_task.await;
    info!(%peer, "connection closed");
    Ok(())
}

async fn reject(
    sink: &mut futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<TcpStream>, WsMessage>,
    code: i32,
    message: &str,
) -> anyhow::Result<()> {
    sink.send(frame(&Outbound::Response(Response {
        id: 0,
        result: None,
        error: Some(ErrorInfo { code, message: message.to_string() }),
    })))
    .await
    .map_err(|e| anyhow::anyhow!("failed to send reject response: {e}"))?;
    Ok(())
}

fn parse_hello(msg: WsMessage) -> anyhow::Result<Hello> {
    match msg {
        WsMessage::Text(text) => {
            let inbound: Inbound = serde_json::from_str(&text)?;
            match inbound {
                Inbound::Hello(h) => Ok(h),
                Inbound::Request(_) => anyhow::bail!("expected hello, got request"),
            }
        }
        _ => anyhow::bail!("expected text frame"),
    }
}

fn frame(outbound: &Outbound) -> WsMessage {
    WsMessage::Text(serde_json::to_string(outbound).expect("outbound serialization"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_tungstenite::connect_async;
    use tungstenite::client::IntoClientRequest;
    use zhiyu_protocol::Event;

    struct EchoHandler;

    #[async_trait]
    impl RequestHandler for EchoHandler {
        async fn handle(&self, command: Command) -> Result<serde_json::Value, ErrorInfo> {
            Ok(serde_json::to_value(command).unwrap())
        }
    }

    fn spawn_server(
        token: String,
        bus: Arc<EventBus>,
    ) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let handler: Arc<dyn RequestHandler> = Arc::new(EchoHandler);
        let handle = tokio::spawn(async move {
            let listener = TcpListener::from_std(listener).unwrap();
            let (stream, _) = listener.accept().await.unwrap();
            let _ = handle_connection(stream, addr, token, handler, bus).await;
        });
        (addr, handle)
    }

    /// Full loopback integration test: handshake → request → response.
    #[tokio::test]
    async fn handshake_request_response_loop() {
        let token = crate::auth::generate_token();
        let bus = Arc::new(EventBus::new());
        let (addr, srv) = spawn_server(token.clone(), bus.clone());

        let url = format!("ws://{addr}");
        let req = url.into_client_request().unwrap();
        let (mut ws, _) = connect_async(req).await.unwrap();

        let hello_json = serde_json::json!({
            "protocolVersion": PROTOCOL_VERSION,
            "token": token,
            "clientName": "test",
            "lastSeq": 0
        });
        ws.send(WsMessage::Text(hello_json.to_string())).await.unwrap();

        let reply = ws.next().await.unwrap().unwrap();
        let reply: Outbound = serde_json::from_str(&reply.to_text().unwrap()).unwrap();
        let Outbound::HelloReply(r) = reply else { panic!("expected hello reply") };
        assert_eq!(r.protocol_version, PROTOCOL_VERSION);
        assert_eq!(r.last_seq, bus.last_seq());

        let req_json = serde_json::json!({ "id": 1, "command": { "method": "modelList" } });
        ws.send(WsMessage::Text(req_json.to_string())).await.unwrap();
        let resp = ws.next().await.unwrap().unwrap();
        let resp: Outbound = serde_json::from_str(&resp.to_text().unwrap()).unwrap();
        let Outbound::Response(r) = resp else { panic!("expected response") };
        assert_eq!(r.id, 1);
        assert!(r.error.is_none());
        assert!(r.result.is_some());

        ws.send(WsMessage::Close(None)).await.unwrap();
        srv.await.unwrap();
    }

    /// Event emitted after the handshake is delivered live.
    #[tokio::test]
    async fn live_event_delivery() {
        let token = crate::auth::generate_token();
        let bus = Arc::new(EventBus::new());
        let (addr, srv) = spawn_server(token.clone(), bus.clone());

        let url = format!("ws://{addr}");
        let req = url.into_client_request().unwrap();
        let (mut ws, _) = connect_async(req).await.unwrap();

        let hello_json = serde_json::json!({
            "protocolVersion": PROTOCOL_VERSION,
            "token": token,
            "clientName": "test",
            "lastSeq": 0
        });
        ws.send(WsMessage::Text(hello_json.to_string())).await.unwrap();
        let _reply = ws.next().await.unwrap().unwrap();

        bus.emit(Event::Status { seq: 0, session_id: None, text: "hello".into() });
        let ev = ws.next().await.unwrap().unwrap();
        let ev: Outbound = serde_json::from_str(&ev.to_text().unwrap()).unwrap();
        let Outbound::Event(Event::Status { text, .. }) = ev else { panic!("expected status event") };
        assert_eq!(text, "hello");

        ws.send(WsMessage::Close(None)).await.unwrap();
        srv.await.unwrap();
    }

    /// Reject a bad token.
    #[tokio::test]
    async fn rejects_bad_token() {
        let token = crate::auth::generate_token();
        let bus = Arc::new(EventBus::new());
        let (addr, srv) = spawn_server(token, bus);

        let url = format!("ws://{addr}");
        let req = url.into_client_request().unwrap();
        let (mut ws, _) = connect_async(req).await.unwrap();

        let hello_json = serde_json::json!({
            "protocolVersion": PROTOCOL_VERSION,
            "token": "wrong-token",
            "clientName": "test"
        });
        ws.send(WsMessage::Text(hello_json.to_string())).await.unwrap();
        let resp = ws.next().await.unwrap().unwrap();
        let resp: Outbound = serde_json::from_str(&resp.to_text().unwrap()).unwrap();
        let Outbound::Response(r) = resp else { panic!("expected response") };
        assert!(r.error.is_some());
        assert_eq!(r.error.unwrap().code, -32002);
        srv.await.unwrap();
    }

    /// Reject a protocol mismatch.
    #[tokio::test]
    async fn rejects_protocol_mismatch() {
        let token = crate::auth::generate_token();
        let bus = Arc::new(EventBus::new());
        let (addr, srv) = spawn_server(token.clone(), bus);

        let url = format!("ws://{addr}");
        let req = url.into_client_request().unwrap();
        let (mut ws, _) = connect_async(req).await.unwrap();

        let hello_json = serde_json::json!({
            "protocolVersion": 999,
            "token": token,
            "clientName": "test"
        });
        ws.send(WsMessage::Text(hello_json.to_string())).await.unwrap();
        let resp = ws.next().await.unwrap().unwrap();
        let resp: Outbound = serde_json::from_str(&resp.to_text().unwrap()).unwrap();
        let Outbound::Response(r) = resp else { panic!("expected response") };
        assert!(r.error.is_some());
        assert_eq!(r.error.unwrap().code, -32001);
        srv.await.unwrap();
    }
}
