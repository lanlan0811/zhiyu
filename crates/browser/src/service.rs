//! The embedded-browser service: combines tab lifecycle management with the
//! control engine. The WebView2 bridge (M7, Tauri side) plugs into
//! `BrowserService::execute` — in this crate the engine runs in-memory so
//! the tool surface and its unit tests work without a real webview.

use std::sync::Mutex;

use serde_json::{json, Value};
use uuid::Uuid;

use crate::engine::{BrowserCommand, OpResult};
use crate::tabs::{Tab, TabManager, TabOrigin};

/// A navigation + action log entry (for the UI and agent observability).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserLogEntry {
    pub tab_id: Uuid,
    pub method: String,
    pub ok: bool,
    pub value: Value,
}

/// The browser service handle shared by sessions.
pub struct BrowserService {
    tabs: Mutex<TabManager>,
    log: Mutex<Vec<BrowserLogEntry>>,
}

impl Default for BrowserService {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserService {
    pub fn new() -> Self {
        BrowserService { tabs: Mutex::new(TabManager::new()), log: Mutex::new(Vec::new()) }
    }

    pub fn open_user_tab(&self, url: &str) -> Uuid {
        self.tabs.lock().unwrap().create(url, TabOrigin::User, None)
    }

    pub fn open_agent_tab(&self, url: &str, session: Uuid) -> Uuid {
        self.tabs.lock().unwrap().create(url, TabOrigin::Agent, Some(session))
    }

    pub fn close_tab(&self, id: Uuid) {
        self.tabs.lock().unwrap().close(id);
    }

    pub fn tabs(&self) -> Vec<Tab> {
        self.tabs.lock().unwrap().tabs()
    }

    pub fn user_tabs(&self) -> Vec<Tab> {
        self.tabs.lock().unwrap().user_tabs()
    }

    pub fn release_to_user(&self, id: Uuid) -> bool {
        self.tabs.lock().unwrap().release_to_user(id)
    }

    /// Executes a browser command for a session. In the desktop app the
    /// WebView2 bridge handles the actual page interaction; the in-memory
    /// engine here returns deterministic results for the tool surface and
    /// tests.
    pub fn execute(&self, session: Uuid, command: BrowserCommand) -> OpResult {
        let started = std::time::Instant::now();
        let (method, value, ok) = self.apply(session, &command);
        let result = OpResult { ok, value, elapsed_ms: started.elapsed().as_millis() as u64 };
        self.log.lock().unwrap().push(BrowserLogEntry {
            tab_id: self.tabs.lock().unwrap().active().unwrap_or_default(),
            method,
            ok,
            value: result.value.clone(),
        });
        result
    }

    fn apply(&self, session: Uuid, command: &BrowserCommand) -> (String, Value, bool) {
        let mut tabs = self.tabs.lock().unwrap();
        let active = tabs.active();
        match command {
            BrowserCommand::Navigate { url } => {
                let id = match active {
                    Some(id) if tabs.get(id).is_some() => id,
                    _ => tabs.create(url, TabOrigin::Agent, Some(session)),
                };
                if let Some(tab) = tabs.get_mut(id) {
                    tab.url = url.clone();
                }
                ("navigate", json!({ "tabId": id.to_string(), "url": url }), true)
            }
            BrowserCommand::Snapshot => {
                let tree = crate::engine::SnapshotNode {
                    r#ref: "ref_0".into(),
                    tag: "body".into(),
                    text: Some("snapshot (in-memory)".into()),
                    attributes: Default::default(),
                    enabled: true,
                    children: vec![],
                };
                ("snapshot", json!({ "tree": tree, "tabId": active.map(|i| i.to_string()) }), true)
            }
            BrowserCommand::Click { r#ref } | BrowserCommand::Dblclick { r#ref } | BrowserCommand::Hover { r#ref } => {
                ("click", json!({ "ref": r#ref, "resolved": true }), true)
            }
            BrowserCommand::Fill { r#ref, value } => {
                ("fill", json!({ "ref": r#ref, "value": value }), true)
            }
            BrowserCommand::Evaluate { script } => {
                ("evaluate", json!({ "result": format!("evaluated: {script}") }), true)
            }
            BrowserCommand::ListTabs => ("listTabs", json!(tabs.tabs()), true),
            BrowserCommand::ListUserTabs => ("listUserTabs", json!(tabs.user_tabs()), true),
            BrowserCommand::ViewportSet { width, height } => {
                ("viewportSet", json!({ "width": width, "height": height }), true)
            }
            _ => (command_method_name(command), json!({ "ok": true, "note": "in-memory engine" }), true),
        }
    }

    /// Recent action log.
    pub fn log(&self) -> Vec<BrowserLogEntry> {
        self.log.lock().unwrap().clone()
    }
}

fn command_method_name(command: &BrowserCommand) -> String {
    match command {
        BrowserCommand::Navigate { .. } => "navigate",
        BrowserCommand::Back => "back",
        BrowserCommand::Forward => "forward",
        BrowserCommand::Reload => "reload",
        BrowserCommand::Snapshot => "snapshot",
        BrowserCommand::Click { .. } => "click",
        BrowserCommand::Dblclick { .. } => "dblclick",
        BrowserCommand::Hover { .. } => "hover",
        BrowserCommand::Fill { .. } => "fill",
        BrowserCommand::Type { .. } => "type",
        BrowserCommand::Press { .. } => "press",
        BrowserCommand::Select { .. } => "select",
        BrowserCommand::Check { .. } => "check",
        BrowserCommand::Scroll { .. } => "scroll",
        BrowserCommand::Screenshot { .. } => "screenshot",
        BrowserCommand::ElementInfo { .. } => "elementInfo",
        BrowserCommand::GetState { .. } => "getState",
        BrowserCommand::Evaluate { .. } => "evaluate",
        BrowserCommand::WaitFor { .. } => "waitFor",
        BrowserCommand::GetDialog => "getDialog",
        BrowserCommand::HandleDialog { .. } => "handleDialog",
        BrowserCommand::ListTabs => "listTabs",
        BrowserCommand::ListUserTabs => "listUserTabs",
        BrowserCommand::ViewportSet { .. } => "viewportSet",
        BrowserCommand::ViewportReset => "viewportReset",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_user_and_agent_tabs() {
        let svc = BrowserService::new();
        let user = svc.open_user_tab("https://example.com");
        let session = Uuid::new_v4();
        let agent = svc.open_agent_tab("https://dev.local", session);
        assert_eq!(svc.user_tabs().len(), 1);
        assert_eq!(svc.tabs().len(), 2);
        svc.close_tab(agent);
        assert_eq!(svc.tabs().len(), 1);
        let _ = user;
    }

    #[test]
    fn execute_navigate_and_snapshot() {
        let svc = BrowserService::new();
        let session = Uuid::new_v4();
        let res = svc.execute(session, BrowserCommand::Navigate { url: "https://example.com".into() });
        assert!(res.ok);
        assert_eq!(res.value["url"], json!("https://example.com"));

        let res = svc.execute(session, BrowserCommand::Snapshot);
        assert!(res.ok);
        assert!(res.value.get("tree").is_some());
    }

    #[test]
    fn execute_logs_actions() {
        let svc = BrowserService::new();
        let session = Uuid::new_v4();
        svc.execute(session, BrowserCommand::Navigate { url: "https://a.com".into() });
        svc.execute(session, BrowserCommand::Click { r#ref: "ref_1".into() });
        let log = svc.log();
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].method, "navigate");
        assert_eq!(log[1].method, "click");
    }

    #[test]
    fn agent_control_supported() {
        assert!(crate::supports_agent_control());
    }
}
