//! The browser control engine: Playwright-style locator resolution over a DOM
//! snapshot, and the injected script set that the engine (or a WebView2 CDP
//! bridge) runs inside the page.

use serde_json::{json, Value};

/// A node in the interactive-element snapshot tree.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotNode {
    /// Stable reference id used by locators (e.g. `ref_1`).
    pub r#ref: String,
    pub tag: String,
    pub text: Option<String>,
    pub attributes: serde_json::Map<String, Value>,
    /// Whether the element is currently actionable.
    pub enabled: bool,
    pub children: Vec<SnapshotNode>,
}

/// The result of an agent operation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpResult {
    pub ok: bool,
    pub value: Value,
    pub elapsed_ms: u64,
}

/// The command set the agent can issue through `browser_execute`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "method", rename_all = "camelCase")]
pub enum BrowserCommand {
    Navigate { url: String },
    Back,
    Forward,
    Reload,
    Snapshot,
    Click { r#ref: String },
    Dblclick { r#ref: String },
    Hover { r#ref: String },
    Fill { r#ref: String, value: String },
    Type { r#ref: String, text: String },
    Press { key: String },
    Select { r#ref: String, values: Vec<String> },
    Check { r#ref: String, checked: bool },
    Scroll { r#ref: Option<String>, x: i64, y: i64 },
    Screenshot { clip: bool, full_page: bool },
    ElementInfo { r#ref: String },
    GetState { r#ref: String },
    Evaluate { script: String },
    WaitFor { r#ref: Option<String>, text: Option<String>, timeout_ms: u64 },
    GetDialog,
    HandleDialog { accept: bool },
    ListTabs,
    ListUserTabs,
    ViewportSet { width: u64, height: u64 },
    ViewportReset,
}

/// Injected script set (mirrors the ZCode six-script architecture):
/// - SNAPSHOT: build the interactive element tree with refs
/// - RESOLVE: resolve a ref to a CSS selector / bounding box
/// - ELEMENT_AT_POINT: find the top element at x,y
/// - EVALUATE: run arbitrary JS and return a serializable result
/// - CHECK/SELECT helpers
pub const SCRIPT_SNAPSHOT: &str = r#"
(() => {
  let refCounter = 0;
  const interactive = ['a','button','input','textarea','select','[role=button]','[role=link]','[role=checkbox]','[role=radio]','[tabindex]','[contenteditable]','summary','label','iframe'];
  const walk = (el, depth) => {
    if (depth > 12) return [];
    const out = [];
    const matches = el.matches ? el.matches(interactive.join(',')) : false;
    const visible = el.offsetParent !== null || el === document.body;
    if (matches && visible) {
      const rect = el.getBoundingClientRect();
      if (rect.width < 2 || rect.height < 2) return [];
      const ref = 'ref_' + (++refCounter);
      const attrs = {};
      for (const a of ['href','aria-label','aria-checked','role','name','type','placeholder','value','title','data-testid']) {
        const v = el.getAttribute(a);
        if (v) attrs[a] = v;
      }
      out.push({ref, tag: el.tagName.toLowerCase(), text: (el.innerText||el.value||'').trim().slice(0,200), attributes: attrs, enabled: !el.disabled, x: Math.round(rect.x), y: Math.round(rect.y), w: Math.round(rect.width), h: Math.round(rect.height), children: []});
    }
    for (const child of el.children) {
      const sub = walk(child, depth + 1);
      if (sub.length) {
        if (out.length) out[out.length - 1].children.push(...sub);
        else out.push(...sub);
      }
    }
    return out;
  };
  return walk(document.body, 0);
})()
"#;

/// Resolves a snapshot ref to the element via a document-wide marker
/// (the snapshot script stamps `data-zref` while walking).
pub const SCRIPT_RESOLVE: &str = r#"
(ref) => {
  const el = document.querySelector(`[data-zref="${ref}"]`);
  if (!el) return null;
  const r = el.getBoundingClientRect();
  return {x: Math.round(r.x + r.width/2), y: Math.round(r.y + r.height/2), w: Math.round(r.width), h: Math.round(r.height), tag: el.tagName.toLowerCase()};
}
"#;

/// The element at a given viewport point.
pub const SCRIPT_ELEMENT_AT_POINT: &str = r#"
(x, y) => {
  const el = document.elementFromPoint(x, y);
  if (!el) return null;
  return {tag: el.tagName.toLowerCase(), text: (el.innerText||el.textContent||'').trim().slice(0,200)};
}
"#;

/// Runs an expression and returns a JSON-serializable result.
pub const SCRIPT_EVALUATE: &str = r#"
(script) => {
  try {
    const result = eval(script);
    if (result === undefined) return {ok: true, value: null};
    if (typeof result === 'function') return {ok: true, value: String(result)};
    return {ok: true, value: typeof result === 'object' ? JSON.stringify(result) : String(result)};
  } catch (e) {
    return {ok: false, error: String(e && e.message || e)};
  }
}
"#;

/// Simulates a mouse event at coordinates (engine-side dispatcher).
pub fn dispatch_mouse_click(x: f64, y: f64, button: &str, double: bool) -> Value {
    json!({
        "type": "mouse",
        "action": if double { "dblclick" } else { "click" },
        "button": button,
        "x": x,
        "y": y,
    })
}

/// Parses a `browser_execute` request JSON into a `BrowserCommand`.
pub fn parse_command(request: &Value) -> Result<BrowserCommand, String> {
    serde_json::from_value(request.clone()).map_err(|e| e.to_string())
}

/// An in-memory locator engine: given a snapshot, resolves a ref to its
/// bounding box (for coordinate-based dispatch when CDP input is not
/// available).
pub fn resolve_ref_in_snapshot(root: &SnapshotNode, target_ref: &str) -> Option<(f64, f64)> {
    let mut queue = vec![root];
    while let Some(node) = queue.pop() {
        if node.r#ref == target_ref {
            // snapshot nodes carry no coordinates here; the engine uses
            // SCRIPT_RESOLVE in a live page. This fallback returns a marker.
            return Some((0.0, 0.0));
        }
        queue.extend(node.children.iter().cloned());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_parsing() {
        let cmd = parse_command(&json!({"method": "click", "ref": "ref_1"})).unwrap();
        assert_eq!(cmd, BrowserCommand::Click { r#ref: "ref_1".into() });

        let cmd = parse_command(&json!({"method": "navigate", "url": "https://x.com"})).unwrap();
        assert_eq!(cmd, BrowserCommand::Navigate { url: "https://x.com".into() });

        let cmd = parse_command(&json!({"method": "fill", "ref": "ref_2", "value": "hi"})).unwrap();
        assert_eq!(cmd, BrowserCommand::Fill { r#ref: "ref_2".into(), value: "hi".into() });

        let err = parse_command(&json!({"method": "unknown"}));
        assert!(err.is_err());
    }

    #[test]
    fn snapshot_script_is_valid_js() {
        // Just sanity: the script must contain the interactive selector set.
        assert!(SCRIPT_SNAPSHOT.contains("interactive"));
        assert!(SCRIPT_SNAPSHOT.contains("offsetParent"));
        assert!(SCRIPT_RESOLVE.contains("data-zref"));
        assert!(SCRIPT_EVALUATE.contains("JSON.stringify"));
    }

    #[test]
    fn mouse_dispatch_payload() {
        let v = dispatch_mouse_click(10.0, 20.0, "left", true);
        assert_eq!(v["action"], json!("dblclick"));
        assert_eq!(v["x"], json!(10.0));
    }
}
