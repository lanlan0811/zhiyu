//! Thought-level patch system: applies a `RequestPatch` (path set/unset) to a
//! JSON request body, mirroring the ZCode provider-options patch semantics.
//!
//! For the OpenAI dual protocols the built-in catalogues supply:
//! - chat:      `reasoning_effort` (top-level string)
//! - responses: `reasoning.effort` (nested object)

use serde_json::{json, Value};
use zhiyu_protocol::{RequestPatch, ThoughtLevel};

/// Applies a patch to `body` in place.
///
/// `set` paths are created if missing (objects for intermediate keys, the
/// leaf takes the given value). `unset` paths remove the leaf (and empty
/// parents) from the body.
pub fn apply_patch(body: &mut Value, patch: &RequestPatch) {
    for pv in &patch.set {
        set_path(body, &pv.path, pv.value.clone());
    }
    for path in &patch.unset {
        unset_path(body, path);
    }
}

/// Builds the default chat-protocol patch for a level: `reasoning_effort`.
/// `off` removes the effort field entirely (reasoning disabled).
pub fn chat_patch(level: ThoughtLevel) -> RequestPatch {
    if level == ThoughtLevel::Off {
        return RequestPatch {
            set: vec![],
            unset: vec![vec!["reasoning_effort".into()]],
        };
    }
    RequestPatch {
        set: vec![zhiyu_protocol::PathValue {
            path: vec!["reasoning_effort".into()],
            value: json!(level.as_str()),
        }],
        unset: vec![],
    }
}

/// Builds the default responses-protocol patch for a level:
/// `reasoning.effort`. `off` removes the reasoning block entirely.
pub fn responses_patch(level: ThoughtLevel) -> RequestPatch {
    if level == ThoughtLevel::Off {
        return RequestPatch {
            set: vec![],
            unset: vec![vec!["reasoning".into(), "effort".into()]],
        };
    }
    RequestPatch {
        set: vec![zhiyu_protocol::PathValue {
            path: vec!["reasoning".into(), "effort".into()],
            value: json!(level.as_str()),
        }],
        unset: vec![],
    }
}

fn set_path(body: &mut Value, path: &[String], value: Value) {
    if path.is_empty() {
        *body = value;
        return;
    }
    let obj = body.as_object_mut().expect("set path needs an object root");
    set_path_in(obj, path, value);
}

fn set_path_in(obj: &mut serde_json::Map<String, Value>, path: &[String], value: Value) {
    let (head, rest) = path.split_first().expect("non-empty path");
    if rest.is_empty() {
        obj.insert(head.clone(), value);
        return;
    }
    let entry = obj.entry(head.clone()).or_insert_with(|| json!({}));
    // force the intermediate node into an object
    if !entry.is_object() {
        *entry = json!({});
    }
    set_path_in(entry.as_object_mut().expect("intermediate is object"), rest, value);
}

fn unset_path(body: &mut Value, path: &[String]) {
    if path.is_empty() {
        return;
    }
    let Some(obj) = body.as_object_mut() else { return };
    unset_path_in(obj, path);
}

fn unset_path_in(obj: &mut serde_json::Map<String, Value>, path: &[String]) {
    let (head, rest) = path.split_first().expect("non-empty path");
    if rest.is_empty() {
        obj.remove(head);
        return;
    }
    if let Some(child) = obj.get_mut(head).and_then(|c| c.as_object_mut()) {
        unset_path_in(child, rest);
        if child.is_empty() {
            obj.remove(head);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zhiyu_protocol::PathValue;

    #[test]
    fn chat_effort_patch() {
        let mut body = json!({"model": "m", "messages": []});
        let patch = chat_patch(ThoughtLevel::High);
        apply_patch(&mut body, &patch);
        assert_eq!(body["reasoning_effort"], json!("high"));
        assert_eq!(body["model"], json!("m"));
    }

    #[test]
    fn responses_effort_patch_nests() {
        let mut body = json!({"model": "m"});
        let patch = responses_patch(ThoughtLevel::Max);
        apply_patch(&mut body, &patch);
        assert_eq!(body["reasoning"]["effort"], json!("max"));
        assert_eq!(body["model"], json!("m"));
    }

    #[test]
    fn unset_removes_and_prunes_empty_parents() {
        let mut body = json!({"reasoning": {"effort": "high", "budget": 3}, "model": "m"});
        let patch = RequestPatch { set: vec![], unset: vec![vec!["reasoning".into(), "effort".into()]] };
        apply_patch(&mut body, &patch);
        // effort removed; reasoning.budget still present
        assert_eq!(body["reasoning"]["budget"], json!(3));
        assert!(body["reasoning"].get("effort").is_none());

        let patch = RequestPatch { set: vec![], unset: vec![vec!["reasoning".into(), "budget".into()]] };
        apply_patch(&mut body, &patch);
        // reasoning emptied → pruned entirely
        assert!(body.get("reasoning").is_none());
        assert_eq!(body["model"], json!("m"));
    }

    #[test]
    fn set_creates_missing_intermediates() {
        let mut body = json!({});
        let patch = RequestPatch {
            set: vec![PathValue { path: vec!["a".into(), "b".into(), "c".into()], value: json!(1) }],
            unset: vec![],
        };
        apply_patch(&mut body, &patch);
        assert_eq!(body["a"]["b"]["c"], json!(1));
    }

    #[test]
    fn off_level_unsets_effort() {
        let mut body = json!({"reasoning": {"effort": "high"}});
        let patch = RequestPatch {
            set: vec![],
            unset: vec![vec!["reasoning".into(), "effort".into()]],
        };
        apply_patch(&mut body, &patch);
        assert!(body.get("reasoning").is_none());
    }
}
