//! Read/control the existing local backend; never start a separate inference server.
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::{
    path::Path,
    time::{Duration, Instant},
};

#[cfg(unix)]
pub struct Client {
    socket: tungstenite::WebSocket<std::os::unix::net::UnixStream>,
    next_id: u64,
}

#[cfg(unix)]
impl Client {
    pub fn connect_desktop(runtime: &crate::models::DesktopRuntime) -> Result<Self> {
        let home = runtime.profile_home.as_deref().context("Desktop execution profile is unknown")?;
        let backend = runtime.backend_pid.context("Desktop backend ownership could not be confirmed")?;
        // A private stdio desktop and a CLI daemon can use the same profile simultaneously. The
        // daemon's account/read or thread/loaded/list does not describe that desktop's active work.
        anyhow::ensure!(runtime.uses_daemon_proxy || crate::desktop::daemon_pid(home) == Some(backend),
            "This desktop uses a private backend that Profiles cannot control. Finish its active work and quit it before applying the new account; a separate CLI daemon was left untouched");
        Self::connect(home)
    }
    pub fn connect(home: &Path) -> Result<Self> {
        // Managed profile paths exceed macOS's Unix-socket path limit. Codex publishes a
        // symlink to a short /private/tmp socket; resolve it before calling connect().
        let socket_path = home
            .join("app-server-control/app-server-control.sock")
            .canonicalize()
            .context("Desktop backend is not reachable")?;
        let stream = std::os::unix::net::UnixStream::connect(socket_path)
            .context("Desktop backend is not reachable")?;
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

    pub fn threads(&mut self) -> Result<Vec<Value>> {
        let mut result = Vec::new();
        let mut cursor = Value::Null;
        loop {
            let page = self.request("thread/loaded/list", json!({"cursor":cursor,"limit":100}))?;
            for id in page["data"]
                .as_array()
                .context("Desktop did not report its loaded sessions")?
            {
                let value =
                    self.request("thread/read", json!({"threadId":id,"includeTurns":false}))?;
                result.push(value["thread"].clone());
            }
            cursor = page.get("nextCursor").cloned().unwrap_or(Value::Null);
            if cursor.is_null() {
                return Ok(result);
            }
            anyhow::ensure!(
                result.len() < 1000,
                "Too many sessions to checkpoint safely"
            );
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn refuses_a_cli_daemon_as_a_private_desktop_backend() -> Result<()> {
        let root = std::env::temp_dir().join(format!("desktop-owner-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("app-server-daemon"))?;
        std::fs::write(root.join("app-server-daemon/daemon.pid"), r#"{"pid":123}"#)?;
        let runtime = crate::models::DesktopRuntime {
            app: crate::models::DesktopApp::Chatgpt, installed: true, running: true, pid: Some(20),
            backend_pid: Some(456), uses_daemon_proxy: false,
            profile_home: Some(root.clone()), session_home: Some(root.clone()),
        };
        let error = Client::connect_desktop(&runtime).err().context("Wrong backend was accepted")?;
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
    pub fn threads(&mut self) -> Result<Vec<Value>> {
        anyhow::bail!("Desktop recovery currently supports macOS")
    }
}
