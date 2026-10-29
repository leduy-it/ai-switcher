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
        && access_expiry(auth).is_some_and(|expiry| expiry <= now.saturating_add(10 * 60))
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct RenewalValidationError(String);

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
            "Native renewal changed the workspace or user identity; this profile needs attention"
                .into(),
        )
        .into());
    }
    if !access_expiry(after).is_some_and(|expiry| expiry > now.saturating_add(120)) {
        return Err(RenewalValidationError("Native renewal did not produce a verifiably fresh access token; check this profile in Codex".into()).into());
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
    static ATTEMPTS: OnceLock<Mutex<BTreeMap<PathBuf, (Instant, String, bool, String)>>> =
        OnceLock::new();
    use sha2::{Digest, Sha256};
    let fingerprint = format!(
        "{:x}",
        Sha256::digest(
            before
                .pointer("/tokens/refresh_token")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .as_bytes()
        )
    );
    {
        let attempts = ATTEMPTS
            .get_or_init(|| Mutex::new(BTreeMap::new()))
            .lock()
            .map_err(|_| anyhow::anyhow!("Credential renewal tracking unavailable"))?;
        if let Some((_, message, attention, _)) =
            attempts.get(profile).filter(|(at, _, hard, saved)| {
                saved == &fingerprint && (*hard || at.elapsed() < Duration::from_secs(60))
            })
        {
            if *attention {
                return Err(RenewalValidationError(message.clone()).into());
            }
            anyhow::bail!("{message}");
        }
    }
    let result = (|| -> Result<bool> {
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
            refresh_idle_profile(binary, profile)?;
        }
        let after = load(profile).map_err(|_| RenewalValidationError("Native renewal did not leave a valid Codex auth file; this profile needs attention".into()))?;
        validate_renewal(&before, &after, chrono::Utc::now().timestamp())?;
        Ok(true)
    })();
    if let Ok(mut attempts) = ATTEMPTS.get_or_init(|| Mutex::new(BTreeMap::new())).lock() {
        if let Err(error) = &result {
            attempts.insert(
                profile.to_path_buf(),
                (
                    Instant::now(),
                    error.to_string(),
                    renewal_requires_attention(error),
                    fingerprint,
                ),
            );
        } else {
            attempts.remove(profile);
        }
    }
    result
}

struct IsolatedAuthHome(PathBuf, bool);
impl Drop for IsolatedAuthHome {
    fn drop(&mut self) {
        if self.1 {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn refresh_idle_profile(binary: &Path, profile: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let original = std::fs::read(profile.join("auth.json"))
        .context("Could not read this profile's auth file")?;
    let (seed, before) = crate::codex_import::load(&crate::models::CodexAuthSourceInput {
        auth_file_path: None,
        auth_json: Some(String::from_utf8(original.clone()).context("Auth JSON is not UTF-8")?),
    })?;
    // Keep uncertain candidates in the private profile rather than an OS-cleaned temp folder.
    let mut home = IsolatedAuthHome(
        profile.join(format!(".native-auth-renewal-{}", uuid::Uuid::new_v4())),
        true,
    );
    std::fs::create_dir(&home.0)?;
    std::fs::set_permissions(&home.0, std::fs::Permissions::from_mode(0o700))?;
    crate::credential_export::write_private(&home.0.join("auth.json"), seed.as_bytes())?;
    home.1 = false;
    let outcome = (|| -> Result<()> {
        let result = run_native_refresh(binary, &home.0);
        let renewed = match std::fs::read(home.0.join("auth.json")) {
            Ok(bytes) => bytes,
            Err(error) => {
                return match result {
                    Err(native) => Err(native),
                    Ok(()) => Err(error.into()),
                }
            }
        };
        let after = match load(&home.0) {
            Ok(auth) => auth,
            Err(error) => {
                return match result {
                    Err(native) => Err(native),
                    Ok(()) => Err(error),
                }
            }
        };
        if let Err(validation) = validate_renewal(&before, &after, chrono::Utc::now().timestamp()) {
            return match result {
                Err(error) => Err(error),
                Ok(()) => Err(validation),
            };
        }
        std::fs::File::open(home.0.join("auth.json"))?.sync_all()?;
        std::fs::File::open(&home.0)?.sync_all()?;
        commit_renewed_auth(profile, &original, &renewed)
    })();
    match outcome {
        Ok(()) => {
            home.1 = true;
            Ok(())
        }
        Err(error) => {
            if std::fs::read(home.0.join("auth.json")).ok().as_deref() == Some(seed.as_bytes()) {
                home.1 = true;
                Err(error)
            } else {
                Err(RenewalValidationError(format!("{error}. The native credential snapshot was retained privately at {}; no uncertain candidate was discarded", home.0.display())).into())
            }
        }
    }
}

fn commit_renewed_auth(profile: &Path, original: &[u8], renewed: &[u8]) -> Result<()> {
    // Prepare+fsync candidate first. Recheck immediately before rename; external Codex clients do
    // not honor this manager's lock, so a cross-process race cannot be claimed impossible.
    let candidate = profile.join(format!(
        ".credential-renewal-recovery-{}.json",
        uuid::Uuid::new_v4()
    ));
    crate::credential_export::write_private(&candidate, renewed)?;
    let unchanged = std::fs::read(profile.join("auth.json"))
        .map(|bytes| bytes == original)
        .unwrap_or(false);
    if !unchanged || crate::desktop::profile_is_in_use(profile) {
        return Err(RenewalValidationError(format!("The profile changed or became active during renewal. Its credential was not overwritten; renewed auth was saved privately at {}", candidate.display())).into());
    }
    std::fs::rename(&candidate, profile.join("auth.json")).map_err(|_| {
        RenewalValidationError(format!(
            "Could not commit renewed auth; the private recovery file remains at {}",
            candidate.display()
        ))
    })?;
    std::fs::File::open(profile)?.sync_all()?;
    Ok(())
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
enum NativeRenewalFailure {
    #[error("The refresh token was already used. Obtain current auth.json from the working client or sign in; retrying this old file cannot renew it.")]
    AlreadyUsed,
    #[error("The refresh token expired or was revoked. Obtain current auth.json from the working client or sign in.")]
    InvalidRefresh,
    #[error("Native auth startup was blocked by history backfill; no renewal was completed.")]
    HistoryBackfill,
}

fn classify_native_stderr(line: &str) -> Option<NativeRenewalFailure> {
    let lower = line.to_ascii_lowercase();
    if lower.contains("refresh token was already used") || lower.contains("refresh_token_reused") {
        Some(NativeRenewalFailure::AlreadyUsed)
    } else if lower.contains("refresh_token_expired")
        || lower.contains("refresh token has expired")
        || lower.contains("refresh token expired")
        || lower.contains("invalid_grant")
        || lower.contains("refresh token was revoked")
    {
        Some(NativeRenewalFailure::InvalidRefresh)
    } else if lower.contains("state db backfill is running") {
        Some(NativeRenewalFailure::HistoryBackfill)
    } else {
        None
    }
}

pub fn renewal_requires_attention(error: &anyhow::Error) -> bool {
    error.downcast_ref::<RenewalValidationError>().is_some()
        || error
            .downcast_ref::<NativeRenewalFailure>()
            .is_some_and(|failure| {
                matches!(
                    failure,
                    NativeRenewalFailure::AlreadyUsed | NativeRenewalFailure::InvalidRefresh
                )
            })
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
    use std::sync::atomic::{AtomicU8, Ordering};
    let status = Arc::new(AtomicU8::new(0));
    let (sender, receiver) = std::sync::mpsc::channel();
    let result = run_native_protocol(binary, profile, timeout, status.clone(), sender);
    let _ = receiver.recv_timeout(Duration::from_millis(100));
    let failure = match status.load(Ordering::SeqCst) {
        3 => Some(NativeRenewalFailure::AlreadyUsed),
        2 => Some(NativeRenewalFailure::InvalidRefresh),
        1 => Some(NativeRenewalFailure::HistoryBackfill),
        _ => None,
    };
    if let Some(failure) = failure {
        return Err(failure.into());
    }
    result
}

fn run_native_protocol(
    binary: &Path,
    profile: &Path,
    timeout: Duration,
    error_status: Arc<std::sync::atomic::AtomicU8>,
    error_done: std::sync::mpsc::Sender<()>,
) -> Result<()> {
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
        .stderr(Stdio::piped());
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
    if let Some(stderr) = child.0.stderr.take() {
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stderr)
                .lines()
                .map_while(std::result::Result::ok)
            {
                if let Some(failure) = classify_native_stderr(&line) {
                    let priority = match failure {
                        NativeRenewalFailure::AlreadyUsed => 3,
                        NativeRenewalFailure::InvalidRefresh => 2,
                        NativeRenewalFailure::HistoryBackfill => 1,
                    };
                    error_status.fetch_max(priority, std::sync::atomic::Ordering::SeqCst);
                }
            }
            let _ = error_done.send(());
        });
    }
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
        json!({"id":1,"method":"initialize","params":{"clientInfo":{"name":"michael_profiles_auth","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":false}}}),
    )?;
    response(1)?;
    send(json!({"method":"initialized","params":{}}))?;
    let refresh = load(profile)
        .map(|auth| needs_renewal(&auth, chrono::Utc::now().timestamp()))
        .unwrap_or(true);
    send(json!({"id":2,"method":"account/read","params":{"refreshToken":refresh}}))?;
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
    fn missing_destination_keeps_a_private_renewed_candidate() {
        let dir =
            std::env::temp_dir().join(format!("native-auth-missing-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(commit_renewed_auth(&dir, b"original", b"rotated-refresh-credential").is_err());
        let candidate = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".credential-renewal-recovery-")
            })
            .unwrap();
        assert_eq!(
            std::fs::read(candidate).unwrap(),
            b"rotated-refresh-credential"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failed_renewal_validation_retains_rotated_credential() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("native-auth-retain-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let original=serde_json::to_vec(&json!({"tokens":{"access_token":token(1),"refresh_token":"old-refresh","account_id":"same"}})).unwrap();
        std::fs::write(dir.join("auth.json"), &original).unwrap();
        let binary = dir.join("fixture");
        std::fs::write(&binary,r#"#!/usr/bin/env python3
import sys,json,os,base64,time
from pathlib import Path
home=Path(os.environ['CODEX_HOME'])
for line in sys.stdin:
 m=json.loads(line)
 if m['method']=='initialize': print(json.dumps({'id':m['id'],'result':{}}),flush=True)
 if m['method']=='account/read':
  auth=json.loads((home/'auth.json').read_text())
  part=base64.urlsafe_b64encode(json.dumps({'exp':int(time.time())+5}).encode()).decode().rstrip('=')
  auth['tokens']['access_token']='synthetic.'+part+'.signature'; auth['tokens']['refresh_token']='new-rotated-refresh'
  (home/'auth.json').write_text(json.dumps(auth))
  print(json.dumps({'id':m['id'],'result':{'account':{'type':'chatgpt'}}}),flush=True)
"#).unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let error = ensure_fresh(&binary, &dir).unwrap_err().to_string();
        assert!(error.contains("retained privately"));
        assert_eq!(std::fs::read(dir.join("auth.json")).unwrap(), original);
        let recovery = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.is_dir()
                    && path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with(".native-auth-renewal-")
            })
            .unwrap();
        let saved: Value =
            serde_json::from_slice(&std::fs::read(recovery.join("auth.json")).unwrap()).unwrap();
        assert_eq!(saved["tokens"]["refresh_token"], "new-rotated-refresh");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn provider_reuse_reason_survives_cooldown_without_exposing_stderr() {
        use std::os::unix::fs::PermissionsExt;
        let dir =
            std::env::temp_dir().join(format!("native-auth-rejected-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let original=serde_json::to_vec(&json!({"tokens":{"access_token":token(1),"refresh_token":"synthetic-refresh","account_id":"same"}})).unwrap();
        std::fs::write(dir.join("auth.json"), &original).unwrap();
        let binary = dir.join("codex-fixture-rejected");
        std::fs::write(&binary,r#"#!/usr/bin/env python3
import sys,json
from pathlib import Path
root=Path(__file__).parent
count=root/'starts'; count.write_text(str(int(count.read_text())+1) if count.exists() else '1')
for line in sys.stdin:
 m=json.loads(line)
 if m['method']=='initialize': print(json.dumps({'id':m['id'],'result':{}}),flush=True)
 if m['method']=='account/read':
  print('state db backfill is running',file=sys.stderr,flush=True)
  print(json.dumps({'fields':{'message':'Your refresh token was already used. synthetic-secret-must-not-leak'}}),file=sys.stderr,flush=True)
  print(json.dumps({'id':m['id'],'result':{'account':None,'requiresOpenaiAuth':True}}),flush=True)
"#).unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let first = ensure_fresh(&binary, &dir).unwrap_err().to_string();
        let second = ensure_fresh(&binary, &dir).unwrap_err().to_string();
        assert!(first.contains("already used"));
        assert!(!first.contains("synthetic-secret"));
        assert_eq!(first, second);
        assert_eq!(std::fs::read_to_string(dir.join("starts")).unwrap(), "1");
        assert_eq!(std::fs::read(dir.join("auth.json")).unwrap(), original);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn renewed_auth_never_overwrites_a_concurrently_changed_profile() {
        let dir = std::env::temp_dir().join(format!("auth-cas-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("auth.json"), b"newer-client-auth").unwrap();
        assert!(commit_renewed_auth(&dir, b"original-auth", b"renewed-auth").is_err());
        assert_eq!(
            std::fs::read(dir.join("auth.json")).unwrap(),
            b"newer-client-auth"
        );
        let recovery = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".credential-renewal-recovery-")
            })
            .unwrap();
        assert_eq!(std::fs::read(recovery).unwrap(), b"renewed-auth");
        std::fs::remove_dir_all(dir).unwrap();
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
        assert!(needs_renewal(
            &json!({"tokens":{"access_token":token(now+300),"refresh_token":"synthetic"}}),
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
        std::fs::create_dir(dir.join("sessions")).unwrap();
        std::fs::write(dir.join("sessions").join("history-must-stay"), "preserve").unwrap();
        let binary = dir.join("codex-fixture");
        std::fs::write(&binary,r#"#!/usr/bin/env python3
import sys,json,os,base64,time
from pathlib import Path
home=Path(os.environ['CODEX_HOME']); fixture=Path(__file__).parent; methods=[]
if home == fixture or (home/'sessions').exists():
 print(json.dumps({'id':1,'error':{'message':'auth subprocess must not open shared history'}}),flush=True); sys.exit(1)
for line in sys.stdin:
 msg=json.loads(line); methods.append(msg['method'])
 if msg['method']=='initialize': print(json.dumps({'id':msg['id'],'result':{}}),flush=True)
 if msg['method']=='account/read':
  assert msg['params']['refreshToken'] is True
  auth=json.loads((home/'auth.json').read_text())
  payload=base64.urlsafe_b64encode(json.dumps({'exp':int(time.time())+3600}).encode()).decode().rstrip('=')
  auth['tokens']['access_token']='synthetic.'+payload+'.signature'
  auth['tokens']['refresh_token']='synthetic-rotated'
  (home/'auth.json').write_text(json.dumps(auth)); (fixture/'methods.json').write_text(json.dumps(methods))
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
        assert_eq!(
            std::fs::read_to_string(dir.join("sessions/history-must-stay")).unwrap(),
            "preserve"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
