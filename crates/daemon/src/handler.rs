//! The daemon's request handler: routes every `Command` to the core services
//! and emits events on the bus. This is the heart of the local control plane.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use zhiyu_browser::BrowserService;
use zhiyu_core::kb::KnowledgeBase;
use zhiyu_core::keyring::KeyStore;
use zhiyu_core::model_store::ModelStore;
use zhiyu_core::sessions::SessionManager;
use zhiyu_core::settings::{default_thought_level_for, load_settings, patch_settings, save_settings};
use zhiyu_core::thought_runtime::switch_model_config;
use zhiyu_core::Store;
use crate::RequestHandler;
use crate::event_bus::EventBus;
use zhiyu_protocol::{Command, ErrorInfo, Mode, Settings};

/// All the state the daemon serves. One instance per daemon process.
pub struct AppState {
    pub store: Mutex<Store>,
    pub sessions: SessionManager,
    pub models: ModelStore,
    pub keys: KeyStore,
    pub kb: KnowledgeBase,
    pub browser: BrowserService,
    pub settings: Mutex<Settings>,
    pub bus: Arc<EventBus>,
}

impl AppState {
    /// Opens everything under `~/.zhiyu` (injectable root for tests).
    pub fn open(root: Option<&PathBuf>) -> anyhow::Result<AppState> {
        let root = root.cloned().unwrap_or_else(|| zhiyu_core::paths::data_dir());
        std::fs::create_dir_all(&root)?;
        let db_path = root.join("zhiyu.db");
        let store = Mutex::new(Store::open(&db_path)?);
        let sessions = SessionManager::new(Store::open(&db_path)?);
        let models = ModelStore::new();
        let keys = KeyStore::new();
        let kb = KnowledgeBase::open(&root.join("knowledge"))?;
        let browser = BrowserService::new();
        let settings = Mutex::new(load_settings(None));
        let bus = Arc::new(EventBus::new());
        Ok(AppState { store, sessions, models, keys, kb, browser, settings, bus })
    }
}

fn err(code: i32, message: impl Into<String>) -> ErrorInfo {
    ErrorInfo { code, message: message.into() }
}

fn internal(e: impl std::fmt::Display) -> ErrorInfo {
    err(-32603, format!("internal error: {e}"))
}

/// The JSON-RPC command handler bound to an `AppState`.
pub struct CoreHandler {
    pub state: Arc<AppState>,
}

#[async_trait]
impl RequestHandler for CoreHandler {
    async fn handle(&self, command: Command) -> Result<serde_json::Value, ErrorInfo> {
        let state = self.state.clone();
        match command {
            // ---- sessions --------------------------------------------------
            Command::SessionList { mode } => {
                let rows = state.sessions.list(mode).map_err(internal)?;
                Ok(serde_json::to_value(rows).map_err(internal)?)
            }
            Command::SessionCreate { mode, title, workspace_dir } => {
                let row = state.sessions.create(mode, title.as_deref(), workspace_dir.as_deref()).map_err(internal)?;
                state.bus.emit(zhiyu_protocol::Event::SessionChanged {
                    seq: 0,
                    session_id: row.id,
                    mode,
                });
                Ok(serde_json::to_value(row).map_err(internal)?)
            }
            Command::SessionOpen { session_id } => {
                // resolve the mode by probing both tables
                let row = state
                    .sessions
                    .open(Mode::Daily, session_id)
                    .map_err(internal)?
                    .or(state.sessions.open(Mode::Coding, session_id).map_err(internal)?)
                    .ok_or_else(|| err(-32010, "session not found"))?;
                Ok(serde_json::to_value(row).map_err(internal)?)
            }
            Command::SessionDelete { session_id } => {
                // delete from both modes (idempotent)
                let _ = state.sessions.delete(Mode::Daily, session_id);
                let _ = state.sessions.delete(Mode::Coding, session_id);
                Ok(serde_json::json!({ "ok": true }))
            }
            Command::SessionSend { session_id, text, thought_level } => {
                // find which mode the session belongs to
                let mode = find_mode(&state, session_id).ok_or_else(|| err(-32010, "session not found"))?;
                state.sessions.append_message(mode, session_id, zhiyu_protocol::Role::User, &text, None, None).map_err(internal)?;
                state.sessions.enqueue_turn(session_id, text).map_err(internal)?;
                if let Some(level) = thought_level {
                    state.sessions.set_thought_level(session_id, level).map_err(internal)?;
                }
                // kick off the turn runner (fire-and-forget; events stream back)
                let state = state.clone();
                tokio::spawn(async move {
                    if let Err(e) = run_turn(&state, session_id, mode).await {
                        state.bus.emit(zhiyu_protocol::Event::Status {
                            seq: 0,
                            session_id: Some(session_id),
                            text: format!("turn error: {e}"),
                        });
                    }
                });
                Ok(serde_json::json!({ "ok": true }))
            }
            Command::SessionResume { cursor } => {
                let msgs = state.sessions.resume(cursor).map_err(internal)?;
                Ok(serde_json::to_value(msgs).map_err(internal)?)
            }
            Command::SessionStop { session_id } => {
                // best-effort: mark the session not-streaming so the runner
                // observes the stop on its next step
                state.sessions.set_streaming(session_id, false);
                Ok(serde_json::json!({ "ok": true }))
            }
            Command::SessionSteer { session_id, text } => {
                state.sessions.enqueue_turn(session_id, text).map_err(internal)?;
                Ok(serde_json::json!({ "ok": true }))
            }

            // ---- models & keys ---------------------------------------------
            Command::ModelList => {
                let (_file, models) = state.models.load_models();
                Ok(serde_json::to_value(models).map_err(internal)?)
            }
            Command::ModelSave { config } => {
                let (mut file, _) = state.models.load_models();
                state.models.save_model(&mut file, config).map_err(internal)?;
                Ok(serde_json::json!({ "ok": true }))
            }
            Command::ModelDelete { model_id } => {
                let (mut file, _) = state.models.load_models();
                state.models.delete_model(&mut file, &model_id).map_err(internal)?;
                Ok(serde_json::json!({ "ok": true }))
            }
            Command::KeyList { provider } => {
                let keys = match provider {
                    Some(p) => state.keys.load(&p).ok().into_iter().collect::<Vec<_>>(),
                    None => {
                        // list all known providers
                        let mut all = Vec::new();
                        for p in ["deepseek", "glm"] {
                            if let Ok(k) = state.keys.load(p) {
                                all.push(k);
                            }
                        }
                        all
                    }
                };
                Ok(serde_json::to_value(keys).map_err(internal)?)
            }
            Command::KeySave { provider, key } => {
                let keys = state.keys.upsert_key(&provider, &key).map_err(internal)?;
                Ok(serde_json::to_value(keys).map_err(internal)?)
            }
            Command::KeyDelete { provider, key_id } => {
                state.keys.delete_key(&provider, &key_id).map_err(internal)?;
                Ok(serde_json::json!({ "ok": true }))
            }
            Command::KeySetDefault { provider, key_id } => {
                state.keys.set_default(&provider, &key_id).map_err(internal)?;
                Ok(serde_json::json!({ "ok": true }))
            }

            // ---- thought level ----------------------------------------------
            Command::SessionSetThoughtLevel { session_id, level } => {
                state.sessions.set_thought_level(session_id, level).map_err(internal)?;
                Ok(serde_json::json!({ "ok": true }))
            }
            Command::SettingsSetDefaultThoughtLevel { mode, level } => {
                let mut settings = load_settings(None);
                match mode {
                    Some(m) => {
                        settings.mode_thought_level.insert(m, level);
                    }
                    None => {
                        settings.default_thought_level = level;
                    }
                }
                save_settings(None, &settings).map_err(internal)?;
                *state.settings.lock().unwrap() = settings.clone();
                Ok(serde_json::to_value(settings).map_err(internal)?)
            }

            // ---- context management ----------------------------------------
            Command::SessionContextUsage { session_id } => {
                let usage = context_usage(&state, session_id).map_err(internal)?;
                Ok(serde_json::to_value(usage).map_err(internal)?)
            }
            Command::SessionCompact { session_id, .. } => {
                // summary-style compaction: keep the recent tail, prefix a
                // summary system message (the model call for the summary is a
                // follow-up; the marker is inserted now).
                let mode = find_mode(&state, session_id).ok_or_else(|| err(-32010, "session not found"))?;
                let msgs = state.sessions.messages(session_id).map_err(internal)?;
                let keep_last = 8usize.min(msgs.len());
                let summary = format!("（共 {} 条历史消息已压缩）", msgs.len().saturating_sub(keep_last));
                let plan = zhiyu_context::plan_compaction(&msgs, summary, keep_last, "manual", 0, 0);
                state.sessions.truncate(session_id, (msgs.len().saturating_sub(keep_last)) as u64).map_err(internal)?;
                let compacted = zhiyu_context::compacted_transcript(&plan, &msgs, session_id);
                for m in compacted {
                    state.sessions.append_message(mode, session_id, m.role, &m.content, m.reasoning.as_deref(), m.tool_name.as_deref()).map_err(internal)?;
                }
                state.bus.emit(zhiyu_protocol::Event::ContextCompacted {
                    seq: 0,
                    session_id,
                    trigger: plan.separator.trigger.clone(),
                    pre_compact_tokens: plan.separator.pre_compact_token_count,
                    post_compact_tokens: plan.separator.post_compact_token_count,
                });
                Ok(serde_json::to_value(plan.separator).map_err(internal)?)
            }
            Command::ModelSwitchGuard { session_id, model_id } => {
                let used = context_usage(&state, session_id).map_err(internal)?.used_tokens;
                let (_file, models) = state.models.load_models();
                let target = models.into_iter().find(|m| m.id == model_id).ok_or_else(|| err(-32011, "model not found"))?;
                let busy = state.sessions.is_streaming(session_id);
                let result = zhiyu_context::evaluate_switch(used, &target, busy);
                // when allowed, sanitize the session's thought level against
                // the target model's declared levels (CAS switch semantics)
                if matches!(result, zhiyu_context::GuardResult::Ok { .. }) {
                    let mode = find_mode(&state, session_id);
                    let level = state.sessions.take_thought_level(session_id, default_thought_level_for(&load_settings(None), mode.unwrap_or(Mode::Daily)));
                    let _sw = switch_model_config(None, &target, level);
                }
                Ok(serde_json::to_value(result).map_err(internal)?)
            }

            // ---- knowledge base ----------------------------------------------
            Command::KnowledgeSearch { query, limit } => {
                let hits = state.kb.search(&query, limit.unwrap_or(5) as usize).map_err(internal)?;
                Ok(serde_json::to_value(hits).map_err(internal)?)
            }
            Command::KnowledgeImport { path } => {
                let doc = state.kb.import_file(&PathBuf::from(path)).map_err(internal)?;
                Ok(serde_json::to_value(doc).map_err(internal)?)
            }
            Command::KnowledgeList => {
                let docs = state.kb.list().map_err(internal)?;
                Ok(serde_json::to_value(docs).map_err(internal)?)
            }
            Command::KnowledgeDelete { doc_id } => {
                state.kb.delete(doc_id).map_err(internal)?;
                Ok(serde_json::json!({ "ok": true }))
            }
            Command::KnowledgeReindex => {
                let count = state.kb.reindex(&zhiyu_core::paths::knowledge_dir()).map_err(internal)?;
                Ok(serde_json::json!({ "count": count }))
            }

            // ---- workspace (coding mode) ------------------------------------
            Command::WorkspaceOpen { session_id, dir } => {
                let _row = state.sessions.open(Mode::Coding, session_id).map_err(internal)?.ok_or_else(|| err(-32010, "session not found"))?;
                // note: the workspace dir binding is carried by the session
                // row; keeping it in sync on every open is a follow-up with
                // the full persistence layer
                let _ = dir;
                Ok(serde_json::json!({ "ok": true }))
            }
            Command::WorkspaceListDir { session_id, path } => {
                let root = workspace_root(&state, session_id)?;
                let entries = zhiyu_core::workspace::list_dir(&root, path.as_deref()).map_err(internal)?;
                Ok(serde_json::to_value(entries).map_err(internal)?)
            }
            Command::WorkspaceReadFile { session_id, path } => {
                let root = workspace_root(&state, session_id)?;
                let content = zhiyu_core::workspace::read_file(&root, &path).map_err(internal)?;
                Ok(serde_json::json!({ "content": content }))
            }
            Command::WorkspaceWriteFile { session_id, path, content } => {
                let root = workspace_root(&state, session_id)?;
                zhiyu_core::workspace::write_file(&root, &path, &content).map_err(internal)?;
                Ok(serde_json::json!({ "ok": true }))
            }
            Command::TerminalExec { session_id, command } => {
                let root = workspace_root(&state, session_id)?;
                let output = run_shell(&root, &command).map_err(internal)?;
                state.bus.emit(zhiyu_protocol::Event::Status {
                    seq: 0,
                    session_id: Some(session_id),
                    text: format!("$ {command}"),
                });
                Ok(serde_json::json!({ "output": output }))
            }
            Command::GitStatus { session_id } => {
                let root = workspace_root(&state, session_id)?;
                let out = zhiyu_core::git::diff(&root, "HEAD", "").map_err(internal)?;
                Ok(serde_json::json!({ "status": out }))
            }
            Command::GitCheckpoint { session_id, description } => {
                let root = workspace_root(&state, session_id)?;
                let cp_id = uuid::Uuid::new_v4();
                let sha = zhiyu_core::create_checkpoint(&root, session_id, cp_id, description.as_deref().unwrap_or("turn")).map_err(internal)?;
                let cp = state.store.lock().unwrap().save_checkpoint(session_id, &format!("refs/zhiyu/checkpoints/{}/{}", session_id.simple(), cp_id.simple()), description.as_deref().unwrap_or("turn")).map_err(internal)?;
                Ok(serde_json::json!({ "checkpoint": cp, "sha": sha }))
            }
            Command::GitRollback { session_id, checkpoint_id } => {
                let root = workspace_root(&state, session_id)?;
                let cp = state.store.lock().unwrap().get_checkpoint(checkpoint_id).map_err(internal)?.ok_or_else(|| err(-32012, "checkpoint not found"))?;
                zhiyu_core::rollback(&root, &cp.ref_name).map_err(internal)?;
                Ok(serde_json::json!({ "ok": true, "ref": cp.ref_name }))
            }
            Command::ReviewDiff { session_id } => {
                let root = workspace_root(&state, session_id)?;
                let diff = zhiyu_core::git::diff(&root, "HEAD", "").map_err(internal)?;
                Ok(serde_json::json!({ "prompt": zhiyu_core::git::review_prompt(&diff), "diff": diff }))
            }

            // ---- browser -----------------------------------------------------
            Command::BrowserExecute { session_id, request } => {
                let cmd = zhiyu_browser::engine::parse_command(&request).map_err(|e| err(-32013, e))?;
                let result = state.browser.execute(session_id, cmd);
                Ok(serde_json::to_value(result).map_err(internal)?)
            }

            // ---- writing (daily mode) ----------------------------------------
            Command::WritingRun { session_id, task } => {
                // assembles a writing prompt; the model call streams via the
                // turn runner in a follow-up turn
                let prompt = writing_prompt(&task);
                state.sessions.enqueue_turn(session_id, prompt).map_err(internal)?;
                Ok(serde_json::json!({ "ok": true }))
            }

            // ---- settings -----------------------------------------------------
            Command::SettingsGet => {
                let s = state.settings.lock().unwrap().clone();
                Ok(serde_json::to_value(s).map_err(internal)?)
            }
            Command::SettingsSet { patch } => {
                let s = patch_settings(None, patch).map_err(internal)?;
                *state.settings.lock().unwrap() = s.clone();
                Ok(serde_json::to_value(s).map_err(internal)?)
            }
        }
    }
}

/// Finds which mode a session belongs to.
fn find_mode(state: &AppState, session_id: uuid::Uuid) -> Option<Mode> {
    if state.sessions.list(Mode::Daily).ok()?.iter().any(|s| s.id == session_id) {
        return Some(Mode::Daily);
    }
    if state.sessions.list(Mode::Coding).ok()?.iter().any(|s| s.id == session_id) {
        return Some(Mode::Coding);
    }
    None
}

/// The workspace root for a coding session.
fn workspace_root(state: &AppState, session_id: uuid::Uuid) -> Result<PathBuf, ErrorInfo> {
    state
        .sessions
        .workspace_dir(Mode::Coding, session_id)
        .map_err(internal)?
        .map(PathBuf::from)
        .ok_or_else(|| err(-32014, "session has no workspace dir"))
}

/// Runs one queued turn: pops the text, resolves the session's model + key,
/// drives the model through the driver's SSE stream and fans events out to
/// the bus. Assistant text is accumulated and persisted on completion.
async fn run_turn(state: &AppState, session_id: uuid::Uuid, mode: Mode) -> anyhow::Result<()> {
    let Some(text) = state.sessions.pop_turn(session_id)? else {
        return Ok(());
    };
    state.sessions.set_streaming(session_id, true);

    // resolve model config: session-bound model, else mode default
    let (_file, models) = state.models.load_models();
    let default_model = zhiyu_core::settings::default_model_for(&load_settings(None), mode);
    let model_id = state.sessions.model_id(session_id, &default_model);
    let Some(model) = models.into_iter().find(|m| m.id == model_id) else {
        state.bus.emit(zhiyu_protocol::Event::Status {
            seq: 0,
            session_id: Some(session_id),
            text: format!("模型 {model_id} 未配置，请在模型设置中检查"),
        });
        state.sessions.set_streaming(session_id, false);
        return Ok(());
    };

    // resolve the API key from the keyring (provider-level)
    let provider = model.provider_key_id.clone().unwrap_or_else(|| "deepseek".into());
    let api_key = match state.keys.default_key(&provider) {
        Ok(k) => k,
        Err(_) => {
            state.bus.emit(zhiyu_protocol::Event::Status {
                seq: 0,
                session_id: Some(session_id),
                text: format!("provider {provider} 未配置 API-Key，请在模型设置中填写"),
            });
            state.sessions.set_streaming(session_id, false);
            return Ok(());
        }
    };

    // resolve the thought level (session override → mode default)
    let level = state.sessions.take_thought_level(session_id, zhiyu_core::settings::default_thought_level_for(&load_settings(None), mode));
    let level = model.reasoning.sanitize(level);

    // build the message list: the full transcript as driver ChatMessages
    let messages: Vec<zhiyu_driver::ChatMessage> = state
        .sessions
        .messages(session_id)?
        .into_iter()
        .map(|m| zhiyu_driver::ChatMessage {
            role: match m.role {
                zhiyu_protocol::Role::User => "user".into(),
                zhiyu_protocol::Role::Assistant => "assistant".into(),
                zhiyu_protocol::Role::System => "system".into(),
                zhiyu_protocol::Role::Tool => "tool".into(),
            },
            content: m.content,
            tool_call_id: m.tool_name,
            tool_calls: vec![],
        })
        .collect();

    // persist the user message + stream the assistant reply
    state.sessions.append_message(mode, session_id, zhiyu_protocol::Role::User, &text, None, None)?;

    let client = zhiyu_driver::default_client();
    let handle = zhiyu_driver::stream_completion(client, &model, &api_key, &messages, &[], level);

    let mut assistant_text = String::new();
    let mut reasoning_text = String::new();
    let mut usage: Option<zhiyu_protocol::Usage> = None;
    let mut rx = handle.rx;
    while let Some(chunk) = rx.recv().await {
        match chunk {
            zhiyu_driver::SseChunk::TextDelta(d) => {
                assistant_text.push_str(&d);
                state.bus.emit(zhiyu_protocol::Event::TextDelta { seq: 0, session_id, delta: d });
            }
            zhiyu_driver::SseChunk::ReasoningDelta(d) => {
                reasoning_text.push_str(&d);
                state.bus.emit(zhiyu_protocol::Event::ReasoningDelta { seq: 0, session_id, delta: d });
            }
            zhiyu_driver::SseChunk::Usage(u) => {
                usage = Some(u);
            }
            zhiyu_driver::SseChunk::Done | zhiyu_driver::SseChunk::ToolCallDelta { .. } | zhiyu_driver::SseChunk::ToolCallArgs { .. } => {}
        }
    }

    // report the task result (streaming error surfaces as a status event)
    if let Err(e) = handle.task.await {
        state.bus.emit(zhiyu_protocol::Event::Status {
            seq: 0,
            session_id: Some(session_id),
            text: format!("流式请求失败：{e}"),
        });
    }

    // persist the assistant message
    if !assistant_text.is_empty() {
        state.sessions.append_message(
            mode,
            session_id,
            zhiyu_protocol::Role::Assistant,
            &assistant_text,
            if reasoning_text.is_empty() { None } else { Some(&reasoning_text) },
            None,
        )?;
    }

    // usage event for the context manager
    if let Some(u) = usage {
        state.bus.emit(zhiyu_protocol::Event::UsageUpdate { seq: 0, session_id, usage: u });
    }

    let cursor = state.sessions.next_cursor(session_id)?;
    state.bus.emit(zhiyu_protocol::Event::TurnFinished { seq: 0, session_id, cursor });
    state.sessions.set_streaming(session_id, false);
    Ok(())
}

/// A small shell runner for TerminalExec (cross-platform: cmd on Windows,
/// sh elsewhere).
fn run_shell(root: &PathBuf, command: &str) -> Result<String, String> {
    #[cfg(windows)]
    let mut cmd = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", command]);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = std::process::Command::new("sh");
        c.args(["-c", command]);
        c
    };
    let out = cmd
        .current_dir(root)
        .output()
        .map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    Ok(format!("{stdout}{stderr}").trim().to_string())
}

/// The live context usage of a session (approximated from message count for
/// now; the driver usage feed wires the real tracker in M7).
fn context_usage(state: &AppState, session_id: uuid::Uuid) -> anyhow::Result<zhiyu_protocol::ContextUsage> {
    let msgs = state.sessions.messages(session_id)?;
    let mut tracker = zhiyu_context::UsageTracker::new();
    tracker.set_source(zhiyu_protocol::UsageSource::Messages, (msgs.len() * 400) as u64);
    let (_file, models) = state.models.load_models();
    let max = models
        .first()
        .map(|m| m.context_window)
        .unwrap_or(zhiyu_context::default_window());
    Ok(tracker.snapshot(max))
}

/// Builds the writing prompt for a daily-mode writing task.
pub fn writing_prompt(task: &zhiyu_protocol::WritingTask) -> String {
    let kind = match task.kind {
        zhiyu_protocol::WritingKind::Longform => "长文写作",
        zhiyu_protocol::WritingKind::Rewrite => "改写",
        zhiyu_protocol::WritingKind::Polish => "润色",
        zhiyu_protocol::WritingKind::Summarize => "摘要",
        zhiyu_protocol::WritingKind::Translate => "翻译",
        zhiyu_protocol::WritingKind::Outline => "大纲",
    };
    let mut prompt = format!("请帮我完成「{kind}」，主题：{}", task.topic);
    if let Some(len) = &task.length {
        prompt.push_str(&format!("，篇幅：{len}"));
    }
    if let Some(lang) = &task.language {
        prompt.push_str(&format!("，语言：{lang}"));
    }
    prompt
}
