use crate::{
    app_state::ManagedState,
    desktop_rpc::Client,
    models::{DesktopOperationView, DesktopSessionView, DesktopSyncSettings},
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use tauri::Emitter;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Operation {
    view: DesktopOperationView,
    settings: DesktopSyncSettings,
    target_home: PathBuf,
    shared_home: PathBuf,
    force: bool,
    #[serde(default)]
    automatic: bool,
    #[serde(default)]
    native_resume: bool,
    #[serde(default)]
    native_queued_at: Option<String>,
    sessions: Vec<Session>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Session {
    view: DesktopSessionView,
    host: String,
    app: String,
    previous_home: PathBuf,
    turn_id: String,
    thread: Value,
    config: Value,
    terminals: Value,
    interrupt_requested: bool,
    interrupted: bool,
    #[serde(default)]
    quota_failed: bool,
    send_state: String,
    resumed_turn_id: Option<String>,
    client_message_id: String,
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
fn path(state: &ManagedState, id: &str) -> Result<PathBuf> {
    uuid::Uuid::parse_str(id).context("Invalid recovery operation ID")?;
    Ok(state
        .store
        .state_path()
        .parent()
        .context("Missing app data directory")?
        .join("desktop-recovery")
        .join(format!("{id}.json")))
}
fn save(state: &ManagedState, op: &mut Operation, app: Option<&tauri::AppHandle>) -> Result<()> {
    use std::io::Write;
    op.view.updated_at = now();
    op.view.sessions = op.sessions.iter().map(|s| s.view.clone()).collect();
    let file = path(state, &op.view.id)?;
    let directory = file.parent().context("Missing recovery directory")?;
    std::fs::create_dir_all(directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))?;
    }
    let temp = directory.join(format!("{}.{}.tmp", op.view.id, uuid::Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut out = options.open(&temp)?;
    out.write_all(&serde_json::to_vec_pretty(op)?)?;
    out.sync_all()?;
    std::fs::rename(&temp, &file)?;
    std::fs::File::open(directory)?.sync_all()?;
    {
        let mut data = state
            .data
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        data.desktop_sync.operation = Some(op.view.clone());
        state.store.save(&data)?;
    }
    if let Some(app) = app {
        if let Ok(snapshot) = state.snapshot() {
            let _ = app.emit("snapshot-changed", snapshot);
        }
    }
    Ok(())
}
fn load(state: &ManagedState) -> Result<Option<Operation>> {
    let view = state
        .data
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?
        .desktop_sync
        .operation
        .clone();
    let Some(view) = view else {
        return Ok(None);
    };
    Ok(Some(serde_json::from_slice(&std::fs::read(path(
        state, &view.id,
    )?)?)?))
}

pub fn request(state: &ManagedState, settings: DesktopSyncSettings) -> Result<()> {
    request_kind(state, settings, false)
}
pub fn request_automatic(state: &ManagedState, settings: DesktopSyncSettings) -> Result<()> {
    request_kind(state, settings, true)
}

fn request_kind(
    state: &ManagedState,
    settings: DesktopSyncSettings,
    automatic: bool,
) -> Result<()> {
    let (account, target_home, shared_home) = state.desktop_selected_target()?;
    if let Some(mut prior) = load(state)? {
        anyhow::ensure!(
            !["checkpointing", "switching", "restoring", "retrySwitch"]
                .contains(&prior.view.phase.as_str())
                && (prior.native_queued_at.is_none()
                    || ["applied", "cancelled"].contains(&prior.view.phase.as_str())),
            "Finish or dismiss the pending recovery before another desktop switch"
        );
        if prior.sessions.iter().any(|s| {
            s.interrupt_requested && !["acknowledged", "dismissed"].contains(&s.send_state.as_str())
        }) {
            anyhow::bail!(
                "Recover or dismiss the interrupted sessions before another desktop switch"
            );
        }
        if prior.view.phase == "waiting"
            && prior.view.target_account_id == account.id
            && prior.target_home == target_home
            && prior.shared_home == shared_home
            && prior.settings.app == settings.app
            && prior.settings.enabled == settings.enabled
            && prior.settings.resume_active_sessions == settings.resume_active_sessions
        {
            if automatic && !prior.automatic {
                prior.automatic = true;
                return save(state, &mut prior, None);
            }
            return Ok(());
        }
        if prior.view.phase == "waiting" {
            prior.view.phase = "cancelled".into();
            prior.view.message = "Replaced by a newer account selection".into();
            save(state, &mut prior, None)?;
        }
    }
    anyhow::ensure!(
        account.api_provider.is_none(),
        "Desktop switching supports subscription profiles; API profiles use their CLI launcher"
    );
    let mut op = Operation {
        view: DesktopOperationView {
            id: uuid::Uuid::new_v4().to_string(),
            phase: "waiting".into(),
            message: "Waiting for a safe point before applying the desktop account".into(),
            target_account_id: account.id,
            ..Default::default()
        },
        settings: settings.clone(),
        automatic,
        target_home,
        shared_home,
        force: settings.resume_active_sessions
            && crate::desktop::supports_native_resume(&settings.app),
        native_resume: settings.resume_active_sessions
            && crate::desktop::supports_native_resume(&settings.app),
        native_queued_at: None,
        sessions: vec![],
    };
    {
        let mut data = state
            .data
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        data.desktop_sync.settings = settings;
        data.desktop_sync.error = None;
        state.store.save(&data)?;
    }
    save(state, &mut op, None)
}

pub fn update_settings(state: &ManagedState, settings: DesktopSyncSettings) -> Result<()> {
    if let Some(mut op) = load(state)? {
        anyhow::ensure!(
            !crate::app_state::operation_blocks_rotation(&op.view)
                && (op.native_queued_at.is_none()
                    || ["applied", "cancelled"].contains(&op.view.phase.as_str())),
            "Finish or dismiss recovery before changing desktop settings"
        );
        if op.view.phase == "waiting" {
            anyhow::ensure!(
                !op.sessions.iter().any(|s| s.interrupt_requested),
                "An interruption is pending; recover it before changing settings"
            );
            if !settings.enabled {
                op.view.phase = "cancelled".into();
                op.view.message =
                    "Automatic desktop handoff disabled; no session was interrupted".into();
                op.view.block_reason = None;
            } else {
                op.force = settings.resume_active_sessions
                    && crate::desktop::supports_native_resume(&settings.app);
                op.native_resume = op.force;
            }
            op.settings = settings.clone();
            {
                let mut data = state
                    .data
                    .lock()
                    .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
                data.desktop_sync.settings = settings;
                state.store.save(&data)?;
            }
            return save(state, &mut op, None);
        }
    }
    if settings.enabled {
        return request(state, settings);
    }
    let mut data = state
        .data
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    data.desktop_sync.settings = settings;
    state.store.save(&data)
}

fn wait(
    state: &ManagedState,
    op: &mut Operation,
    app: &tauri::AppHandle,
    reason: &str,
    message: &str,
) -> Result<()> {
    let sessions: Vec<_> = op.sessions.iter().map(|s| s.view.clone()).collect();
    if op.view.message == message
        && op.view.block_reason.as_deref() == Some(reason)
        && op.view.sessions == sessions
    {
        return Ok(());
    }
    op.view.message = message.into();
    op.view.block_reason = Some(reason.into());
    save(state, op, Some(app))
}

fn checkpoint_config(
    native: bool,
    thread: &Value,
    mut request: impl FnMut(&str, Value) -> Result<Value>,
) -> Result<Value> {
    if native {
        // The native client owns its policy/model/tool runtime. Never thread/resume an active
        // native chat from the recovery connection just to inspect its configuration.
        return Ok(
            json!({"nativeDesktopOwnsExecutionConfig":true,"cwd":thread["cwd"],"model":thread["model"],"modelProvider":thread["modelProvider"],"reasoningEffort":thread["reasoningEffort"]}),
        );
    }
    request(
        "thread/resume",
        json!({"threadId":thread["id"],"excludeTurns":true}),
    )
}

#[derive(Debug, PartialEq, Eq)]
enum NativeObservation {
    Pending,
    Continued,
    UserActivity,
    NeedsAttention,
}
fn native_observation(thread: &Value, original: &str, consumed: bool) -> NativeObservation {
    let Some(turns) = thread["turns"].as_array() else {
        return NativeObservation::NeedsAttention;
    };
    if !turns
        .iter()
        .any(|t| t["id"] == original && (t["status"] == "interrupted" || quota_failure(t)))
    {
        return NativeObservation::NeedsAttention;
    }
    let Some(last) = turns.last() else {
        return NativeObservation::NeedsAttention;
    };
    if last["id"] == original {
        return NativeObservation::Pending;
    }
    if !last["id"].as_str().is_some_and(|id| !id.is_empty())
        || !["inProgress", "completed"].contains(&last["status"].as_str().unwrap_or_default())
    {
        return NativeObservation::NeedsAttention;
    }
    if !consumed {
        return NativeObservation::Pending;
    }
    let Some(items) = last["items"].as_array() else {
        return NativeObservation::NeedsAttention;
    };
    if items.iter().any(|i| {
        i["type"] == "userMessage" && i["content"].as_array().is_some_and(|c| !c.is_empty())
    }) {
        NativeObservation::UserActivity
    } else {
        NativeObservation::Continued
    }
}

fn retry_phase(op: &Operation) -> &'static str {
    if op.native_queued_at.is_some() {
        "restoring"
    } else if op.sessions.iter().any(|s| {
        s.interrupt_requested && !["acknowledged", "dismissed"].contains(&s.send_state.as_str())
    }) {
        "retrySwitch"
    } else {
        "waiting"
    }
}
fn native_candidates(sessions: &[Session]) -> Vec<(String, String)> {
    sessions
        .iter()
        .filter(|s| {
            s.interrupt_requested
                && (s.interrupted || s.quota_failed)
                && s.send_state != "dismissed"
        })
        .map(|s| (s.view.thread_id.clone(), s.turn_id.clone()))
        .collect()
}

pub fn action(state: &ManagedState, action: &str) -> Result<()> {
    let _guard = state
        .account_switch
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let mut op = load(state)?.context("No desktop switch to manage")?;
    match action {
        "cancel" if op.view.phase == "waiting" => {
            anyhow::ensure!(
                op.native_queued_at.is_none() && !op.sessions.iter().any(|s| s.interrupt_requested),
                "An interruption is pending; recover or dismiss its status before cancelling"
            );
            op.view.block_reason = None;
            op.view.phase = "cancelled".into();
            op.view.message = "Pending desktop switch cancelled; CLI selection is unchanged".into();
        }
        "switchNow" if op.view.phase == "waiting" => {
            anyhow::ensure!(
                op.native_queued_at.is_none() && !op.sessions.iter().any(|s| s.interrupt_requested),
                "An interruption is pending; recover its saved checkpoint first"
            );
            op.force = true;
            op.native_resume = crate::desktop::supports_native_resume(&op.settings.app);
            op.view.message =
                "Will checkpoint eligible sessions and interrupt them with recovery".into();
        }
        "retry" if op.view.phase == "needsAttention" => {
            op.view.phase = retry_phase(&op).into();
            op.view.message =
                "Recovery retry requested; identity and send history will be checked first".into();
        }
        "dismiss" if op.view.phase == "needsAttention" => {
            op.view.phase = "cancelled".into();
            op.view.message = "Recovery dismissed. Its private checkpoint remains on disk".into();
            for s in &mut op.sessions {
                s.send_state = "dismissed".into();
                s.interrupted = false;
            }
        }
        _ => anyhow::bail!("This recovery action is not available in the current phase"),
    }
    save(state, &mut op, None)
}

fn active(t: &Value) -> bool {
    t["status"]["type"] == "active"
}
fn quota_failure(turn: &Value) -> bool {
    turn["status"] == "failed" && turn["error"]["codexErrorInfo"] == "usageLimitExceeded"
}
fn matching_quota_failure(thread: &Value, original: &str) -> bool {
    thread["turns"]
        .as_array()
        .and_then(|turns| turns.last())
        .is_some_and(|turn| turn["id"] == original && quota_failure(turn))
}
fn recent_quota_failure(thread: &Value) -> bool {
    let age = thread["updatedAt"]
        .as_i64()
        .map(|at| chrono::Utc::now().timestamp() - at);
    age.is_some_and(|age| (0..=360).contains(&age))
        && thread["turns"]
            .as_array()
            .and_then(|turns| turns.last())
            .is_some_and(quota_failure)
}
fn safe_thread(t: &Value) -> bool {
    t["ephemeral"] == false
        && t["canAcceptDirectInput"] == true
        && t["status"]["activeFlags"]
            .as_array()
            .is_some_and(|a| a.is_empty())
        // Creator metadata is immutable and does not identify today's executor. The caller
        // has already verified an exclusive native desktop connection before inspecting turns.
        && t["parentThreadId"].is_null()
        && !["aeon", "automation", "pull_request_fix_automation"].contains(&t["threadSource"].as_str().unwrap_or_default())
}
fn view(t: &Value) -> DesktopSessionView {
    DesktopSessionView {
        thread_id: t["id"].as_str().unwrap_or_default().into(),
        name: t["name"].as_str().unwrap_or("Untitled session").into(),
        cwd: t["cwd"].as_str().unwrap_or_default().into(),
        phase: "waiting".into(),
        message: "Waiting for this turn to finish".into(),
    }
}

pub fn tick(state: &ManagedState, app: &tauri::AppHandle) {
    let Ok(_guard) = state.account_switch.try_lock() else {
        return;
    };
    let Ok(Some(mut op)) = load(state) else {
        return;
    };
    if !["waiting", "restoring", "retrySwitch"].contains(&op.view.phase.as_str()) {
        return;
    }
    if let Err(error) = advance(state, &mut op, app) {
        op.view.phase = "needsAttention".into();
        op.view.message = format!("{error:#}");
        let _ = save(state, &mut op, Some(app));
    }
}

fn advance(state: &ManagedState, op: &mut Operation, app: &tauri::AppHandle) -> Result<()> {
    let (account, current, shared) = state.desktop_selected_target()?;
    if current != op.target_home || account.id != op.view.target_account_id {
        anyhow::ensure!(!op.sessions.iter().any(|s| s.interrupt_requested && !["acknowledged", "dismissed"].contains(&s.send_state.as_str())), "CLI selection changed during recovery. Reselect the handoff's account and retry, or dismiss recovery to keep its private checkpoint for manual continuation");
        op.view.phase = "cancelled".into();
        op.view.message = "CLI selection changed; stale desktop handoff cancelled".into();
        return save(state, op, Some(app));
    }
    if op.view.phase == "restoring" {
        return restore(state, op, app);
    }
    let runtimes = crate::desktop::runtime();
    let mut loaded = Vec::new();
    let mut seen_threads = std::collections::BTreeSet::new();
    for runtime in runtimes.iter().filter(|r| r.running) {
        let Some(home) = &runtime.profile_home else {
            return wait(state, op, app, "privateBackend", "Desktop needs a managed launch. Finish active work and close it once; Profiles will reopen it with automatic switching enabled.");
        };
        if !runtime.uses_managed_proxy
            && !runtime.uses_local_daemon
            && !runtime.uses_daemon_proxy
            && crate::desktop::daemon_pid(home) != runtime.backend_pid
        {
            return wait(state, op, app, "privateBackend", "Desktop is using a private backend. Close it once after active work finishes; Profiles will reopen it in managed mode.");
        }
        if crate::desktop::ensure_exclusive_desktop(runtime).is_err() {
            return wait(state, op, app, "otherClient", "Another CLI/client is using this account. Finish its work before desktop restart; it has been left running.");
        }
        let mut client = Client::connect_desktop(runtime)?;
        for mut thread in client.threads()? {
            if !active(&thread) {
                if !(op.automatic && op.force && op.native_resume) || op.view.phase == "retrySwitch"
                {
                    continue;
                }
                thread = client.request(
                    "thread/read",
                    json!({"threadId":thread["id"],"includeTurns":true}),
                )?["thread"]
                    .clone();
                if !recent_quota_failure(&thread) {
                    continue;
                }
            }
            if seen_threads.insert((
                home.clone(),
                thread["id"].as_str().unwrap_or_default().to_string(),
            )) {
                loaded.push((home.clone(), format!("{:?}", runtime.app), thread));
            }
        }
    }
    if !op.force && !loaded.is_empty() {
        op.sessions = loaded
            .into_iter()
            .map(|(home, app, thread)| Session {
                view: view(&thread),
                previous_home: home,
                app,
                host: "local".into(),
                thread,
                turn_id: String::new(),
                config: Value::Null,
                terminals: Value::Null,
                interrupt_requested: false,
                interrupted: false,
                quota_failed: false,
                send_state: "notSent".into(),
                resumed_turn_id: None,
                client_message_id: uuid::Uuid::new_v4().to_string(),
            })
            .collect();
        return wait(
            state,
            op,
            app,
            "activeSessions",
            &format!(
                "Waiting for {} active session(s); nothing has been interrupted",
                op.sessions.len()
            ),
        );
    }
    if op.force {
        for (home, _, thread) in &loaded {
            let mut client = Client::connect(home)?;
            let full = client.request(
                "thread/read",
                json!({"threadId":thread["id"],"includeTurns":true}),
            )?["thread"]
                .clone();
            let pending_tool = full["turns"].as_array().is_some_and(|turns| {
                turns.iter().any(|turn| {
                    turn["status"] == "inProgress"
                        && turn["items"].as_array().is_some_and(|items| {
                            items.iter().any(|item| {
                                ["mcpToolCall", "dynamicToolCall"]
                                    .contains(&item["type"].as_str().unwrap_or_default())
                                    && item["status"] == "inProgress"
                            })
                        })
                })
            });
            if pending_tool {
                return wait(state, op, app, "pendingTool", "Waiting for an external tool result before desktop restart; nothing has been interrupted.");
            }
        }
    }
    state.preflight_desktop_target(
        op.automatic,
        &op.view.target_account_id,
        &op.target_home,
        &op.shared_home,
    )?;
    state.ensure_desktop_target(&op.view.target_account_id, &op.target_home, &op.shared_home)?;
    crate::desktop::prepare_target_daemon(&op.settings, &op.target_home, &op.shared_home, true)?;
    if op.force {
        anyhow::ensure!(loaded.iter().all(|(_, _, t)| safe_thread(t)), "An active session is ephemeral, needs input/approval, or belongs to another client. Finish it before switching; nothing was interrupted");
    }
    if op.force && op.native_resume && !loaded.is_empty() {
        let ids = loaded
            .iter()
            .filter_map(|(_, _, thread)| thread["id"].as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        crate::desktop::preflight_native_resume(&op.settings.app, &op.target_home, &ids)?;
    }
    // Catalog repair may restart the destination profile's daemon. Never interrupt its unrelated
    // CLI work just because its desktop is closed.
    if crate::desktop::backend_requires_restart(&current, &shared) {
        let mut target = Client::connect(&current)?;
        anyhow::ensure!(!target.threads()?.iter().any(active), "The destination profile's backend needs catalog repair and is busy. Wait for its CLI/desktop work to finish before applying");
    }
    if op.view.phase == "retrySwitch" {
        anyhow::ensure!(
            loaded.is_empty(),
            "Other work is active. Finish it before retrying this handoff"
        );
        if op.native_resume {
            anyhow::ensure!(
                op.native_queued_at.is_none(),
                "Native continuation was already queued; retry only observes it"
            );
            // If the old GUI already closed, launch the selected account without a marker first.
            // Read durable history from that verified core before deciding whether another restart
            // may stage the original one-shot continuation. No thread/resume or inference is sent here.
            if !crate::desktop::runtime().iter().any(|r| r.running) {
                state.perform_desktop_handoff(
                    op.settings.clone(),
                    &op.view.target_account_id,
                    None,
                )?;
            }
            for session in op
                .sessions
                .iter_mut()
                .filter(|s| s.interrupt_requested && s.send_state != "dismissed")
            {
                let source = crate::desktop::runtime()
                    .into_iter()
                    .find(|r| {
                        r.running
                            && (r.profile_home.as_ref() == Some(&session.previous_home)
                                || r.profile_home.as_ref() == Some(&op.target_home))
                    })
                    .context("No verified desktop available to recheck the saved original turn")?;
                let mut client = Client::connect_desktop(&source)?;
                let latest = client.request(
                    "thread/read",
                    json!({"threadId":session.view.thread_id,"includeTurns":true}),
                )?;
                if !original_recoverable_turn(&latest["thread"], &session.turn_id) {
                    session.send_state = "dismissed".into();
                    session.view.phase = "completed".into();
                    session.view.message =
                        "Original turn changed; no native continuation queued".into();
                } else {
                    if op.automatic && matching_quota_failure(&latest["thread"], &session.turn_id) {
                        session.quota_failed = true;
                    }
                    session.interrupted = latest["thread"]["turns"]
                        .as_array()
                        .and_then(|t| t.last())
                        .is_some_and(|t| t["status"] == "interrupted");
                }
            }
            let candidates = native_candidates(&op.sessions);
            if !candidates.is_empty() {
                op.native_queued_at = Some(now());
                op.view.block_reason = Some("nativeContinuation".into());
                for session in op
                    .sessions
                    .iter_mut()
                    .filter(|s| s.interrupt_requested && s.send_state != "dismissed")
                {
                    session.send_state = "nativeQueued".into();
                }
                save(state, op, Some(app))?;
            }
            state.perform_desktop_handoff(
                op.settings.clone(),
                &op.view.target_account_id,
                (!candidates.is_empty()).then_some(candidates.as_slice()),
            )?;
        } else if state
            .verify_desktop_identity(&op.target_home, &op.view.target_account_id)
            .is_err()
        {
            state.perform_desktop_handoff(op.settings.clone(), &op.view.target_account_id, None)?;
        }
        op.view.phase = "restoring".into();
        save(state, op, Some(app))?;
        return restore(state, op, app);
    }
    op.view.phase = "checkpointing".into();
    op.view.block_reason = None;
    op.view.message = "Saving session configuration and last recorded results".into();
    op.sessions.clear();
    save(state, op, Some(app))?;
    for (home, source_app, thread) in loaded {
        let mut client = Client::connect(&home)?;
        let full = client.request(
            "thread/read",
            json!({"threadId":thread["id"],"includeTurns":true}),
        )?["thread"]
            .clone();
        let quota_failed = op.automatic && op.native_resume && recent_quota_failure(&full);
        if !active(&full) && !quota_failed {
            continue;
        }
        anyhow::ensure!(
            safe_thread(&full),
            "Session changed while saving; handoff stopped before interruption"
        );
        let turn = full["turns"]
            .as_array()
            .and_then(|a| {
                a.iter()
                    .rev()
                    .find(|t| t["status"] == "inProgress" || (quota_failed && quota_failure(t)))
            })
            .context("Cannot identify the active turn safely")?;
        let id = turn["id"]
            .as_str()
            .context("Active turn has no ID")?
            .to_string();
        if turn["items"].as_array().is_some_and(|a| {
            a.iter().any(|i| {
                ["mcpToolCall", "dynamicToolCall"].contains(&i["type"].as_str().unwrap_or_default())
                    && i["status"] == "inProgress"
            })
        }) {
            op.view.phase = "waiting".into();
            op.view.message = "Waiting for an external tool result before checkpoint/restart; no work has been interrupted".into();
            return save(state, op, Some(app));
        }
        if let Some(rollout) = full["path"].as_str() {
            use std::io::BufRead;
            let path = std::path::Path::new(rollout)
                .canonicalize()
                .context("Cannot inspect the saved session's tool requirements")?;
            let root = op.shared_home.canonicalize()?;
            anyhow::ensure!(path.starts_with(root.join("sessions")) || path.starts_with(root.join("archived_sessions")), "Session history is outside the shared local catalog; use its original client to recover");
            let file = std::fs::File::open(path)?;
            let first = std::io::BufReader::new(file)
                .lines()
                .next()
                .transpose()?
                .unwrap_or_default();
            let metadata: Value = serde_json::from_str(&first)?;
            anyhow::ensure!(op.native_resume || !metadata["payload"]["dynamic_tools"].as_array().is_some_and(|a| !a.is_empty()), "This session needs native client-owned tools. Finish it before switching; Switcher cannot reconstruct that tool runtime");
        } else {
            anyhow::bail!("Session has no durable local history; finish it before switching");
        }
        let config = checkpoint_config(op.native_resume, &full, |method, params| {
            client.request(method, params)
        })?;
        let terminals = client
            .request(
                "thread/backgroundTerminals/list",
                json!({"threadId":full["id"]}),
            )
            .unwrap_or(json!({"unavailable":true}));
        let mut v = view(&full);
        v.phase = "checkpointing".into();
        v.message = "Private recovery checkpoint saved".into();
        op.sessions.push(Session {
            view: v,
            app: source_app,
            host: "local".into(),
            previous_home: home,
            turn_id: id,
            thread: full,
            config,
            terminals,
            interrupt_requested: false,
            interrupted: false,
            quota_failed,
            send_state: "notSent".into(),
            resumed_turn_id: None,
            client_message_id: uuid::Uuid::new_v4().to_string(),
        });
        save(state, op, Some(app))?;
    }
    for index in 0..op.sessions.len() {
        let thread_id = op.sessions[index].view.thread_id.clone();
        let turn_id = op.sessions[index].turn_id.clone();
        let source = crate::desktop::runtime()
            .into_iter()
            .find(|r| {
                r.running
                    && r.profile_home.as_deref() == Some(op.sessions[index].previous_home.as_path())
            })
            .context("The source desktop changed before interruption")?;
        crate::desktop::ensure_exclusive_desktop(&source)?;
        let mut client = Client::connect_desktop(&source)?;
        let latest = client.request(
            "thread/read",
            json!({"threadId":thread_id,"includeTurns":true}),
        )?["thread"]
            .clone();
        if op.automatic && op.native_resume && matching_quota_failure(&latest, &turn_id) {
            // The same turn can reach quota between checkpointing and this final read.
            op.sessions[index].quota_failed = true;
        }
        if op.sessions[index].quota_failed && original_recoverable_turn(&latest, &turn_id) {
            // Quota already ended this turn; persist continuation ownership without interrupting.
            state.ensure_desktop_target(
                &op.view.target_account_id,
                &op.target_home,
                &op.shared_home,
            )?;
            op.sessions[index].interrupt_requested = true;
            op.sessions[index].view.phase = "interrupted".into();
            op.sessions[index].view.message =
                "Quota ended this turn; native continuation checkpoint saved".into();
            save(state, op, Some(app))?;
            continue;
        }
        if !active(&latest) {
            op.sessions[index].view.phase = "completed".into();
            op.sessions[index].view.message =
                "Finished before interruption; no continuation will be sent".into();
            op.sessions[index].send_state = "dismissed".into();
            continue;
        }
        anyhow::ensure!(
            safe_thread(&latest)
                && latest["turns"].as_array().is_some_and(|a| a
                    .iter()
                    .any(|t| t["id"] == turn_id && t["status"] == "inProgress")),
            "Active turn changed; stopped before interrupting it"
        );
        state.ensure_desktop_target(
            &op.view.target_account_id,
            &op.target_home,
            &op.shared_home,
        )?;
        op.sessions[index].interrupt_requested = true;
        save(state, op, Some(app))?;
        client.request(
            "turn/interrupt",
            json!({"threadId":thread_id,"turnId":turn_id}),
        )?;
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let thread = client.request(
                "thread/read",
                json!({"threadId":thread_id,"includeTurns":true}),
            )?["thread"]
                .clone();
            let status = thread["turns"]
                .as_array()
                .and_then(|a| a.iter().find(|t| t["id"] == turn_id))
                .and_then(|t| t["status"].as_str());
            if status == Some("interrupted") {
                op.sessions[index].interrupted = true;
                op.sessions[index].view.phase = "interrupted".into();
                op.sessions[index].thread = thread;
                break;
            }
            if status == Some("completed") {
                op.sessions[index].view.phase = "completed".into();
                op.sessions[index].view.message = "Completed during handoff; not resumed".into();
                op.sessions[index].send_state = "dismissed".into();
                op.sessions[index].thread = thread;
                break;
            }
            anyhow::ensure!(
                Instant::now() < deadline,
                "Turn interruption could not be confirmed. Desktop restart stopped"
            );
            std::thread::sleep(Duration::from_millis(300));
        }
        save(state, op, Some(app))?;
    }
    // Earlier sessions can change while later interruptions are being checked.
    for session in op
        .sessions
        .iter_mut()
        .filter(|s| (s.interrupted || s.quota_failed) && s.send_state != "dismissed")
    {
        let mut client = Client::connect(&session.previous_home)?;
        let latest = client.request(
            "thread/read",
            json!({"threadId":session.view.thread_id,"includeTurns":true}),
        )?;
        if !original_recoverable_turn(&latest["thread"], &session.turn_id) {
            session.send_state = "dismissed".into();
            session.view.phase = "completed".into();
            session.view.message =
                "Session changed after interruption; no automatic continuation queued".into();
        }
    }
    state.ensure_desktop_target(&op.view.target_account_id, &op.target_home, &op.shared_home)?;
    op.view.phase = "switching".into();
    op.view.message = "Applying profile and verifying the desktop backend account".into();
    save(state, op, Some(app))?;
    let native_threads = if op.native_resume {
        native_candidates(&op.sessions)
    } else {
        vec![]
    };
    if !native_threads.is_empty() {
        op.native_queued_at = Some(now());
        op.view.block_reason = Some("nativeContinuation".into());
        for session in op
            .sessions
            .iter_mut()
            .filter(|s| (s.interrupted || s.quota_failed) && s.send_state != "dismissed")
        {
            session.send_state = "nativeQueued".into();
        }
        // Persist intent first. Retries must never restage an uncertain native continuation.
        save(state, op, Some(app))?;
    }
    state.perform_desktop_handoff(
        op.settings.clone(),
        &op.view.target_account_id,
        (!native_threads.is_empty()).then_some(native_threads.as_slice()),
    )?;
    op.view.phase = "restoring".into();
    op.view.message = "Account confirmed; checking interrupted sessions".into();
    save(state, op, Some(app))?;
    restore(state, op, app)
}

pub(crate) fn original_recoverable_turn(thread: &Value, turn_id: &str) -> bool {
    thread["turns"]
        .as_array()
        .and_then(|turns| turns.last())
        .is_some_and(|turn| {
            turn["id"] == turn_id && (turn["status"] == "interrupted" || quota_failure(turn))
        })
}

#[cfg(test)]
mod final_guard_tests {
    use super::*;
    #[test]
    fn completed_or_new_user_activity_is_never_queued() {
        let quota = json!({"updatedAt":chrono::Utc::now().timestamp(),"turns":[{"id":"quota","status":"failed","error":{"codexErrorInfo":"usageLimitExceeded"}}]});
        assert!(recent_quota_failure(&quota));
        assert!(matching_quota_failure(&quota, "quota"));
        assert!(!matching_quota_failure(&quota, "older-unrelated-turn"));
        assert!(original_recoverable_turn(&quota, "quota"));
        let mut stale = quota.clone();
        stale["updatedAt"] = json!(chrono::Utc::now().timestamp() - 361);
        assert!(!recent_quota_failure(&stale));
        let mut other = quota.clone();
        other["turns"][0]["error"]["codexErrorInfo"] = json!("rateLimitExceeded");
        assert!(!recent_quota_failure(&other));
        assert!(original_recoverable_turn(
            &json!({"turns":[{"id":"old","status":"interrupted"}]}),
            "old"
        ));
        assert!(!original_recoverable_turn(
            &json!({"turns":[{"id":"old","status":"interrupted"},{"id":"new","status":"completed"}]}),
            "old"
        ));
        assert!(!original_recoverable_turn(
            &json!({"turns":[{"id":"old","status":"completed"}]}),
            "old"
        ));
    }
}

fn restore(state: &ManagedState, op: &mut Operation, app: &tauri::AppHandle) -> Result<()> {
    state.verify_desktop_identity(&op.target_home, &op.view.target_account_id)?;
    let mut client = Client::connect(&op.target_home)?;
    for index in 0..op.sessions.len() {
        if !op.sessions[index].interrupt_requested
            || ["acknowledged", "dismissed"].contains(&op.sessions[index].send_state.as_str())
        {
            continue;
        }
        let id = op.sessions[index].view.thread_id.clone();
        let marker = format!("[Michael Le Profiles recovery {}]", op.view.id);
        let latest = client.request("thread/read", json!({"threadId":id,"includeTurns":true}))?
            ["thread"]
            .clone();
        if op.sessions[index].send_state == "nativeQueued" {
            let consumed = crate::desktop_state::native_resume_consumed(&op.target_home)?;
            match native_observation(&latest, &op.sessions[index].turn_id, consumed) {
                NativeObservation::Continued => {
                    op.sessions[index].send_state = "acknowledged".into();
                    op.sessions[index].view.phase = "continued".into();
                    op.sessions[index].view.message = "Native continuation observed after its one-shot marker was consumed".into();
                    save(state, op, Some(app))?; continue;
                }
                NativeObservation::UserActivity => {
                    op.sessions[index].send_state = "dismissed".into();
                    op.sessions[index].view.phase = "notResumed".into();
                    op.sessions[index].view.message = "New user input exists; no extra continuation was sent".into();
                    save(state, op, Some(app))?; continue;
                }
                NativeObservation::NeedsAttention => anyhow::bail!("Original interruption or native continuation could not be confirmed. Open the original chat; no extra continuation was sent"),
                NativeObservation::Pending => (),
            }
            let elapsed = op
                .native_queued_at
                .as_ref()
                .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
                .map(|at| chrono::Utc::now().signed_duration_since(at).num_seconds())
                .unwrap_or(61);
            anyhow::ensure!(elapsed < 60, "Native desktop continuation has not been confirmed. Open the original chats; no duplicate continuation was sent");
            return wait(
                state,
                op,
                app,
                "nativeContinuation",
                "Account confirmed; waiting for native desktop continuation",
            );
        }
        if !op.sessions[index].interrupted {
            let status = latest["turns"]
                .as_array()
                .and_then(|a| a.iter().find(|t| t["id"] == op.sessions[index].turn_id))
                .and_then(|t| t["status"].as_str());
            match status {
                Some("interrupted") => {
                    op.sessions[index].interrupted = true;
                    save(state, op, Some(app))?;
                }
                Some("completed") => {
                    op.sessions[index].send_state = "dismissed".into();
                    op.sessions[index].view.phase = "completed".into();
                    save(state, op, Some(app))?;
                    continue;
                }
                _ => anyhow::bail!(
                    "Interruption outcome is uncertain. No automatic continuation was sent"
                ),
            }
        }
        // Check the persisted marker on every retry; never blindly resend an uncertain request.
        let acknowledged = latest["turns"].as_array().is_some_and(|turns| {
            turns.iter().any(|turn| {
                turn["items"].as_array().is_some_and(|items| {
                    items.iter().any(|item| {
                        item["type"] == "userMessage"
                            && item["content"].as_array().is_some_and(|parts| {
                                parts.iter().any(|part| {
                                    part["text"]
                                        .as_str()
                                        .is_some_and(|text| text.contains(&marker))
                                })
                            })
                    })
                })
            })
        });
        if acknowledged {
            op.sessions[index].send_state = "acknowledged".into();
            op.sessions[index].view.phase = "continued".into();
            save(state, op, Some(app))?;
            continue;
        }
        anyhow::ensure!(op.sessions[index].send_state == "notSent", "Continuation delivery is uncertain and no persisted acknowledgement was found. Inspect the original session before retrying");
        anyhow::ensure!(
            !active(&latest),
            "The original session is already active; no duplicate continuation was sent"
        );
        if latest["turns"]
            .as_array()
            .and_then(|a| a.last())
            .is_some_and(|last| last["id"] != op.sessions[index].turn_id)
        {
            op.sessions[index].view.phase = "notResumed".into();
            op.sessions[index].view.message =
                "Newer user activity exists; no automatic continuation was sent".into();
            op.sessions[index].send_state = "dismissed".into();
            save(state, op, Some(app))?;
            continue;
        }
        let interrupted_turn = latest["turns"].as_array().and_then(|a| {
            a.iter()
                .rev()
                .find(|t| t["id"] == op.sessions[index].turn_id)
        });
        anyhow::ensure!(
            interrupted_turn.is_some_and(|t| t["status"] == "interrupted"),
            "Original interruption could not be verified; session needs attention"
        );
        let config = op.sessions[index].config.clone();
        // Bring the existing session into the native UI, rather than creating another chat.
        crate::desktop::open_thread(&op.settings.app, &id)?;
        let params = json!({"threadId":id,"excludeTurns":true,"cwd":config["cwd"],"model":config["model"],"modelProvider":config["modelProvider"],"approvalPolicy":config["approvalPolicy"],"approvalsReviewer":config["approvalsReviewer"],"runtimeWorkspaceRoots":config["runtimeWorkspaceRoots"]});
        let resumed = client.request("thread/resume", params)?;
        for key in [
            "cwd",
            "model",
            "modelProvider",
            "approvalPolicy",
            "approvalsReviewer",
            "sandbox",
        ] {
            anyhow::ensure!(resumed[key] == config[key], "Session execution settings changed ({key}); open the thread to recover without changing its policy");
        }
        anyhow::ensure!(
            resumed["thread"]["canAcceptDirectInput"] == true,
            "Native desktop cannot accept direct input for this session; open it to recover"
        );
        anyhow::ensure!(resumed["thread"]["environments"] == op.sessions[index].thread["environments"], "Session environments changed; open the original desktop session to restore its execution host");
        let prompt = format!("{marker}\nContinue the work from before the account switch. Inspect the current files and processes, identify the last completed step, and finish the remaining work. Avoid repeating operations that already succeeded. Preserve the original objective, model and execution policies. A recovery checkpoint recorded process/terminal state, but RAM and PTY state may have been lost. If an external action's result is uncertain, stop and report what needs checking instead of repeating it.");
        let mut turn = json!({"threadId":id,"clientUserMessageId":op.sessions[index].client_message_id,"input":[{"type":"text","text":prompt}],"cwd":config["cwd"],"model":config["model"],"effort":config["reasoningEffort"],"approvalPolicy":config["approvalPolicy"],"approvalsReviewer":config["approvalsReviewer"],"sandboxPolicy":config["sandbox"],"collaborationMode":config["collaborationMode"],"serviceTier":config["serviceTier"]});
        if config.get("disabledPluginIds").is_some() {
            turn["disabledPluginIds"] = config["disabledPluginIds"].clone();
        }
        op.sessions[index].send_state = "sending".into();
        op.sessions[index].view.phase = "restoring".into();
        save(state, op, Some(app))?;
        let response = client.request("turn/start", turn)?;
        let turn_id = response["turn"]["id"]
            .as_str()
            .context("Continuation was sent but no turn acknowledgement was returned")?
            .to_string();
        op.sessions[index].resumed_turn_id = Some(turn_id);
        op.sessions[index].send_state = "acknowledged".into();
        op.sessions[index].view.phase = "continued".into();
        op.sessions[index].view.message = "Continuation acknowledged in the original thread".into();
        save(state, op, Some(app))?;
    }
    op.view.phase = "applied".into();
    op.view.message = format!(
        "Desktop account confirmed; {} checkpointed session(s) continued",
        op.sessions
            .iter()
            .filter(|s| s.send_state == "acknowledged")
            .count()
    );
    save(state, op, Some(app))
}

pub fn recover_after_restart(state: &ManagedState) {
    if let Ok(Some(mut op)) = load(state) {
        if ["checkpointing", "switching", "retrySwitch"].contains(&op.view.phase.as_str()) {
            op.view.phase = "needsAttention".into();
            op.view.message = "Switcher restarted during a handoff. The checkpoint is preserved; retry will verify account, interruption and delivery before continuing".into();
            let _ = save(state, &mut op, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn waiting_fixture() -> (ManagedState, Operation, PathBuf) {
        let root = std::env::temp_dir().join(format!("waiting-settings-{}", uuid::Uuid::new_v4()));
        let mut data = crate::store::StoredState::default();
        data.desktop_sync.settings.enabled = true;
        data.desktop_sync.settings.resume_active_sessions = false;
        let state = ManagedState {
            store: crate::store::Store::for_test(root.clone()).unwrap(),
            api_server: std::sync::Mutex::new(crate::api_gateway::ApiServerHandle::stopped(
                &data.api_gateway,
            )),
            data: std::sync::Mutex::new(data.clone()),
            usage_scan: std::sync::Mutex::new(()),
            account_switch: std::sync::Mutex::new(()),
            priming: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        let op = Operation {
            view: DesktopOperationView {
                id: uuid::Uuid::new_v4().to_string(),
                phase: "waiting".into(),
                ..Default::default()
            },
            settings: data.desktop_sync.settings,
            target_home: root.join("target"),
            shared_home: root.join("shared"),
            automatic: false,
            force: false,
            native_resume: false,
            native_queued_at: None,
            sessions: vec![],
        };
        (state, op, root)
    }

    #[test]
    fn quota_checkpoint_retry_queues_once_then_only_observes() {
        let (state, mut op, root) = waiting_fixture();
        op.native_resume = true;
        op.view.phase = "needsAttention".into();
        op.sessions.push(Session {view:DesktopSessionView {thread_id:"chat".into(),..Default::default()},host:"local".into(),app:"Chatgpt".into(),previous_home:root.clone(),turn_id:"quota".into(),thread:json!({"turns":[{"id":"quota","status":"failed","error":{"codexErrorInfo":"usageLimitExceeded"}}]}),config:Value::Null,terminals:Value::Null,interrupt_requested:true,interrupted:false,quota_failed:true,send_state:"notSent".into(),resumed_turn_id:None,client_message_id:"id".into()});
        save(&state, &mut op, None).unwrap();
        action(&state, "retry").unwrap();
        let retry = load(&state).unwrap().unwrap();
        assert_eq!(retry.view.phase, "retrySwitch");
        assert_eq!(
            native_candidates(&retry.sessions),
            vec![("chat".into(), "quota".into())]
        );
        op.native_queued_at = Some(now());
        op.sessions[0].send_state = "nativeQueued".into();
        op.view.phase = "needsAttention".into();
        save(&state, &mut op, None).unwrap();
        action(&state, "retry").unwrap();
        assert_eq!(load(&state).unwrap().unwrap().view.phase, "restoring");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn waiting_settings_can_be_disabled_without_losing_the_checkpoint() {
        let (state, mut op, root) = waiting_fixture();
        save(&state, &mut op, None).unwrap();
        let mut settings = op.settings.clone();
        settings.enabled = false;
        update_settings(&state, settings).unwrap();
        assert!(!state.data.lock().unwrap().desktop_sync.settings.enabled);
        let saved = load(&state).unwrap().unwrap();
        assert_eq!(saved.view.phase, "cancelled");
        assert_eq!(saved.view.id, op.view.id);
        assert!(path(&state, &op.view.id).unwrap().exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn waiting_cancel_cannot_drop_an_uncertain_interruption() {
        let (state, mut op, root) = waiting_fixture();
        op.sessions.push(Session {
            view: DesktopSessionView {
                phase: "checkpointing".into(),
                ..Default::default()
            },
            host: "local".into(),
            app: "Chatgpt".into(),
            previous_home: root.clone(),
            turn_id: "old".into(),
            thread: Value::Null,
            config: Value::Null,
            terminals: Value::Null,
            interrupt_requested: true,
            interrupted: false,
            quota_failed: false,
            send_state: "notSent".into(),
            resumed_turn_id: None,
            client_message_id: "id".into(),
        });
        save(&state, &mut op, None).unwrap();
        let before = std::fs::read(path(&state, &op.view.id).unwrap()).unwrap();
        assert!(action(&state, "cancel").is_err());
        assert_eq!(
            std::fs::read(path(&state, &op.view.id).unwrap()).unwrap(),
            before
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovery_settings_cannot_discard_a_pending_native_continuation() {
        let (state, mut op, root) = waiting_fixture();
        op.view.phase = "needsAttention".into();
        op.native_queued_at = Some(now());
        save(&state, &mut op, None).unwrap();
        let before = std::fs::read(path(&state, &op.view.id).unwrap()).unwrap();
        let mut settings = op.settings.clone();
        settings.enabled = false;
        assert!(update_settings(&state, settings).is_err());
        assert!(state.data.lock().unwrap().desktop_sync.settings.enabled);
        assert_eq!(
            std::fs::read(path(&state, &op.view.id).unwrap()).unwrap(),
            before
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_recovery_excludes_automation_and_subagents_not_historical_cli_roots() {
        let mut thread = json!({"ephemeral":false,"canAcceptDirectInput":true,"status":{"type":"active","activeFlags":[]},"originator":"Codex Desktop","threadSource":"automation","parentThreadId":null});
        assert!(!safe_thread(&thread));
        thread["threadSource"] = Value::Null;
        thread["parentThreadId"] = json!("parent");
        assert!(!safe_thread(&thread));
        thread["parentThreadId"] = Value::Null;
        thread["originator"] = json!("codex_cli_rs");
        assert!(safe_thread(&thread), "creator metadata must not exclude a CLI-originated chat now running in the verified exclusive desktop");
    }

    #[test]
    fn native_checkpoint_never_claims_the_desktop_tool_runtime() {
        let thread = json!({"id":"original-thread","cwd":"/project","model":"original-model","modelProvider":"openai"});
        let config = checkpoint_config(true, &thread, |_, _| -> Result<Value> {
            panic!("native checkpoint must not thread/resume through the recovery client")
        })
        .unwrap();
        assert_eq!(config["model"], "original-model");
        let config = checkpoint_config(false, &thread, |method, params| {
            assert_eq!(method, "thread/resume");
            assert_eq!(params["threadId"], "original-thread");
            Ok(json!({"model":"saved-model","approvalPolicy":"on-request"}))
        })
        .unwrap();
        assert_eq!(config["approvalPolicy"], "on-request");
    }

    #[test]
    fn native_acknowledgement_needs_durable_marker_consumption_and_no_new_user_input() {
        let mut thread = json!({"turns":[{"id":"old","status":"interrupted"},{"id":"new","status":"completed","items":[]}]});
        assert_eq!(
            native_observation(&thread, "old", false),
            NativeObservation::Pending
        );
        assert_eq!(
            native_observation(&thread, "old", true),
            NativeObservation::Continued
        );
        thread["turns"][1]["items"] =
            json!([{"type":"userMessage","content":[{"type":"text","text":"new objective"}]}]);
        assert_eq!(
            native_observation(&thread, "old", true),
            NativeObservation::UserActivity
        );
        thread["turns"][1]["status"] = json!("failed");
        assert_eq!(
            native_observation(&thread, "old", true),
            NativeObservation::NeedsAttention
        );
        thread["turns"][0]["status"] = json!("completed");
        assert_eq!(
            native_observation(&thread, "old", true),
            NativeObservation::NeedsAttention
        );
    }

    #[test]
    fn native_retry_only_observes_and_never_restarts_or_restages() {
        let root = std::env::temp_dir().join(format!("native-retry-{}", uuid::Uuid::new_v4()));
        let data = crate::store::StoredState::default();
        let state = ManagedState {
            store: crate::store::Store::for_test(root.clone()).unwrap(),
            api_server: std::sync::Mutex::new(crate::api_gateway::ApiServerHandle::stopped(
                &data.api_gateway,
            )),
            data: std::sync::Mutex::new(data),
            usage_scan: std::sync::Mutex::new(()),
            account_switch: std::sync::Mutex::new(()),
            priming: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        let queued = now();
        let mut op = Operation {
            view: DesktopOperationView {
                id: uuid::Uuid::new_v4().to_string(),
                phase: "needsAttention".into(),
                ..Default::default()
            },
            settings: DesktopSyncSettings::default(),
            target_home: root.join("target"),
            shared_home: root.clone(),
            force: true,
            automatic: false,
            native_resume: true,
            native_queued_at: Some(queued.clone()),
            sessions: vec![],
        };
        save(&state, &mut op, None).unwrap();
        action(&state, "retry").unwrap();
        let restored = load(&state).unwrap().unwrap();
        assert_eq!(restored.view.phase, "restoring");
        assert_eq!(restored.native_queued_at.as_deref(), Some(queued.as_str()));
        std::fs::remove_dir_all(root).unwrap();
    }
}
