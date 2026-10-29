//! Desktop state is profile-local and saved by atomic rename. A symlink cannot share it reliably.
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

const STATE: &str = ".codex-global-state.json";

fn merge_object(target: &mut Value, source: &Value) {
    if let (Some(target), Some(source)) = (target.as_object_mut(), source.as_object()) {
        for (key, value) in source {
            if let Some(existing) = target.get_mut(key) {
                merge_object(existing, value);
            } else {
                target.insert(key.clone(), value.clone());
            }
        }
    }
}

fn read(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let value: Value = serde_json::from_slice(&fs::read(path)?)
        .context("Desktop state could not be read; original files were preserved")?;
    anyhow::ensure!(
        value.is_object(),
        "Unsupported desktop state; original files were preserved"
    );
    Ok(value)
}

// Merge organization and thread UI records only. Keep the destination's existing placements and
// preferences authoritative; account/cloud caches and opaque runtime tokens are never imported.
fn merge_missing(target: &mut Value, source: &Value) {
    for key in [
        "local-projects",
        "thread-project-assignments",
        "thread-project-membership-host-ids",
        "thread-workspace-root-hints",
        "thread-projectless-output-directories",
        "thread-writable-roots",
        "sidebar-project-thread-orders",
        "project-appearances",
        "app-server-project-id-by-legacy-project-id-by-host",
    ] {
        let Some(source) = source.get(key).filter(|value| value.is_object()) else {
            continue;
        };
        let destination = target
            .as_object_mut()
            .unwrap()
            .entry(key)
            .or_insert_with(|| json!({}));
        merge_object(destination, source);
    }
    for key in ["projectless-thread-ids", "project-order", "remote-projects"] {
        let Some(source) = source.get(key).and_then(Value::as_array) else {
            continue;
        };
        let destination = target
            .as_object_mut()
            .unwrap()
            .entry(key)
            .or_insert_with(|| json!([]));
        if let Some(destination) = destination.as_array_mut() {
            for value in source {
                let existing = destination.iter().any(|item| {
                    item == value
                        || key == "remote-projects"
                            && value.get("id").is_some()
                            && item.get("id") == value.get("id")
                });
                if !existing {
                    destination.push(value.clone());
                }
            }
        }
    }
    // A valid explicit project placement wins over a stale projectless record from an old profile.
    let assigned: Vec<String> = target
        .get("thread-project-assignments")
        .and_then(Value::as_object)
        .map(|v| v.keys().cloned().collect())
        .unwrap_or_default();
    if let Some(ids) = target
        .get_mut("projectless-thread-ids")
        .and_then(Value::as_array_mut)
    {
        ids.retain(|id| {
            !id.as_str()
                .is_some_and(|id| assigned.iter().any(|key| key == id))
        });
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn save(path: &Path, value: &Value, shared: &Path) -> Result<()> {
    let directory = shared
        .join("backups")
        .join(format!("desktop-state-sharing-{}", uuid::Uuid::new_v4()));
    crate::session_migration::private_directory(&directory)?;
    let original = if path.exists() {
        Some(fs::read(path)?)
    } else {
        None
    };
    if let Some(bytes) = &original {
        write_private(&directory.join("state-before.json"), bytes)?;
    }
    write_private(
        &directory.join("prepared.json"),
        &serde_json::to_vec_pretty(&json!({
            "version": 1, "destination": path, "status": "prepared", "hadOriginal": original.is_some()
        }))?,
    )?;
    fs::File::open(&directory)?.sync_all()?;
    let temporary = path.with_file_name(format!("{STATE}.tmp-{}", uuid::Uuid::new_v4()));
    write_private(&temporary, &serde_json::to_vec(value)?)?;
    let unchanged = if let Some(bytes) = original {
        fs::read(path)? == bytes
    } else {
        !path.exists()
    };
    if !unchanged {
        let _ = fs::remove_file(&temporary);
        anyhow::bail!("Desktop state changed during repair; finish its work and retry. Private backups were preserved");
    }
    // Replace old symlinks with a regular snapshot, matching the desktop's own atomic saves.
    fs::rename(&temporary, path)?;
    fs::File::open(
        path.parent()
            .context("Desktop state has no parent directory")?,
    )?
    .sync_all()?;
    write_private(
        &directory.join("completed.json"),
        b"{\"version\":1,\"status\":\"completed\"}",
    )?;
    Ok(())
}

pub fn merge_profile(profile: &Path, shared: &Path) -> Result<bool> {
    if profile.canonicalize()? == shared.canonicalize()? {
        return Ok(false);
    }
    let target_path = shared.join(STATE);
    let mut target = read(&target_path)?;
    let before = target.clone();
    // Earlier versions renamed these copies before linking, without merging their sidebar data.
    let mut sources: Vec<PathBuf> = fs::read_dir(profile)?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(&format!("{STATE}.before-desktop-sharing-"))
                .then(|| entry.path())
        })
        .collect();
    sources.sort_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok());
    sources.reverse();
    sources.insert(0, profile.join(STATE));
    for path in sources {
        merge_missing(&mut target, &read(&path)?);
    }
    if target == before {
        return Ok(false);
    }
    save(&target_path, &target, shared)?;
    Ok(true)
}

pub fn needs_refresh(profile: &Path, shared: &Path) -> Result<bool> {
    let current = read(&profile.join(STATE))?;
    let mut merged = current.clone();
    merge_missing(&mut merged, &read(&shared.join(STATE))?);
    Ok(current != merged)
}

// Call only after the desktop exits: its in-memory state otherwise overwrites repaired files.
pub fn prepare_handoff(outgoing: &[PathBuf], profile: &Path, shared: &Path) -> Result<()> {
    for home in outgoing {
        merge_profile(home, shared)?;
    }
    merge_profile(profile, shared)?;
    if profile.canonicalize()? != shared.canonicalize()? {
        let path = profile.join(STATE);
        let value = read(&shared.join(STATE))?;
        if read(&path)? != value
            || fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink())
        {
            save(&path, &value, shared)?;
        }
    }
    // The desktop has another SQLite catalog under CODEX_HOME. Its incremental watermark can
    // skip restored threads whose original timestamps predate it. Force a local full scan while
    // keeping inboxes, automations, other hosts and history untouched. Never symlink this database.
    for name in ["codex.db", "codex-dev.db"] {
        let db = profile.join("sqlite").join(name);
        if !db.exists() {
            continue;
        }
        let columns = crate::session_migration::sql(
            &db,
            "PRAGMA table_info(local_thread_catalog_sync_state);",
            true,
        )?;
        if columns.is_empty() {
            continue;
        }
        for field in [
            "host_id",
            "watermark_updated_at",
            "initial_build_complete",
            "last_full_reconciled_at",
        ] {
            anyhow::ensure!(
                columns.iter().any(|c| c["name"] == field),
                "Unsupported desktop catalog cache; original database was preserved"
            );
        }
        let directory = shared
            .join("backups")
            .join(format!("desktop-catalog-refresh-{}", uuid::Uuid::new_v4()));
        crate::session_migration::private_directory(&directory)?;
        crate::session_migration::backup(&db, &directory.join(name))?;
        crate::session_migration::sql(&db, "BEGIN IMMEDIATE; UPDATE local_thread_catalog_sync_state SET watermark_updated_at=NULL,initial_build_complete=0,last_full_reconciled_at=NULL WHERE host_id='local'; COMMIT;", false)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovers_projectless_chats_without_overwriting_current_placements_or_account_cache() {
        let mut target = json!({"projectless-thread-ids":["existing"],"thread-project-assignments":{"moved":{"projectId":"new"}},"account-token":"keep"});
        let source = json!({"projectless-thread-ids":["missing","moved"],"thread-project-assignments":{"moved":{"projectId":"old"}},"thread-projectless-output-directories":{"missing":"/output"},"account-token":"do-not-copy"});
        merge_missing(&mut target, &source);
        assert_eq!(
            target["projectless-thread-ids"],
            json!(["existing", "missing"])
        );
        assert_eq!(
            target["thread-project-assignments"]["moved"]["projectId"],
            "new"
        );
        assert_eq!(target["account-token"], "keep");
        assert_eq!(
            target["thread-projectless-output-directories"]["missing"],
            "/output"
        );
        let once = target.clone();
        merge_missing(&mut target, &source);
        assert_eq!(target, once);
        let mut projects = json!({"remote-projects":[{"id":"host","label":"new"}],"app-server-project-id-by-legacy-project-id-by-host":{"local":{"one":"native-one"}}});
        merge_missing(
            &mut projects,
            &json!({"remote-projects":[{"id":"host","label":"old"}],"app-server-project-id-by-legacy-project-id-by-host":{"local":{"two":"native-two"}}}),
        );
        assert_eq!(projects["remote-projects"].as_array().unwrap().len(), 1);
        assert_eq!(projects["remote-projects"][0]["label"], "new");
        assert_eq!(
            projects["app-server-project-id-by-legacy-project-id-by-host"]["local"]["two"],
            "native-two"
        );
    }

    #[test]
    fn atomic_desktop_save_survives_a_round_trip_between_profiles() -> Result<()> {
        let root =
            std::env::temp_dir().join(format!("desktop-state-test-{}", uuid::Uuid::new_v4()));
        let shared = root.join("shared");
        let first = root.join("first");
        let second = root.join("second");
        for home in [&shared, &first, &second] {
            fs::create_dir_all(home)?;
        }
        fs::write(
            shared.join(STATE),
            b"{\"projectless-thread-ids\":[\"older\"]}",
        )?;
        fs::write(
            first.join(STATE),
            b"{\"projectless-thread-ids\":[\"today\"]}",
        )?;
        fs::write(first.join("auth.json"), "private-first")?;
        fs::write(second.join("auth.json"), "private-second")?;
        prepare_handoff(&[first.clone()], &second, &shared)?;
        assert!(!needs_refresh(&second, &shared)?);
        assert!(needs_refresh(&first, &shared)?);
        assert_eq!(
            read(&second.join(STATE))?["projectless-thread-ids"],
            json!(["older", "today"])
        );
        let temporary = second.join("desktop-atomic-save");
        fs::write(
            &temporary,
            b"{\"projectless-thread-ids\":[\"older\",\"today\",\"next\"]}",
        )?;
        fs::rename(temporary, second.join(STATE))?;
        prepare_handoff(&[second.clone()], &first, &shared)?;
        assert_eq!(
            read(&first.join(STATE))?["projectless-thread-ids"],
            json!(["older", "today", "next"])
        );
        assert!(!fs::symlink_metadata(first.join(STATE))?
            .file_type()
            .is_symlink());
        assert_eq!(
            fs::read_to_string(first.join("auth.json"))?,
            "private-first"
        );
        assert_eq!(
            fs::read_to_string(second.join("auth.json"))?,
            "private-second"
        );
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn recovers_state_renamed_by_old_versions_and_refreshes_only_local_cache() -> Result<()> {
        let root =
            std::env::temp_dir().join(format!("desktop-state-test-{}", uuid::Uuid::new_v4()));
        let shared = root.join("shared");
        let profile = root.join("profile");
        fs::create_dir_all(&shared)?;
        fs::create_dir_all(profile.join("sqlite"))?;
        fs::write(shared.join(STATE), b"{}")?;
        fs::write(
            profile.join(format!("{STATE}.before-desktop-sharing-old")),
            b"{\"projectless-thread-ids\":[\"restored\"]}",
        )?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(shared.join(STATE), profile.join(STATE))?;
        let db = profile.join("sqlite/codex-dev.db");
        let status = std::process::Command::new("/usr/bin/sqlite3").arg(&db).arg("CREATE TABLE local_thread_catalog_sync_state(host_id TEXT PRIMARY KEY,watermark_updated_at REAL,initial_build_complete INTEGER,last_full_reconciled_at INTEGER); INSERT INTO local_thread_catalog_sync_state VALUES('local',100,1,100),('remote',200,1,200); CREATE TABLE automations(id TEXT); INSERT INTO automations VALUES('keep');").status()?;
        anyhow::ensure!(status.success(), "Could not create test cache");
        prepare_handoff(&[], &profile, &shared)?;
        assert_eq!(
            read(&profile.join(STATE))?["projectless-thread-ids"],
            json!(["restored"])
        );
        assert!(!fs::symlink_metadata(profile.join(STATE))?
            .file_type()
            .is_symlink());
        let rows = crate::session_migration::sql(
            &db,
            "SELECT * FROM local_thread_catalog_sync_state ORDER BY host_id;",
            true,
        )?;
        assert_eq!(rows[0]["initial_build_complete"], 0);
        assert!(rows[0]["watermark_updated_at"].is_null());
        assert_eq!(rows[1]["watermark_updated_at"], 200.0);
        assert_eq!(
            crate::session_migration::sql(&db, "SELECT id FROM automations;", true)?[0]["id"],
            "keep"
        );
        fs::remove_dir_all(root)?;
        Ok(())
    }
}

/// Call only after the outgoing desktop has exited normally. The desktop consumes this
/// versioned one-shot marker, verifies its own account identity, and starts native empty turns.
fn native_resume_state(
    profile: &Path,
    account_id: &str,
    user_id: &str,
    threads: &[String],
) -> Result<Value> {
    anyhow::ensure!(
        !account_id.trim().is_empty() && !user_id.trim().is_empty(),
        "Native desktop continuation needs a verified account and user identity"
    );
    anyhow::ensure!(
        !threads.is_empty() && threads.iter().all(|id| uuid::Uuid::parse_str(id).is_ok()),
        "Native desktop continuation needs valid local thread IDs"
    );
    let path = profile.join(STATE);
    let mut state = read(&path)?;
    let atoms = state
        .as_object_mut()
        .unwrap()
        .entry("electron-persisted-atom-state")
        .or_insert_with(|| json!({}));
    let atoms = atoms
        .as_object_mut()
        .context("Unsupported desktop atom state; continuation was not queued")?;
    let now = chrono::Utc::now().timestamp_millis();
    anyhow::ensure!(
        !atoms
            .get("update-resume-state-v1")
            .is_some_and(|old| !old.is_null()
                && old["expiresAtMs"]
                    .as_i64()
                    .is_none_or(|deadline| deadline > now)),
        "A native desktop continuation is already pending; it was left untouched"
    );
    let ids: std::collections::BTreeSet<_> = threads.iter().cloned().collect();
    atoms.insert("update-resume-state-v1".into(), json!({"accountId":account_id,"userId":user_id,"threadIds":ids,"expiresAtMs":now + 15 * 60 * 1000}));
    Ok(state)
}

pub fn validate_native_resume(
    profile: &Path,
    account_id: &str,
    user_id: &str,
    threads: &[String],
) -> Result<()> {
    native_resume_state(profile, account_id, user_id, threads).map(|_| ())
}

pub fn stage_native_resume(
    profile: &Path,
    shared: &Path,
    account_id: &str,
    user_id: &str,
    threads: &[String],
) -> Result<()> {
    let state = native_resume_state(profile, account_id, user_id, threads)?;
    save(&profile.join(STATE), &state, shared)
}

#[cfg(test)]
mod native_resume_tests {
    use super::*;

    #[test]
    fn native_preflight_rejects_before_any_state_mutation() {
        let root = std::env::temp_dir().join(format!("native-preflight-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let thread = uuid::Uuid::new_v4().to_string();
        assert!(validate_native_resume(&root, "", "user", &[thread.clone()]).is_err());
        assert!(!root.join(STATE).exists());
        assert!(!root.join("backups").exists());
        fs::write(root.join(STATE), br#"{"electron-persisted-atom-state":{"update-resume-state-v1":{"expiresAtMs":9223372036854775807}}}"#).unwrap();
        let before = fs::read(root.join(STATE)).unwrap();
        assert!(validate_native_resume(&root, "account", "user", &[thread]).is_err());
        assert_eq!(fs::read(root.join(STATE)).unwrap(), before);
        assert!(!root.join("backups").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_resume_is_bound_to_destination_and_preserves_other_state() {
        let root = std::env::temp_dir().join(format!("native-resume-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join(STATE),
            br#"{"electron-persisted-atom-state":{"other":42},"local-projects":{"keep":{}}}"#,
        )
        .unwrap();
        let thread = uuid::Uuid::new_v4().to_string();
        stage_native_resume(
            &root,
            &root,
            "target-account",
            "target-user",
            &[thread.clone()],
        )
        .unwrap();
        let state = read(&root.join(STATE)).unwrap();
        assert_eq!(state["electron-persisted-atom-state"]["other"], 42);
        let resume = &state["electron-persisted-atom-state"]["update-resume-state-v1"];
        assert_eq!(resume["accountId"], "target-account");
        assert_eq!(resume["userId"], "target-user");
        assert_eq!(resume["threadIds"], json!([thread]));
        assert!(resume["expiresAtMs"].as_i64().unwrap() > chrono::Utc::now().timestamp_millis());
        assert!(stage_native_resume(
            &root,
            &root,
            "target-account",
            "target-user",
            &[uuid::Uuid::new_v4().to_string()]
        )
        .is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_resume_rejects_missing_identity_and_invalid_threads() {
        let root =
            std::env::temp_dir().join(format!("native-resume-invalid-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        assert!(stage_native_resume(
            &root,
            &root,
            "",
            "user",
            &[uuid::Uuid::new_v4().to_string()]
        )
        .is_err());
        assert!(stage_native_resume(&root, &root, "account", "user", &["invalid".into()]).is_err());
        assert!(!root.join(STATE).exists());
        fs::remove_dir_all(root).unwrap();
    }
}
