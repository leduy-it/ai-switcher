//! Read/control the existing local backend; never start a separate inference server.
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::{
    path::Path,
    time::{Duration, Instant},
};

/// Secret comparisons stay in Rust; callers receive only static failures or account metadata.
pub fn verify_credential(account: &Value, status: &Value, auth: &Value) -> Result<()> {
    let expected = auth
        .pointer("/tokens/access_token")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty());
    anyhow::ensure!(account["account"]["type"] == "chatgpt" && status["authMethod"] == "chatgpt"
        && expected.is_some() && status["authToken"].as_str() == expected,
        "The desktop backend has not loaded the current managed credential; a safe reload is required");
    let workspace = crate::quota::codex_account_id_from_auth(auth)
        .context("The selected credential has no workspace identity")?;
    anyhow::ensure!(
        account["workspaceRouting"]["chatgptAccountId"].as_str() == Some(workspace.as_str()),
        "The desktop workspace has not been confirmed against the selected credential"
    );
    Ok(())
}

fn read_loaded_threads(
    mut request: impl FnMut(&str, Value) -> Result<Value>,
) -> Result<Vec<Value>> {
    let mut result = Vec::new();
    let mut cursor = Value::Null;
    let mut cursors = std::collections::BTreeSet::new();
    let mut ids = std::collections::BTreeSet::new();
    for _ in 0..100 {
        let page = request("thread/loaded/list", json!({"cursor":cursor,"limit":100}))?;
        for id in page["data"]
            .as_array()
            .context("Desktop did not report its loaded sessions")?
        {
            let id = id
                .as_str()
                .filter(|id| !id.is_empty())
                .context("Desktop returned an invalid loaded thread ID")?;
            if !ids.insert(id.to_string()) {
                continue;
            }
            anyhow::ensure!(ids.len() <= 1000, "Too many sessions to checkpoint safely");
            let value = request("thread/read", json!({"threadId":id,"includeTurns":false}))?;
            anyhow::ensure!(
                value["thread"].is_object() && value["thread"]["id"] == id,
                "Desktop returned metadata for a different thread"
            );
            result.push(value["thread"].clone());
        }
        cursor = page.get("nextCursor").cloned().unwrap_or(Value::Null);
        if cursor.is_null() {
            return Ok(result);
        }
        let next = cursor
            .as_str()
            .context("Desktop returned an invalid pagination cursor")?;
        anyhow::ensure!(
            cursors.insert(next.to_string()),
            "Desktop repeated a loaded-thread cursor; nothing was interrupted"
        );
    }
    anyhow::bail!("Too many loaded-thread pages to checkpoint safely")
}

#[cfg(unix)]
pub struct Client {
    socket: tungstenite::WebSocket<std::os::unix::net::UnixStream>,
    next_id: u64,
}

#[cfg(target_os = "macos")]
fn verify_peer(stream: &std::os::unix::net::UnixStream, expected: i32) -> Result<()> {
    use std::os::fd::AsRawFd;
    let mut actual: libc::pid_t = 0;
    let mut size = std::mem::size_of_val(&actual) as libc::socklen_t;
    let ok = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_LOCAL,
            libc::LOCAL_PEERPID,
            (&mut actual as *mut libc::pid_t).cast(),
            &mut size,
        )
    };
    anyhow::ensure!(
        ok == 0 && actual == expected && expected > 0,
        "Desktop control socket belongs to another process; handoff stopped"
    );
    Ok(())
}

#[cfg(unix)]
impl Client {
    pub fn connect_desktop(runtime: &crate::models::DesktopRuntime) -> Result<Self> {
        let home = runtime
            .profile_home
            .as_deref()
            .context("Desktop execution profile is unknown")?;
        let backend = runtime
            .backend_pid
            .context("Desktop backend ownership could not be confirmed")?;
        // A private stdio desktop and a CLI daemon can use the same profile simultaneously. The
        // daemon's account/read or thread/loaded/list does not describe that desktop's active work.
        anyhow::ensure!(runtime.uses_managed_proxy || runtime.uses_daemon_proxy || runtime.uses_local_daemon || crate::desktop::daemon_pid(home) == Some(backend),
            "This desktop uses a private backend that Profiles cannot control. Finish its active work and quit it before applying the new account; a separate CLI daemon was left untouched");
        if let Some(socket) = runtime
            .control_socket
            .as_ref()
            .filter(|_| runtime.uses_managed_proxy)
        {
            return Self::connect_socket(socket, backend);
        }
        Self::connect_peer(home, backend)
    }
    pub fn connect(home: &Path) -> Result<Self> {
        if let Some((socket, pid)) = crate::desktop::managed_endpoint(home) {
            return Self::connect_socket(&socket, pid);
        }
        let expected = crate::desktop::daemon_pid(home)
            .context("Native daemon process identity is unavailable")?;
        Self::connect_peer(home, expected)
    }
    fn connect_peer(home: &Path, expected: i32) -> Result<Self> {
        // Managed profile paths exceed macOS's Unix-socket path limit. Codex publishes a
        // symlink to a short /private/tmp socket; resolve it before calling connect().
        let socket_path = home
            .join("app-server-control/app-server-control.sock")
            .canonicalize()
            .context("Desktop backend is not reachable")?;
        Self::connect_socket(&socket_path, expected)
    }
    fn connect_socket(socket_path: &Path, expected: i32) -> Result<Self> {
        let stream = std::os::unix::net::UnixStream::connect(socket_path)
            .context("Desktop backend is not reachable")?;
        #[cfg(target_os = "macos")]
        verify_peer(&stream, expected)?;
        #[cfg(not(target_os = "macos"))]
        let _ = expected;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let (socket, _) = tungstenite::client("ws://codex-app-server/rpc", stream)
            .map_err(|_| anyhow::anyhow!("Desktop backend handshake failed"))?;
        let mut client = Self { socket, next_id: 1 };
        client.request("initialize", json!({"clientInfo":{"name":"michael_profiles","title":"Michael Le Profiles","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}}))?;
        client.socket.send(tungstenite::Message::Text(
            json!({"method":"initialized","params":{}}).to_string(),
        ))?;
        Ok(client)
    }

    pub fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.socket.send(tungstenite::Message::Text(
            json!({"id":id,"method":method,"params":params}).to_string(),
        ))?;
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let message = self
                .socket
                .read()
                .context("Desktop backend connection was lost")?;
            let tungstenite::Message::Text(text) = message else {
                continue;
            };
            let value: Value =
                serde_json::from_str(&text).context("Invalid desktop backend response")?;
            if value.get("id") == Some(&json!(id)) && value.get("method").is_none() {
                if value.get("error").is_some() {
                    if method == "account/read" {
                        if let Some(error) =
                            crate::codex_auth::sanitized_native_failure(&value["error"].to_string())
                        {
                            return Err(error);
                        }
                    }
                    anyhow::bail!(
                        "Desktop backend rejected {method}; open the session in its desktop app"
                    );
                }
                return value
                    .get("result")
                    .cloned()
                    .context("Desktop backend returned no result");
            }
            // Never execute a desktop-owned tool or silently approve its request.
            if value.get("method").is_some() && value.get("id").is_some() {
                self.socket.send(tungstenite::Message::Text(json!({"id":value["id"],"error":{"code":-32601,"message":"This recovery connection cannot execute native desktop tools. Open the original thread in the desktop app."}}).to_string()))?;
                anyhow::bail!("This session needs its desktop tool runtime; open it to continue");
            }
        }
        anyhow::bail!("Desktop backend response timed out")
    }

    pub fn current_credential(&mut self, profile: &Path) -> Result<Value> {
        let account = self.request("account/read", json!({"refreshToken":false}))?;
        let status = self.request(
            "getAuthStatus",
            json!({"includeToken":true,"refreshToken":false}),
        )?;
        let (_, auth) = crate::codex_import::load(&crate::models::CodexAuthSourceInput {
            auth_file_path: Some(profile.join("auth.json")),
            auth_json: None,
        })?;
        verify_credential(&account, &status, &auth)?;
        Ok(account)
    }

    pub fn threads(&mut self) -> Result<Vec<Value>> {
        let deadline = Instant::now() + Duration::from_secs(30);
        read_loaded_threads(|method, params| {
            anyhow::ensure!(
                Instant::now() < deadline,
                "Loaded-thread inspection timed out; nothing was interrupted"
            );
            self.request(method, params)
        })
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn loaded_thread_pagination_rejects_repeating_empty_cursors() {
        let mut calls = 0;
        let result = read_loaded_threads(|method, _| {
            assert_eq!(method, "thread/loaded/list");
            calls += 1;
            assert!(calls <= 2);
            Ok(json!({"data":[],"nextCursor":"same"}))
        });
        assert!(result.is_err());
        assert_eq!(calls, 2);
    }

    #[test]
    fn loaded_thread_pagination_deduplicates_ids_and_rejects_wrong_metadata() {
        let mut pages = 0;
        let mut reads = 0;
        let threads=read_loaded_threads(|method,_| {
            if method=="thread/loaded/list" { pages+=1; return Ok(json!({"data":["thread"],"nextCursor":if pages==1 {json!("next")} else {Value::Null}})); }
            reads+=1; Ok(json!({"thread":{"id":"thread"}}))
        }).unwrap();
        assert_eq!(threads.len(), 1);
        assert_eq!(reads, 1);
        assert!(
            read_loaded_threads(|method, _| Ok(if method == "thread/loaded/list" {
                json!({"data":["thread"],"nextCursor":null})
            } else {
                json!({"thread":{"id":"other"}})
            }))
            .is_err()
        );
    }

    #[test]
    fn identity_confirmation_rejects_stale_cached_tokens_and_unknown_workspace() {
        let auth =
            json!({"tokens":{"access_token":"new-private-credential","account_id":"workspace"}});
        let response = json!({"account":{"type":"chatgpt"},"workspaceRouting":{"chatgptAccountId":"workspace"}});
        let mut status = json!({"authMethod":"chatgpt","authToken":"new-private-credential"});
        assert!(verify_credential(&response, &status, &auth).is_ok());
        status["authToken"] = json!("old-private-credential");
        let error = verify_credential(&response, &status, &auth)
            .unwrap_err()
            .to_string();
        assert!(
            !error.contains("new-private-credential") && !error.contains("old-private-credential")
        );
        status["authToken"] = json!("new-private-credential");
        assert!(verify_credential(&json!({"account":{"type":"chatgpt"}}), &status, &auth).is_err());
        assert!(verify_credential(
            &json!({"account":{"type":"chatgpt"},"workspaceRouting":{"chatgptAccountId":"other"}}),
            &status,
            &auth
        )
        .is_err());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn verifies_the_actual_unix_peer_instead_of_trusting_a_pid_file() {
        let path = format!(
            "/private/tmp/mp-peer-{}.sock",
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        );
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let stream = std::os::unix::net::UnixStream::connect(&path).unwrap();
        let (_accepted, _) = listener.accept().unwrap();
        let pid = std::process::id() as i32;
        assert!(verify_peer(&stream, pid).is_ok());
        assert!(verify_peer(&stream, pid + 1).is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn refuses_a_cli_daemon_as_a_private_desktop_backend() -> Result<()> {
        let root =
            std::env::temp_dir().join(format!("desktop-owner-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("app-server-daemon"))?;
        std::fs::write(root.join("app-server-daemon/daemon.pid"), r#"{"pid":123}"#)?;
        let runtime = crate::models::DesktopRuntime {
            app: crate::models::DesktopApp::Chatgpt,
            installed: true,
            running: true,
            pid: Some(20),
            backend_pid: Some(456),
            uses_daemon_proxy: false,
            uses_local_daemon: false,
            uses_managed_proxy: false,
            control_socket: None,
            profile_home: Some(root.clone()),
            session_home: Some(root.clone()),
        };
        let error = Client::connect_desktop(&runtime)
            .err()
            .context("Wrong backend was accepted")?;
        assert!(error.to_string().contains("private backend"));
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}

#[cfg(not(unix))]
pub struct Client;
#[cfg(not(unix))]
impl Client {
    pub fn connect_desktop(_runtime: &crate::models::DesktopRuntime) -> Result<Self> {
        anyhow::bail!("Desktop recovery currently supports macOS")
    }
    pub fn connect(_home: &Path) -> Result<Self> {
        anyhow::bail!("Desktop recovery currently supports macOS")
    }
    pub fn request(&mut self, _method: &str, _params: Value) -> Result<Value> {
        anyhow::bail!("Desktop recovery currently supports macOS")
    }
    pub fn current_credential(&mut self, _profile: &Path) -> Result<Value> {
        anyhow::bail!("Desktop recovery currently supports macOS")
    }
    pub fn threads(&mut self) -> Result<Vec<Value>> {
        anyhow::bail!("Desktop recovery currently supports macOS")
    }
}
