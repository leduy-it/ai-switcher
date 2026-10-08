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
    if let Some(mut prior) = load(state)? {
        if prior.sessions.iter().any(|s| {
            s.interrupt_requested && !["acknowledged", "dismissed"].contains(&s.send_state.as_str())
        }) {
            anyhow::bail!(
                "Recover or dismiss the interrupted sessions before another desktop switch"
            );
        }
        if prior.view.phase == "waiting" {
            prior.view.phase = "cancelled".into();
            prior.view.message = "Replaced by a newer account selection".into();
            save(state, &mut prior, None)?;
        }
    }
    let (account, target_home, shared_home) = state.desktop_selected_target()?;
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
        target_home,
        shared_home,
        force: false,
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

pub fn action(state: &ManagedState, action: &str) -> Result<()> {
    let _guard = state
        .account_switch
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let mut op = load(state)?.context("No desktop switch to manage")?;
    match action {
        "cancel" if op.view.phase == "waiting" => {
            op.view.phase = "cancelled".into();
            op.view.message = "Pending desktop switch cancelled; CLI selection is unchanged".into();
        }
        "switchNow" if op.view.phase == "waiting" => {
            op.force = true;
            op.view.message =
                "Will checkpoint eligible sessions and interrupt them with recovery".into();
        }
        "retry" if op.view.phase == "needsAttention" => {
            op.view.phase = if op.sessions.iter().any(|s| {
                s.interrupt_requested
                    && !["acknowledged", "dismissed"].contains(&s.send_state.as_str())
            }) {
                "retrySwitch"
            } else {
                "waiting"
            }
            .into();
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
fn safe_thread(t: &Value) -> bool {
    t["ephemeral"] == false
        && t["canAcceptDirectInput"] == true
        && t["status"]["activeFlags"]
            .as_array()
            .is_some_and(|a| a.is_empty())
        && t["originator"].as_str().is_some_and(|o| {
            [
                "Codex Desktop",
                "codex_desktop",
                "codex-desktop",
                "ChatGPT Desktop",
                "chatgpt_desktop",
            ]
            .contains(&o)
        })
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
            op.view.message = "This desktop was launched outside Switcher and its execution profile is unknown. Finish its work and quit it; the pending switch will then launch it with a verified profile".into();
            return save(state, op, Some(app));
        };
        let mut client = Client::connect(home)?;
        for thread in client.threads()?.into_iter().filter(active) {
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
                send_state: "notSent".into(),
                resumed_turn_id: None,
                client_message_id: uuid::Uuid::new_v4().to_string(),
            })
            .collect();
        op.view.message = format!(
            "Waiting for {} active session(s). No work has been interrupted",
            op.sessions.len()
        );
        return save(state, op, Some(app));
    }
    if op.force {
        anyhow::ensure!(loaded.iter().all(|(_, _, t)| safe_thread(t)), "An active session is ephemeral, needs input/approval, or belongs to another client. Finish it before switching; nothing was interrupted");
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
        if state
            .verify_desktop_identity(&op.target_home, &op.view.target_account_id)
            .is_err()
        {
            state.perform_desktop_handoff(op.settings.clone(), &op.view.target_account_id)?;
        }
        op.view.phase = "restoring".into();
        save(state, op, Some(app))?;
        return restore(state, op, app);
    }
    op.view.phase = "checkpointing".into();
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
        if !active(&full) {
            continue;
        }
        anyhow::ensure!(
            safe_thread(&full),
            "Session changed while saving; handoff stopped before interruption"
        );
        let turn = full["turns"]
            .as_array()
            .and_then(|a| a.iter().rev().find(|t| t["status"] == "inProgress"))
            .context("Cannot identify the active turn safely")?;
        let id = turn["id"]
            .as_str()
            .context("Active turn has no ID")?
            .to_string();
        anyhow::ensure!(!turn["items"].as_array().is_some_and(|a| a.iter().any(|i| ["mcpToolCall", "dynamicToolCall"].contains(&i["type"].as_str().unwrap_or_default()) && i["status"] == "inProgress")), "An external tool is in progress. Wait for its result; recovery will not repeat an unknown external action");
        let config = client.request(
            "thread/resume",
            json!({"threadId":full["id"],"excludeTurns":true}),
        )?;
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
            anyhow::ensure!(!metadata["payload"]["dynamic_tools"].as_array().is_some_and(|a| !a.is_empty()), "This session needs native client-owned tools. Finish it before switching; Switcher cannot reconstruct that tool runtime");
        } else {
            anyhow::bail!("Session has no durable local history; finish it before switching");
        }
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
            send_state: "notSent".into(),
            resumed_turn_id: None,
            client_message_id: uuid::Uuid::new_v4().to_string(),
        });
        save(state, op, Some(app))?;
    }
    for index in 0..op.sessions.len() {
        let thread_id = op.sessions[index].view.thread_id.clone();
        let turn_id = op.sessions[index].turn_id.clone();
        let mut client = Client::connect(&op.sessions[index].previous_home)?;
        let latest = client.request(
            "thread/read",
            json!({"threadId":thread_id,"includeTurns":true}),
        )?["thread"]
            .clone();
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
    op.view.phase = "switching".into();
    op.view.message = "Applying profile and verifying the desktop backend account".into();
    save(state, op, Some(app))?;
    state.perform_desktop_handoff(op.settings.clone(), &op.view.target_account_id)?;
    op.view.phase = "restoring".into();
    op.view.message = "Account confirmed; checking interrupted sessions".into();
    save(state, op, Some(app))?;
    restore(state, op, app)
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
        "Desktop account confirmed; {} interrupted session(s) continued",
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
