//! Native Codex token renewal without inference. Credential values never leave this module.

use anyhow::{Context, Result};
use base64::Engine;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{BufRead, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

fn access_expiry(auth: &Value) -> Option<i64> {
    auth.pointer("/tokens/access_token")
        .and_then(Value::as_str)
        .and_then(|token| token.split('.').nth(1))
        .and_then(|part| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(part)
                .ok()
        })
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|claims| claims["exp"].as_i64())
}

fn needs_renewal(auth: &Value, now: i64) -> bool {
    auth.pointer("/tokens/refresh_token")
        .and_then(Value::as_str)
        .is_some_and(|token| !token.is_empty())
        && access_expiry(auth).is_some_and(|expiry| expiry <= now.saturating_add(120))
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct RenewalValidationError(&'static str);

fn validate_renewal(before: &Value, after: &Value, now: i64) -> Result<()> {
    let old = [
        crate::quota::codex_account_id_from_auth(before),
        crate::quota::codex_user_id_from_auth(before),
        crate::quota::codex_account_email_from_auth(before).map(|email| email.to_lowercase()),
    ];
    let new = [
        crate::quota::codex_account_id_from_auth(after),
        crate::quota::codex_user_id_from_auth(after),
        crate::quota::codex_account_email_from_auth(after).map(|email| email.to_lowercase()),
    ];
    if old.iter().zip(&new).any(|(expected, actual)| {
        expected
            .as_ref()
            .is_some_and(|value| actual.as_ref() != Some(value))
    }) {
        return Err(RenewalValidationError(
            "Native renewal changed the workspace or user identity; this profile needs attention",
        )
        .into());
    }
    if !access_expiry(after).is_some_and(|expiry| expiry > now.saturating_add(120)) {
        return Err(RenewalValidationError("Native renewal did not produce a verifiably fresh access token; check this profile in Codex").into());
    }
    Ok(())
}

fn load(profile: &Path) -> Result<Value> {
    crate::codex_import::load(&crate::models::CodexAuthSourceInput {
        auth_file_path: Some(profile.join("auth.json")),
        auth_json: None,
    })
    .map(|(_, auth)| auth)
}

/// Refresh through the owning native backend, or a bounded auth-only native process for an idle
/// profile. Never launch a competing refresher under an active CLI/private desktop.
pub fn ensure_fresh(binary: &Path, profile: &Path) -> Result<bool> {
    let before = load(profile)?;
    if !needs_renewal(&before, chrono::Utc::now().timestamp()) {
        return Ok(false);
    }
    static LOCKS: OnceLock<Mutex<BTreeMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();
    let lock = LOCKS
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .map_err(|_| anyhow::anyhow!("Credential renewal lock unavailable"))?
        .entry(profile.to_path_buf())
        .or_default()
        .clone();
    let _guard = lock
        .lock()
        .map_err(|_| anyhow::anyhow!("Credential renewal lock unavailable"))?;
    let before = load(profile)?;
    if !needs_renewal(&before, chrono::Utc::now().timestamp()) {
        return Ok(false);
    }
    static ATTEMPTS: OnceLock<Mutex<BTreeMap<PathBuf, Instant>>> = OnceLock::new();
    {
        let mut attempts = ATTEMPTS
            .get_or_init(|| Mutex::new(BTreeMap::new()))
            .lock()
            .map_err(|_| anyhow::anyhow!("Credential renewal tracking unavailable"))?;
        anyhow::ensure!(
            !attempts
                .get(profile)
                .is_some_and(|at| at.elapsed() < Duration::from_secs(60)),
            "Native renewal was just attempted; retry shortly or open this profile in Codex"
        );
        attempts.insert(profile.to_path_buf(), Instant::now());
    }
    let owned = crate::desktop::runtime()
        .into_iter()
        .find(|runtime| runtime.running && runtime.profile_home.as_deref() == Some(profile));
    if let Some(runtime) = owned {
        let mut client = crate::desktop_rpc::Client::connect_desktop(&runtime).map_err(|_| anyhow::anyhow!("This profile is owned by a private desktop backend; use that desktop to renew, or close it before refreshing"))?;
        client
            .request("account/read", json!({"refreshToken":true}))
            .map_err(|_| {
                anyhow::anyhow!("The owning Codex desktop could not renew this credential")
            })?;
    } else {
        anyhow::ensure!(!crate::desktop::profile_is_in_use(profile), "This profile is in use by Codex CLI or a private desktop; let that client renew or close it before refreshing");
        run_native_refresh(binary, profile)?;
    }
    let after = load(profile).map_err(|_| {
        RenewalValidationError(
            "Native renewal did not leave a valid Codex auth file; this profile needs attention",
        )
    })?;
    validate_renewal(&before, &after, chrono::Utc::now().timestamp())?;
    Ok(true)
}

struct NativeChild(Child);
impl Drop for NativeChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn run_native_refresh(binary: &Path, profile: &Path) -> Result<()> {
    run_native_refresh_with_timeout(binary, profile, Duration::from_secs(20))
}

fn run_native_refresh_with_timeout(binary: &Path, profile: &Path, timeout: Duration) -> Result<()> {
    let mut command = Command::new(binary);
    command
        .args([
            "app-server",
            "--listen",
            "stdio://",
            "-c",
            "cli_auth_credentials_store=\"file\"",
        ])
        .arg("-c")
        .arg(format!(
            "sqlite_home={}",
            serde_json::to_string(&profile.to_string_lossy())?
        ))
        .env("CODEX_HOME", profile)
        .env("CODEX_SQLITE_HOME", profile)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for key in [
        "OPENAI_API_KEY",
        "CODEX_API_KEY",
        "CODEX_ACCESS_TOKEN",
        "ACCESS_TOKEN",
        "CODEX_APP_SERVER_WS_URL",
    ] {
        command.env_remove(key);
    }
    let mut child = NativeChild(
        command
            .spawn()
            .context("Could not start the configured Codex binary for auth renewal")?,
    );
    let stdout = child
        .0
        .stdout
        .take()
        .context("Codex auth output unavailable")?;
    let (sender, receiver) = std::sync::mpsc::sync_channel(16);
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stdout).lines() {
            let Ok(line) = line else {
                break;
            };
            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                if sender.send(value).is_err() {
                    break;
                }
            }
        }
    });
    let deadline = Instant::now() + timeout;
    let mut send = |value: Value| -> Result<()> {
        let stdin = child
            .0
            .stdin
            .as_mut()
            .context("Codex auth input unavailable")?;
        writeln!(stdin, "{value}").context("Codex auth process closed")?;
        stdin.flush().context("Codex auth process closed")?;
        Ok(())
    };
    let response = |id: u64| -> Result<Value> {
        loop {
            anyhow::ensure!(
                Instant::now() < deadline,
                "Native Codex renewal timed out; its auth-only process was stopped"
            );
            let value = receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())).map_err(|_| anyhow::anyhow!("Native Codex renewal timed out or exited; credential values were not logged"))?;
            anyhow::ensure!(
                value.get("method").is_none() || value.get("id").is_none(),
                "Auth renewal unexpectedly requested a tool; no request was executed"
            );
            if value["id"] == json!(id) {
                anyhow::ensure!(value.get("error").is_none(), "Codex rejected native token renewal; import current credentials or sign in to this profile");
                return value
                    .get("result")
                    .cloned()
                    .context("Codex returned no auth renewal result");
            }
        }
    };
    send(
        json!({"id":1,"method":"initialize","params":{"clientInfo":{"name":"michael_profiles_auth","version":env!("CARGO_PKG_VERSION")}}}),
    )?;
    response(1)?;
    send(json!({"method":"initialized","params":{}}))?;
    send(json!({"id":2,"method":"account/read","params":{"refreshToken":true}}))?;
    let account = response(2)?;
    anyhow::ensure!(account["account"]["type"] == "chatgpt", "Codex could not renew the managed ChatGPT login; import current auth.json or sign in again");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use serde_json::json;

    fn token(exp: i64) -> String {
        format!(
            "synthetic.{}.signature",
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(serde_json::to_vec(&json!({"exp":exp})).unwrap())
        )
    }

    #[test]
    fn renewal_rejects_another_user_in_same_workspace_and_unverifiable_expiry() {
        let now = chrono::Utc::now().timestamp();
        let id = |user: &str| {
            format!(
                "synthetic.{}.signature",
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
                    serde_json::to_vec(
                        &json!({"user_id":user,"email":format!("{user}@example.invalid")})
                    )
                    .unwrap()
                )
            )
        };
        let before = json!({"account_id":"workspace","tokens":{"access_token":token(1),"refresh_token":"synthetic","id_token":id("alice")}});
        let mut after = before.clone();
        after["tokens"]["access_token"] = json!(token(now + 3600));
        assert!(validate_renewal(&before, &after, now).is_ok());
        after["tokens"]["id_token"] = json!(id("bob"));
        assert!(validate_renewal(&before, &after, now).is_err());
        after["tokens"]["id_token"] = json!(id("alice"));
        after["tokens"]["access_token"] = json!("opaque-unknown");
        assert!(validate_renewal(&before, &after, now).is_err());
        after["tokens"]["access_token"] = json!(token(now + 3600));
        after["account_id"] = json!("another-workspace");
        assert!(validate_renewal(&before, &after, now).is_err());
    }

    #[test]
    fn renewal_notification_flood_cannot_extend_deadline() {
        use std::os::unix::fs::PermissionsExt;
        let dir =
            std::env::temp_dir().join(format!("native-auth-timeout-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let binary = dir.join("fixture");
        std::fs::write(
            &binary,
            r#"#!/bin/sh
echo $$ > "$CODEX_HOME/pid"
while :; do printf '%s\n' '{"method":"irrelevant-notification"}'; done
"#,
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let start = Instant::now();
        assert!(run_native_refresh_with_timeout(&binary, &dir, Duration::from_secs(1)).is_err());
        assert!(start.elapsed() < Duration::from_secs(5));
        let pid: i32 = std::fs::read_to_string(dir.join("pid"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn refreshes_expiring_access_token_not_short_lived_id_token() {
        let now = chrono::Utc::now().timestamp();
        assert!(needs_renewal(
            &json!({"tokens":{"access_token":token(now-1),"refresh_token":"synthetic"}}),
            now
        ));
        assert!(needs_renewal(
            &json!({"tokens":{"access_token":token(now+60),"refresh_token":"synthetic"}}),
            now
        ));
        assert!(!needs_renewal(
            &json!({"tokens":{"access_token":token(now+3600),"id_token":token(now-1),"refresh_token":"synthetic"}}),
            now
        ));
        assert!(!needs_renewal(
            &json!({"tokens":{"access_token":"unknown","refresh_token":"synthetic"}}),
            now
        ));
        assert!(!needs_renewal(
            &json!({"tokens":{"access_token":token(now-1)}}),
            now
        ));
    }

    #[test]
    fn native_subprocess_renews_without_starting_an_inference_turn() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("native-auth-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let auth = json!({"tokens":{"access_token":token(1),"refresh_token":"synthetic-refresh","account_id":"same-account"}});
        std::fs::write(dir.join("auth.json"), serde_json::to_vec(&auth).unwrap()).unwrap();
        let binary = dir.join("codex-fixture");
        std::fs::write(&binary,r#"#!/usr/bin/env python3
import sys,json,os,base64,time
from pathlib import Path
home=Path(os.environ['CODEX_HOME']); methods=[]
for line in sys.stdin:
 msg=json.loads(line); methods.append(msg['method'])
 if msg['method']=='initialize': print(json.dumps({'id':msg['id'],'result':{}}),flush=True)
 if msg['method']=='account/read':
  assert msg['params']['refreshToken'] is True
  auth=json.loads((home/'auth.json').read_text())
  payload=base64.urlsafe_b64encode(json.dumps({'exp':int(time.time())+3600}).encode()).decode().rstrip('=')
  auth['tokens']['access_token']='synthetic.'+payload+'.signature'
  auth['tokens']['refresh_token']='synthetic-rotated'
  (home/'auth.json').write_text(json.dumps(auth)); (home/'methods.json').write_text(json.dumps(methods))
  print(json.dumps({'id':msg['id'],'result':{'account':{'type':'chatgpt'}}}),flush=True)
"#).unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(ensure_fresh(&binary, &dir).unwrap());
        assert!(!ensure_fresh(&binary, &dir).unwrap());
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("auth.json")).unwrap()).unwrap();
        assert!(!needs_renewal(&saved, chrono::Utc::now().timestamp()));
        let methods: Vec<String> =
            serde_json::from_slice(&std::fs::read(dir.join("methods.json")).unwrap()).unwrap();
        assert_eq!(methods, vec!["initialize", "initialized", "account/read"]);
        assert_eq!(saved["tokens"]["account_id"], "same-account");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
