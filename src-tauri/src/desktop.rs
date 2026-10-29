//! macOS desktop handoff. Credentials stay in the selected profile; nothing copies auth.json.
use crate::models::{DesktopApp, DesktopRuntime, DesktopSyncSettings};
use anyhow::{Context, Result};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn bundle(app: &DesktopApp) -> PathBuf {
    let name = match app {
        DesktopApp::Codex => "Codex.app",
        DesktopApp::Chatgpt => "ChatGPT.app",
    };
    let system = PathBuf::from("/Applications").join(name);
    if system.exists() {
        system
    } else {
        crate::tools::home_dir().join("Applications").join(name)
    }
}

fn executable(app: &DesktopApp) -> PathBuf {
    bundle(app).join("Contents/MacOS").join(match app {
        DesktopApp::Codex => "Codex",
        DesktopApp::Chatgpt => "ChatGPT",
    })
}

fn backend_executable(app: &DesktopApp) -> PathBuf {
    bundle(app).join(match app {
        DesktopApp::Codex => "Contents/Resources/codex",
        DesktopApp::Chatgpt => "Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex",
    })
}

#[derive(Clone)]
struct Process {
    pid: i32,
    parent: i32,
    executable: PathBuf,
}

fn process_records() -> Vec<Process> {
    let Ok(output) = Command::new("/bin/ps")
        .args(["-ax", "-o", "pid=,ppid=,comm="])
        .output()
    else {
        return vec![];
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let first = line.find(char::is_whitespace)?;
            let pid = line[..first].parse().ok()?;
            let rest = line[first..].trim();
            let second = rest.find(char::is_whitespace)?;
            Some(Process {
                pid,
                parent: rest[..second].parse().ok()?,
                executable: rest[second..].trim().into(),
            })
        })
        .collect()
}

pub fn open_thread(app: &DesktopApp, id: &str) -> Result<()> {
    uuid::Uuid::parse_str(id).context("Invalid session ID")?;
    // Opening history must not start another brand over the same live desktop data folder.
    let running = runtime().into_iter().find(|r| r.running);
    let app = running.as_ref().map(|r| &r.app).unwrap_or(app);
    let status = Command::new("/usr/bin/open")
        .arg("-a")
        .arg(bundle(app))
        .arg(format!("codex://threads/{id}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    anyhow::ensure!(
        status.success(),
        "Could not open the original desktop session"
    );
    Ok(())
}

fn processes() -> Vec<(i32, PathBuf)> {
    process_records()
        .into_iter()
        .map(|p| (p.pid, p.executable))
        .collect()
}

#[derive(Clone, Default)]
struct Launch {
    paths: BTreeMap<String, PathBuf>,
    app_server: bool,
    daemon_proxy: bool,
    local_daemon: bool,
    force_cli: bool,
    alternate_transport: bool,
}

/// Read only the two non-secret launch paths. Never expose/log the process environment.
#[cfg(target_os = "macos")]
fn launch_info(pid: i32) -> Option<Launch> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    let mut size = 0usize;
    unsafe {
        if libc::sysctl(
            mib.as_mut_ptr(),
            3,
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        ) != 0
            || size < 4
            || size > 4 * 1024 * 1024
        {
            return None;
        }
        let mut bytes = vec![0u8; size];
        if libc::sysctl(
            mib.as_mut_ptr(),
            3,
            bytes.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return None;
        }
        bytes.truncate(size);
        let argc = i32::from_ne_bytes(bytes.get(..4)?.try_into().ok()?);
        if !(0..=10000).contains(&argc) {
            return None;
        }
        let mut cursor = 4 + bytes.get(4..)?.iter().position(|b| *b == 0)? + 1;
        while bytes.get(cursor) == Some(&0) {
            cursor += 1;
        }
        let mut app_server = false;
        let mut daemon_proxy = false;
        let mut previous_app_server = false;
        for _ in 0..argc {
            let length = bytes.get(cursor..)?.iter().position(|b| *b == 0)?;
            let arg = &bytes[cursor..cursor + length];
            daemon_proxy |= previous_app_server && arg == b"proxy";
            previous_app_server = arg == b"app-server";
            app_server |= previous_app_server;
            cursor += length + 1;
        }
        let mut paths = BTreeMap::new();
        let mut local_daemon = false;
        let mut force_cli = false;
        let mut alternate_transport = false;
        for entry in bytes.get(cursor..)?.split(|b| *b == 0) {
            local_daemon |= entry == b"CODEX_APP_SERVER_USE_LOCAL_DAEMON=1";
            force_cli |= entry == b"CODEX_APP_SERVER_FORCE_CLI=1";
            for key in [b"CODEX_CLI_PATH=".as_slice(), b"CODEX_APP_SERVER_WS_URL="] {
                alternate_transport |= entry.strip_prefix(key).is_some_and(|v| !v.is_empty());
            }
            for key in ["CODEX_HOME", "CODEX_SQLITE_HOME"] {
                if let Some(value) = entry.strip_prefix(format!("{key}=").as_bytes()) {
                    let value = std::str::from_utf8(value).ok()?;
                    if !value.is_empty() {
                        let path = PathBuf::from(value);
                        paths.insert(key.to_string(), path.canonicalize().unwrap_or(path));
                    }
                }
            }
        }
        Some(Launch {
            paths,
            app_server,
            daemon_proxy,
            local_daemon,
            force_cli,
            alternate_transport,
        })
    }
}

#[cfg(not(target_os = "macos"))]
fn launch_info(_pid: i32) -> Option<Launch> {
    None
}

fn launch_paths(pid: i32) -> Option<BTreeMap<String, PathBuf>> {
    launch_info(pid).map(|launch| launch.paths)
}

fn owned_backend(
    parent: i32,
    expected: &Path,
    records: &[Process],
    read: impl Fn(i32) -> Option<Launch>,
) -> Option<(i32, Launch)> {
    let mut matches = records
        .iter()
        .filter(|p| p.parent == parent && p.executable == expected)
        .filter_map(|p| {
            read(p.pid)
                .filter(|info| info.app_server)
                .map(|info| (p.pid, info))
        });
    let first = matches.next()?;
    // Multiple candidates require an explicit owner; never guess which process has active work.
    if matches.next().is_some() {
        return None;
    }
    Some(first)
}

fn desktop_backend(
    pid: Option<i32>,
    expected: &Path,
    frontend: Option<&Launch>,
    daemon: Option<i32>,
    records: &[Process],
    read: impl Fn(i32) -> Option<Launch>,
) -> Option<(i32, Launch, bool)> {
    let pid = pid?;
    let private = records.iter().any(|p| {
        p.parent == pid
            && p.executable == expected
            && read(p.pid).is_some_and(|info| info.app_server)
    });
    if private {
        return owned_backend(pid, expected, records, &read).map(|(pid, info)| (pid, info, false));
    }
    local_daemon_backend(frontend?, daemon, records, read).map(|(pid, info)| (pid, info, true))
}

fn local_daemon_backend(
    frontend: &Launch,
    daemon_pid: Option<i32>,
    records: &[Process],
    read: impl Fn(i32) -> Option<Launch>,
) -> Option<(i32, Launch)> {
    if !frontend.local_daemon || frontend.force_cli || frontend.alternate_transport {
        return None;
    }
    let home = frontend.paths.get("CODEX_HOME")?;
    let shared = frontend.paths.get("CODEX_SQLITE_HOME")?;
    let pid = daemon_pid?;
    let process = records.iter().find(|p| p.pid == pid)?;
    let executable = process
        .executable
        .canonicalize()
        .unwrap_or_else(|_| process.executable.clone());
    // Native daemon start installs its own bundled release. A CLI/private server elsewhere must
    // never become the controller merely because it happens to use the same CODEX_HOME.
    if !executable.starts_with(home.join("packages/app-server-daemon/releases"))
        || executable.file_name().is_none_or(|name| name != "codex")
    {
        return None;
    }
    let info = read(pid)?;
    (info.app_server
        && info.paths.get("CODEX_HOME") == Some(home)
        && info.paths.get("CODEX_SQLITE_HOME") == Some(shared))
    .then_some((pid, info))
}

#[cfg(all(unix, test))]
fn desktop_home_alias_in(root: &Path, profile: &Path) -> Result<PathBuf> {
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    let home = profile
        .canonicalize()
        .context("The selected Codex profile is unavailable")?;
    match std::fs::DirBuilder::new().mode(0o700).create(root) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(error) => return Err(error.into()),
    }
    let meta = std::fs::symlink_metadata(root)?;
    anyhow::ensure!(
        meta.is_dir()
            && !meta.file_type().is_symlink()
            && meta.uid() == unsafe { libc::geteuid() }
            && meta.permissions().mode() & 0o077 == 0,
        "The private desktop launch folder is not secure"
    );
    use std::os::unix::ffi::OsStrExt;
    let digest = format!("{:x}", Sha256::digest(home.as_os_str().as_bytes()));
    let alias = root.join(&digest[..16]);
    anyhow::ensure!(
        alias
            .join("app-server-control/app-server-control.sock")
            .as_os_str()
            .as_bytes()
            .len()
            < 104,
        "The desktop control path exceeds macOS's socket limit"
    );
    match std::os::unix::fs::symlink(&home, &alias) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(error) => return Err(error.into()),
    }
    anyhow::ensure!(
        std::fs::symlink_metadata(&alias)?.file_type().is_symlink()
            && alias.canonicalize()? == home,
        "The short desktop launch path belongs to a different profile"
    );
    Ok(alias)
}

fn desktop_launch_command(
    app: &DesktopApp,
    home: &Path,
    shared: &Path,
    user_data: &Path,
    wrapper: &Path,
) -> Command {
    let mut command = Command::new("/usr/bin/open");
    command
        .args(["-n", "--env"])
        .arg(format!("CODEX_HOME={}", home.display()))
        .arg("--env")
        .arg(format!("CODEX_SQLITE_HOME={}", shared.display()))
        .arg("--env")
        .arg(format!(
            "CODEX_ELECTRON_USER_DATA_PATH={}",
            user_data.display()
        ));
    for value in [
        "CODEX_APP_SERVER_USE_LOCAL_DAEMON=0",
        "CODEX_APP_SERVER_FORCE_CLI=0",
        "CODEX_APP_RUNTIME_CLI_PATH=",
        "CODEX_APP_SERVER_WS_URL=",
    ] {
        command.arg("--env").arg(value);
    }
    command
        .arg("--env")
        .arg(format!("CODEX_CLI_PATH={}", wrapper.display()));
    command.arg("--env").arg(format!(
        "MICHAEL_PROFILES_REAL_CODEX={}",
        backend_executable(app).display()
    ));
    for key in [
        "OPENAI_API_KEY",
        "CODEX_API_KEY",
        "CODEX_ACCESS_TOKEN",
        "ACCESS_TOKEN",
    ] {
        command.env_remove(key);
    }
    command
        .arg(bundle(app))
        .arg("--args")
        .arg(format!("--user-data-dir={}", user_data.display()))
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

fn run_daemon_command(app: &DesktopApp, profile: &Path, shared: &Path, action: &str) -> Result<()> {
    run_daemon_binary(&backend_executable(app), profile, shared, action)
}

struct DaemonCommand(std::process::Child);
impl Drop for DaemonCommand {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn run_daemon_binary(binary: &Path, profile: &Path, shared: &Path, action: &str) -> Result<()> {
    let mut command = Command::new(binary);
    command
        .args([
            "-c",
            "cli_auth_credentials_store=\"file\"",
            "-c",
            "features.code_mode_host=true",
            "app-server",
            "daemon",
            action,
        ])
        .env("CODEX_HOME", profile)
        .env("CODEX_SQLITE_HOME", shared)
        .stdout(Stdio::null())
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
    let mut child = DaemonCommand(
        command
            .spawn()
            .context("Could not prepare the native desktop daemon")?,
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.0.try_wait()? {
            anyhow::ensure!(status.success(), "Native desktop daemon setup failed; existing credentials and sessions were preserved");
            return Ok(());
        }
        if Instant::now() >= deadline {
            anyhow::bail!("Native desktop daemon setup timed out");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub(crate) fn daemon_pid(home: &Path) -> Option<i32> {
    let raw = std::fs::read_to_string(home.join("app-server-daemon/daemon.pid")).ok()?;
    parse_daemon_pid(&raw)
}

fn parse_daemon_pid(raw: &str) -> Option<i32> {
    let pid = raw.trim().parse::<i32>().ok().or_else(|| {
        serde_json::from_str::<serde_json::Value>(raw)
            .ok()?
            .get("pid")?
            .as_i64()?
            .try_into()
            .ok()
    })?;
    (pid > 0).then_some(pid)
}

pub fn validate_catalog_config(profile: &Path, shared: &Path) -> Result<()> {
    if let Ok(text) = std::fs::read_to_string(profile.join("config.toml")) {
        let config = text
            .parse::<toml_edit::DocumentMut>()
            .context("Cannot read the profile configuration before desktop handoff")?;
        if let Some(path) = config.get("sqlite_home").and_then(|v| v.as_str()) {
            let path = PathBuf::from(path);
            let shared = shared
                .canonicalize()
                .context("The main Codex catalog directory does not exist")?;
            anyhow::ensure!(path.is_absolute() && path.canonicalize().ok().as_ref() == Some(&shared), "config.toml overrides sqlite_home outside the shared catalog. Set it to the main Codex home before applying desktop");
        }
    }
    Ok(())
}

fn profile_has_other_clients_in(
    profile: &Path,
    daemon: i32,
    frontend: Option<i32>,
    allow_proxies: bool,
    records: &[Process],
    read: impl Fn(i32) -> Option<Launch>,
) -> bool {
    let owned = |process: &Process| {
        let mut pid = process.pid;
        for _ in 0..128 {
            if pid == daemon || frontend == Some(pid) {
                return true;
            }
            let Some(parent) = records.iter().find(|p| p.pid == pid).map(|p| p.parent) else {
                break;
            };
            if parent == pid || parent <= 1 {
                break;
            }
            pid = parent;
        }
        false
    };
    records
        .iter()
        .filter(|p| {
            p.executable.file_name().is_some_and(|name| {
                let name = name.to_string_lossy().to_ascii_lowercase();
                name == "codex" || name.starts_with("codex-")
            }) && !owned(p)
        })
        .any(|p| {
            let Some(info) = read(p.pid) else {
                return true;
            };
            if allow_proxies && info.daemon_proxy {
                return false;
            }
            let home =
                info.paths.get("CODEX_HOME").cloned().unwrap_or_else(|| {
                    crate::tools::default_config_dir(&crate::models::ToolId::Codex)
                });
            home.canonicalize().unwrap_or(home) == profile
        })
}

pub fn ensure_exclusive_desktop(runtime: &DesktopRuntime) -> Result<()> {
    let home = runtime
        .profile_home
        .as_deref()
        .context("Desktop profile is unknown")?;
    let daemon = runtime.backend_pid.context("Desktop backend is unknown")?;
    anyhow::ensure!(!profile_has_other_clients_in(home, daemon, runtime.pid, false, &process_records(), launch_info),
        "Another CLI/client is using this account. Finish that client's work before desktop restart; it was left untouched");
    Ok(())
}

/// An idle native daemon still owns its managed auth. Refresh through that owner rather than
/// copying its credential into another AuthManager. A private client on the same home blocks this.
pub fn idle_managed_daemon_owner(profile: &Path) -> Option<i32> {
    let home = profile.canonicalize().ok()?;
    let pid = daemon_pid(&home)?;
    let records = process_records();
    let info = launch_info(pid)?;
    let frontend = Launch {
        paths: info.paths.clone(),
        local_daemon: true,
        ..Default::default()
    };
    local_daemon_backend(&frontend, Some(pid), &records, launch_info)?;
    (!profile_has_other_clients_in(&home, pid, None, true, &records, launch_info)).then_some(pid)
}

/// Inspect only process executable and profile paths. Unknown Codex ownership blocks renewal.
pub fn profile_is_in_use(profile: &Path) -> bool {
    let target = profile
        .canonicalize()
        .unwrap_or_else(|_| profile.to_path_buf());
    process_records()
        .iter()
        .filter(|process| {
            process.executable.file_name().is_some_and(|name| {
                let name = name.to_string_lossy().to_ascii_lowercase();
                name == "codex" || name.starts_with("codex-")
            })
        })
        .any(|process| {
            let Some(info) = launch_info(process.pid) else {
                return true;
            };
            let home =
                info.paths.get("CODEX_HOME").cloned().unwrap_or_else(|| {
                    crate::tools::default_config_dir(&crate::models::ToolId::Codex)
                });
            home.canonicalize().unwrap_or(home) == target
        })
}

/// Bind the private record to the live GUI -> adapter -> native backend process chain.
fn managed_proxy(
    frontend: i32,
    home: &Path,
    shared: &Path,
    native: &Path,
    records: &[Process],
) -> Option<crate::desktop_proxy::Record> {
    let record = crate::desktop_proxy::read_record(home).ok()?;
    if record.frontend_pid != frontend
        || record.shared_home != shared
        || record.native_binary != native.canonicalize().ok()?
    {
        return None;
    }
    let proxy = records.iter().find(|p| {
        p.pid == record.proxy_pid
            && p.parent == frontend
            && p.executable == record.manager_executable
    })?;
    let backend = records.iter().find(|p| {
        p.pid == record.backend_pid && p.parent == proxy.pid && p.executable == record.native_binary
    })?;
    let info = launch_info(backend.pid)?;
    if !info.app_server
        || info.paths.get("CODEX_HOME") != Some(&record.profile_home)
        || info.paths.get("CODEX_SQLITE_HOME") != Some(&record.shared_home)
    {
        return None;
    }
    Some(record)
}

pub fn managed_endpoint(home: &Path) -> Option<(PathBuf, i32)> {
    let home = home.canonicalize().ok()?;
    runtime()
        .into_iter()
        .find(|r| r.running && r.uses_managed_proxy && r.profile_home.as_ref() == Some(&home))
        .and_then(|r| Some((r.control_socket?, r.backend_pid?)))
}

pub fn runtime() -> Vec<DesktopRuntime> {
    let processes = process_records();
    [DesktopApp::Codex, DesktopApp::Chatgpt]
        .into_iter()
        .flat_map(|app| {
            let path = executable(&app);
            let pids: Vec<_> = processes
                .iter()
                .filter(|p| p.executable == path)
                .map(|p| p.pid)
                .collect();
            let pids: Vec<Option<i32>> = if pids.is_empty() {
                vec![None]
            } else {
                pids.into_iter().map(Some).collect()
            };
            let backend = backend_executable(&app);
            let processes = processes.clone();
            pids.into_iter().map(move |pid| {
                let frontend = pid.and_then(launch_info);
                let daemon = frontend
                    .as_ref()
                    .and_then(|info| info.paths.get("CODEX_HOME"))
                    .and_then(|home| daemon_pid(home));
                let resolved = desktop_backend(
                    pid,
                    &backend,
                    frontend.as_ref(),
                    daemon,
                    &processes,
                    launch_info,
                );
                let proxy = frontend.as_ref().and_then(|f| {
                    managed_proxy(
                        pid?,
                        f.paths.get("CODEX_HOME")?,
                        f.paths.get("CODEX_SQLITE_HOME")?,
                        &backend,
                        &processes,
                    )
                });
                let uses_local_daemon = resolved.as_ref().is_some_and(|(_, _, direct)| *direct);
                let owned = resolved.map(|(pid, info, _)| (pid, info));
                let paths = owned
                    .as_ref()
                    .map(|(_, info)| {
                        let mut paths = info.paths.clone();
                        let home = paths
                            .entry("CODEX_HOME".into())
                            .or_insert_with(|| {
                                crate::tools::default_config_dir(&crate::models::ToolId::Codex)
                            })
                            .clone();
                        paths.entry("CODEX_SQLITE_HOME".into()).or_insert(home);
                        paths
                    })
                    .or_else(|| frontend.as_ref().map(|info| info.paths.clone()));
                DesktopRuntime {
                    installed: path.exists(),
                    app: app.clone(),
                    running: pid.is_some(),
                    pid,
                    backend_pid: proxy
                        .as_ref()
                        .map(|r| r.backend_pid)
                        .or_else(|| owned.as_ref().map(|(pid, _)| *pid)),
                    uses_managed_proxy: proxy.is_some(),
                    control_socket: proxy.as_ref().map(|r| r.control_socket.clone()),
                    uses_daemon_proxy: owned.as_ref().is_some_and(|(_, info)| info.daemon_proxy),
                    uses_local_daemon,
                    profile_home: paths.as_ref().and_then(|p| p.get("CODEX_HOME").cloned()),
                    session_home: paths
                        .as_ref()
                        .and_then(|p| p.get("CODEX_SQLITE_HOME").cloned()),
                }
            })
        })
        .collect()
}

pub fn backend_requires_restart(profile: &Path, shared: &Path) -> bool {
    backend_catalog_home(profile).is_some_and(|home| home != shared)
}

pub fn backend_catalog_home(profile: &Path) -> Option<PathBuf> {
    if let Some((_, pid)) = managed_endpoint(profile) {
        return launch_paths(pid)?.get("CODEX_SQLITE_HOME").cloned();
    }
    let pid = daemon_pid(profile)?;
    if !processes()
        .iter()
        .any(|(p, path)| *p == pid && path.file_name().is_some_and(|n| n == "codex"))
    {
        return None;
    }
    let paths = launch_paths(pid)?;
    let home = paths
        .get("CODEX_HOME")
        .cloned()
        .unwrap_or_else(|| crate::tools::default_config_dir(&crate::models::ToolId::Codex));
    let sqlite = paths
        .get("CODEX_SQLITE_HOME")
        .cloned()
        .unwrap_or_else(|| home.clone());
    (home == profile).then_some(sqlite)
}

/// Prepare the destination before any interruption. An already live destination is checked again
/// after normal desktop exit, so its active backend is never restarted by this preflight.
pub fn prepare_target_daemon(
    settings: &DesktopSyncSettings,
    profile: &Path,
    shared: &Path,
    before_interrupt: bool,
) -> Result<PathBuf> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (settings, profile, shared, before_interrupt);
        anyhow::bail!("Desktop switching currently supports macOS");
    }
    #[cfg(target_os = "macos")]
    {
        anyhow::ensure!(
            supports_local_daemon(&settings.app),
            "This desktop version does not support managed local-daemon switching"
        );
        validate_catalog_config(profile, shared)?;
        let wrapper =
            crate::desktop_proxy::wrapper(&backend_executable(&settings.app), profile, shared)?;
        anyhow::ensure!(
            backend_executable(&settings.app).is_file(),
            "Native desktop CLI is unavailable"
        );
        let live = runtime()
            .iter()
            .any(|r| r.running && r.profile_home.as_deref() == Some(profile));
        if live {
            anyhow::ensure!(before_interrupt, "The destination desktop is still running");
            return Ok(wrapper);
        }
        if daemon_pid(profile).is_some_and(|pid| processes().iter().any(|(p, _)| *p == pid)) {
            anyhow::ensure!(
                idle_managed_daemon_owner(profile).is_some(),
                "Another client owns the destination credential; finish its work before switching"
            );
            anyhow::ensure!(
                !profile_has_other_clients_in(
                    profile,
                    daemon_pid(profile).context("Destination owner disappeared")?,
                    None,
                    false,
                    &process_records(),
                    launch_info
                ),
                "Another CLI client uses the destination; it was left running"
            );
            let mut client = crate::desktop_rpc::Client::connect(profile)?;
            for thread in client.threads()? {
                anyhow::ensure!(
                    thread["status"]["type"] != "active",
                    "The destination backend has active work"
                );
                let terminals = client.request(
                    "thread/backgroundTerminals/list",
                    serde_json::json!({"threadId":thread["id"],"limit":100}),
                )?;
                anyhow::ensure!(
                    terminals["data"]
                        .as_array()
                        .is_some_and(|data| data.is_empty())
                        && terminals["nextCursor"].is_null(),
                    "The destination has background terminals"
                );
            }
            drop(client);
            run_daemon_command(&settings.app, profile, shared, "stop")?;
        }
        anyhow::ensure!(
            !profile_is_in_use(profile),
            "Another Codex client owns the destination credential"
        );
        Ok(wrapper)
    }
}

/// Cache the installed native contract by file identity; an app update invalidates the cache.
fn native_capabilities_at(path: &Path) -> (bool, bool) {
    use std::io::Read;
    use std::sync::{Mutex, OnceLock};
    type Entry = (u64, Option<std::time::SystemTime>, (bool, bool));
    static CACHE: OnceLock<Mutex<BTreeMap<PathBuf, Entry>>> = OnceLock::new();
    let Ok(meta) = std::fs::metadata(path) else {
        return (false, false);
    };
    let modified = meta.modified().ok();
    if let Ok(cache) = CACHE.get_or_init(Default::default).lock() {
        if let Some((length, at, value)) = cache.get(path) {
            if *length == meta.len() && *at == modified {
                return *value;
            }
        }
    }
    let Ok(mut file) = std::fs::File::open(path) else {
        return (false, false);
    };
    let markers: [&[u8]; 5] = [
        b"CODEX_APP_SERVER_USE_LOCAL_DAEMON",
        b"app-server-control.sock",
        b"update-resume-state-v1",
        b"takeResumeState",
        b"app_update_resume",
    ];
    let mut found = [false; 5];
    let mut tail = Vec::new();
    let mut buffer = [0u8; 65536];
    loop {
        let Ok(count) = file.read(&mut buffer) else {
            return (false, false);
        };
        if count == 0 {
            break;
        }
        tail.extend_from_slice(&buffer[..count]);
        for (index, marker) in markers.iter().enumerate() {
            if !found[index] {
                found[index] = tail.windows(marker.len()).any(|window| window == *marker);
            }
        }
        if found.iter().all(|value| *value) {
            break;
        }
        if tail.len() > 64 {
            tail.drain(..tail.len() - 64);
        }
    }
    let value = (found[..2].iter().all(|v| *v), found[2..].iter().all(|v| *v));
    if let Ok(mut cache) = CACHE.get_or_init(Default::default).lock() {
        cache.insert(path.into(), (meta.len(), modified, value));
    }
    value
}

pub fn supports_native_resume(app: &DesktopApp) -> bool {
    native_capabilities_at(&bundle(app).join("Contents/Resources/app.asar")).1
}

fn supports_local_daemon(app: &DesktopApp) -> bool {
    native_capabilities_at(&bundle(app).join("Contents/Resources/app.asar")).0
}

pub fn can_reuse_running_desktop(
    running: bool,
    needs_refresh: bool,
    identity_matches: bool,
    resume_threads: Option<&[(String, String)]>,
) -> bool {
    running
        && !needs_refresh
        && identity_matches
        && resume_threads.is_none_or(|threads| threads.is_empty())
}

pub fn preflight_native_resume(
    app: &DesktopApp,
    profile: &Path,
    threads: &[String],
) -> Result<(String, String)> {
    anyhow::ensure!(
        supports_native_resume(app),
        "This desktop version does not support native continuation"
    );
    let (_, auth) = crate::codex_import::load(&crate::models::CodexAuthSourceInput {
        auth_file_path: Some(profile.join("auth.json")),
        auth_json: None,
    })?;
    let account = crate::quota::codex_account_id_from_auth(&auth)
        .context("Native continuation needs an account ID")?;
    let user = crate::quota::codex_user_id_from_auth(&auth)
        .context("Native continuation needs a user ID")?;
    crate::desktop_state::validate_native_resume(profile, &account, &user, &threads)?;
    Ok((account, user))
}

#[cfg(test)]
mod native_handoff_tests {
    #[test]
    fn pending_native_resume_never_reuses_running_desktop() {
        assert!(!super::can_reuse_running_desktop(
            true,
            false,
            true,
            Some(&[("thread".into(), "turn".into())])
        ));
        assert!(super::can_reuse_running_desktop(true, false, true, None));
        assert!(!super::can_reuse_running_desktop(true, true, true, None));
        assert!(!super::can_reuse_running_desktop(true, false, false, None));
    }
}

pub fn apply(
    settings: &DesktopSyncSettings,
    profile: &Path,
    shared: &Path,
    api: bool,
    state_profiles: &[PathBuf],
    resume_threads: Option<&[(String, String)]>,
) -> Result<()> {
    #[cfg(not(target_os = "macos"))]
    anyhow::bail!("Desktop switching currently supports macOS");
    #[cfg(target_os = "macos")]
    {
        anyhow::ensure!(!api, "Desktop handoff currently supports subscription profiles. API profiles remain available through their CLI launcher");
        anyhow::ensure!(
            executable(&settings.app).exists(),
            "The selected desktop app is not installed"
        );
        validate_catalog_config(profile, shared)?;
        anyhow::ensure!(
            supports_local_daemon(&settings.app),
            "This desktop version does not support managed local-daemon switching"
        );
        // Both brands use the same vendor ID and default userData. Keep that store and run one
        // brand at a time, rather than racing two versions over its live browser databases.
        let paths = [
            executable(&DesktopApp::Codex),
            executable(&DesktopApp::Chatgpt),
        ];
        let pids: Vec<_> = processes()
            .into_iter()
            .filter(|(_, path)| paths.contains(path))
            .map(|(pid, _)| pid)
            .collect();
        let mut outgoing_homes = Vec::new();
        for runtime in runtime().into_iter().filter(|r| r.running) {
            let home = runtime.profile_home.clone().context(
                "Desktop profile is unknown; finish its work and quit it before applying",
            )?;
            let mut client = crate::desktop_rpc::Client::connect_desktop(&runtime)?;
            anyhow::ensure!(!client.threads()?.iter().any(|t| t["status"]["type"] == "active"), "New work started before the handoff. Desktop was left running; retry at a safe point");
            if let Some(candidates) = resume_threads {
                for (id, turn) in candidates {
                    let latest = client.request(
                        "thread/read",
                        serde_json::json!({"threadId":id,"includeTurns":true}),
                    )?;
                    anyhow::ensure!(
                        crate::desktop_recovery::original_recoverable_turn(&latest["thread"], turn),
                        "Session changed before desktop exit; continuation was not queued"
                    );
                }
            }
            outgoing_homes.push(home);
        }
        for pid in &pids {
            // Recheck executable immediately before requesting normal application quit.
            if processes()
                .iter()
                .any(|(p, path)| p == pid && paths.contains(path))
            {
                if let Some(application) =
                    objc2_app_kit::NSRunningApplication::runningApplicationWithProcessIdentifier(
                        *pid,
                    )
                {
                    anyhow::ensure!(
                        application.terminate(),
                        "Desktop declined to quit; finish its work and apply again"
                    );
                }
            }
        }
        let deadline = Instant::now() + Duration::from_secs(15);
        while processes().iter().any(|(_, path)| paths.contains(path)) {
            anyhow::ensure!(
                Instant::now() < deadline,
                "Desktop app did not quit; finish its work and apply again. No force-quit was used"
            );
            std::thread::sleep(Duration::from_millis(200));
        }
        // Reconcile again after quit: a desktop running from the main home may have flushed its
        // in-memory state over the earlier repair. Include every OAuth home and its legacy copies.
        outgoing_homes.extend_from_slice(state_profiles);
        outgoing_homes.sort();
        outgoing_homes.dedup();
        crate::desktop_state::prepare_handoff(&outgoing_homes, profile, shared)?;
        let session_home = if api { profile } else { shared };
        let wrapper = prepare_target_daemon(settings, profile, session_home, false)?;
        if let Some(candidates) = resume_threads.filter(|threads| !threads.is_empty()) {
            let threads: Vec<String> = candidates.iter().map(|(id, _)| id.clone()).collect();
            let (account, user) = preflight_native_resume(&settings.app, profile, &threads)?;
            crate::desktop_state::stage_native_resume(profile, shared, &account, &user, &threads)?;
        }
        let user_data = crate::tools::home_dir().join("Library/Application Support/Codex");
        std::fs::create_dir_all(&user_data)?;
        let status =
            desktop_launch_command(&settings.app, profile, session_home, &user_data, &wrapper)
                .status()
                .context("Could not launch the desktop app")?;
        anyhow::ensure!(
            status.success(),
            "macOS could not launch the desktop app; apply again"
        );
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if runtime().iter().any(|r| {
                r.app == settings.app
                    && r.running
                    && r.profile_home.as_deref() == Some(profile)
                    && r.session_home.as_deref() == Some(session_home)
            }) {
                return Ok(());
            }
            anyhow::ensure!(Instant::now() < deadline, "Desktop launched, but its profile could not be verified. Check the desktop account menu before using it");
            std::thread::sleep(Duration::from_millis(250));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Requires an isolated native desktop fixture with synthetic OAuth"]
    fn native_proxy_fixture_ownership() {
        let home = PathBuf::from(
            std::env::var_os("AISW_DESKTOP_FIXTURE_HOME").expect("isolated fixture home"),
        );
        assert!(home.file_name().is_some_and(|n| n == "profile-b"));
        assert!(home
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("profiles-native-desktop-fixture-"));
        let home = home.canonicalize().unwrap();
        let runtime = runtime()
            .into_iter()
            .find(|r| r.uses_managed_proxy && r.profile_home.as_ref() == Some(&home))
            .expect("Verified GUI-to-proxy-to-native ownership");
        assert_eq!(backend_catalog_home(&home), runtime.session_home);
        let mut client = crate::desktop_rpc::Client::connect_desktop(&runtime).unwrap();
        let account = client.current_credential(&home).unwrap();
        assert_eq!(account["account"]["email"], "fixture-b@example.invalid");
        assert!(!client.threads().unwrap().is_empty());
    }

    #[test]
    fn reads_legacy_and_current_daemon_pid_records() {
        assert_eq!(parse_daemon_pid("123\n"), Some(123));
        assert_eq!(
            parse_daemon_pid(r#"{"pid":456,"processIdentity":{"uniqueId":789}}"#),
            Some(456)
        );
        for invalid in [
            "0",
            "-1",
            "null",
            "{}",
            r#"{"pid":2147483648}"#,
            r#"{"pid":"456"}"#,
        ] {
            assert_eq!(parse_daemon_pid(invalid), None);
        }
    }

    #[test]
    fn discovers_only_the_frontends_owned_app_server() {
        let expected = PathBuf::from("/fixture/bundled/codex");
        let records = vec![
            Process {
                pid: 11,
                parent: 10,
                executable: expected.clone(),
            },
            Process {
                pid: 12,
                parent: 10,
                executable: expected.clone(),
            },
            Process {
                pid: 13,
                parent: 1,
                executable: expected.clone(),
            },
        ];
        let read = |pid| {
            Some(Launch {
                paths: BTreeMap::from([("CODEX_HOME".into(), PathBuf::from("/fixture/profile"))]),
                app_server: pid != 12,
                daemon_proxy: false,
                ..Default::default()
            })
        };
        let (pid, info) = owned_backend(10, &expected, &records, read).unwrap();
        assert_eq!(pid, 11);
        assert_eq!(info.paths["CODEX_HOME"], PathBuf::from("/fixture/profile"));
        assert!(owned_backend(99, &expected, &records, read).is_none());
    }

    #[test]
    fn ambiguous_desktop_backends_are_not_guessed() {
        let expected = PathBuf::from("/fixture/bundled/codex");
        let records = vec![
            Process {
                pid: 11,
                parent: 10,
                executable: expected.clone(),
            },
            Process {
                pid: 12,
                parent: 10,
                executable: expected.clone(),
            },
        ];
        assert!(owned_backend(10, &expected, &records, |_| Some(Launch {
            paths: BTreeMap::new(),
            app_server: true,
            daemon_proxy: true,
            ..Default::default()
        }))
        .is_none());
    }
}

#[cfg(all(test, target_os = "macos"))]
mod managed_transport_tests {
    use super::*;

    fn launch(home: &str, daemon: bool) -> Launch {
        Launch {
            paths: BTreeMap::from([
                ("CODEX_HOME".into(), home.into()),
                ("CODEX_SQLITE_HOME".into(), "/shared".into()),
            ]),
            app_server: false,
            daemon_proxy: false,
            local_daemon: daemon,
            force_cli: false,
            alternate_transport: false,
        }
    }

    #[test]
    fn a_private_child_always_wins_over_a_separate_daemon() {
        let expected = PathBuf::from("/app/codex");
        let records = vec![
            Process {
                pid: 11,
                parent: 10,
                executable: expected.clone(),
            },
            Process {
                pid: 12,
                parent: 1,
                executable: "/profile/packages/app-server-daemon/releases/version/bin/codex".into(),
            },
        ];
        let read = |_| {
            let mut info = launch("/profile", false);
            info.app_server = true;
            Some(info)
        };
        let resolved = desktop_backend(
            Some(10),
            &expected,
            Some(&launch("/profile", true)),
            Some(12),
            &records,
            read,
        )
        .unwrap();
        assert_eq!(resolved.0, 11);
        assert!(!resolved.2);
        let mut ambiguous = records.clone();
        ambiguous.push(Process {
            pid: 13,
            parent: 10,
            executable: expected.clone(),
        });
        assert!(desktop_backend(
            Some(10),
            &expected,
            Some(&launch("/profile", true)),
            Some(12),
            &ambiguous,
            read
        )
        .is_none());
    }

    #[test]
    fn unrelated_cli_clients_block_recovery_but_proxies_share_the_native_auth_owner() {
        let mut records = vec![
            Process {
                pid: 12,
                parent: 1,
                executable: "/profile/packages/app-server-daemon/releases/v/bin/codex".into(),
            },
            Process {
                pid: 13,
                parent: 12,
                executable: "/tools/codex-code-mode-host".into(),
            },
        ];
        let read = |_| Some(launch("/profile", false));
        assert!(!profile_has_other_clients_in(
            Path::new("/profile"),
            12,
            None,
            false,
            &records,
            read
        ));
        records.push(Process {
            pid: 14,
            parent: 1,
            executable: "/tools/codex".into(),
        });
        assert!(profile_has_other_clients_in(
            Path::new("/profile"),
            12,
            None,
            false,
            &records,
            read
        ));
        let proxy = |pid| {
            let mut info = launch("/profile", false);
            info.daemon_proxy = pid == 14;
            Some(info)
        };
        assert!(!profile_has_other_clients_in(
            Path::new("/profile"),
            12,
            None,
            true,
            &records,
            proxy
        ));
        assert!(profile_has_other_clients_in(
            Path::new("/profile"),
            12,
            None,
            false,
            &records,
            proxy
        ));
    }

    #[test]
    fn direct_daemon_requires_frontend_opt_in_and_same_profile() {
        let frontend = launch("/profile", true);
        let records = vec![Process {
            pid: 12,
            parent: 1,
            executable: "/profile/packages/app-server-daemon/releases/version/bin/codex".into(),
        }];
        let read = |_| {
            let mut info = launch("/profile", false);
            info.app_server = true;
            Some(info)
        };
        assert!(local_daemon_backend(&frontend, Some(12), &records, read).is_some());
        assert!(local_daemon_backend(&launch("/other", true), Some(12), &records, read).is_none());
        assert!(
            local_daemon_backend(&launch("/profile", false), Some(12), &records, read).is_none()
        );
        assert!(local_daemon_backend(&frontend, Some(99), &records, read).is_none());
        let mut overridden = frontend.clone();
        overridden.force_cli = true;
        assert!(local_daemon_backend(&overridden, Some(12), &records, read).is_none());
        overridden = frontend.clone();
        overridden.alternate_transport = true;
        assert!(local_daemon_backend(&overridden, Some(12), &records, read).is_none());
    }

    #[test]
    fn daemon_cannot_claim_an_unrelated_binary_or_catalog() {
        let frontend = launch("/profile", true);
        let records = vec![Process {
            pid: 12,
            parent: 1,
            executable: "/other/codex".into(),
        }];
        let read = |_| {
            let mut info = launch("/profile", false);
            info.app_server = true;
            Some(info)
        };
        assert!(local_daemon_backend(&frontend, Some(12), &records, read).is_none());
        let records = vec![Process {
            pid: 12,
            parent: 1,
            executable: "/profile/packages/app-server-daemon/releases/version/bin/codex".into(),
        }];
        let read = |_| {
            let mut info = launch("/profile", false);
            info.app_server = true;
            info.paths
                .insert("CODEX_SQLITE_HOME".into(), "/other".into());
            Some(info)
        };
        assert!(local_daemon_backend(&frontend, Some(12), &records, read).is_none());
    }

    #[test]
    fn desktop_launch_uses_native_daemon_and_clears_transport_overrides() {
        let command = desktop_launch_command(
            &DesktopApp::Chatgpt,
            Path::new("/short-home"),
            Path::new("/shared"),
            Path::new("/userdata"),
            Path::new("/wrapper"),
        );
        let args: Vec<_> = command
            .get_args()
            .map(|v| v.to_string_lossy().into_owned())
            .collect();
        for expected in [
            "CODEX_HOME=/short-home",
            "CODEX_SQLITE_HOME=/shared",
            "CODEX_APP_SERVER_USE_LOCAL_DAEMON=0",
            "CODEX_APP_SERVER_FORCE_CLI=0",
            "CODEX_CLI_PATH=/wrapper",
            "CODEX_APP_SERVER_WS_URL=",
        ] {
            assert!(
                args.iter().any(|v| v == expected),
                "missing launch contract: {expected}"
            );
        }
    }

    #[test]
    fn native_capabilities_require_both_transport_and_continuation_contracts() {
        let file =
            std::env::temp_dir().join(format!("desktop-capabilities-{}", uuid::Uuid::new_v4()));
        std::fs::write(
            &file,
            b"CODEX_APP_SERVER_USE_LOCAL_DAEMON app-server-control.sock",
        )
        .unwrap();
        assert_eq!(native_capabilities_at(&file), (true, false));
        std::fs::write(&file, b"CODEX_APP_SERVER_USE_LOCAL_DAEMON app-server-control.sock update-resume-state-v1 takeResumeState app_update_resume").unwrap();
        assert_eq!(native_capabilities_at(&file), (true, true));
        std::fs::remove_file(file).unwrap();
    }

    #[test]
    #[ignore = "Requires AISW_DESKTOP_TEST_CODEX; starts only an isolated native daemon without credentials or inference"]
    fn isolated_native_daemon_accepts_short_alias_and_verified_control() -> Result<()> {
        let binary = PathBuf::from(
            std::env::var_os("AISW_DESKTOP_TEST_CODEX").context("Set AISW_DESKTOP_TEST_CODEX")?,
        );
        let profile = std::env::temp_dir().join(format!(
            "profiles-native-daemon-long-home-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&profile)?;
        std::fs::write(profile.join("config.toml"), "cli_auth_credentials_store=\"file\"\nchatgpt_base_url=\"http://127.0.0.1:9\"\n[analytics]\nenabled=false\n")?;
        let root = PathBuf::from(format!(
            "/private/tmp/mp-live-{}",
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        ));
        let alias = desktop_home_alias_in(&root, &profile)?;
        struct Stop(PathBuf, PathBuf, PathBuf);
        impl Drop for Stop {
            fn drop(&mut self) {
                let _ = run_daemon_binary(&self.0, &self.1, &self.2, "stop");
            }
        }
        let stop = Stop(binary.clone(), alias.clone(), profile.clone());
        run_daemon_binary(&binary, &alias, &profile, "start")?;
        let home = profile.canonicalize()?;
        let frontend = Launch {
            paths: BTreeMap::from([
                ("CODEX_HOME".into(), home.clone()),
                ("CODEX_SQLITE_HOME".into(), home.clone()),
            ]),
            local_daemon: true,
            ..Default::default()
        };
        let (pid, _) = local_daemon_backend(
            &frontend,
            daemon_pid(&home),
            &process_records(),
            launch_info,
        )
        .context("Native daemon launch identity was not recognized")?;
        assert_eq!(idle_managed_daemon_owner(&home), Some(pid));
        let runtime = crate::models::DesktopRuntime {
            app: DesktopApp::Chatgpt,
            installed: true,
            running: true,
            pid: Some(std::process::id() as i32),
            backend_pid: Some(pid),
            uses_daemon_proxy: false,
            uses_local_daemon: true,
            uses_managed_proxy: false,
            control_socket: None,
            profile_home: Some(home.clone()),
            session_home: Some(home.clone()),
        };
        let mut client = crate::desktop_rpc::Client::connect_desktop(&runtime)?;
        assert!(
            client.request("account/read", serde_json::json!({"refreshToken":false}))?["account"]
                .is_null()
        );
        assert!(client.threads()?.is_empty());
        drop(client);
        drop(stop);
        std::fs::remove_dir_all(root)?;
        std::fs::remove_dir_all(profile)?;
        Ok(())
    }

    #[test]
    fn native_home_alias_preserves_credentials_and_fits_macos_socket_limit() {
        let profile =
            std::env::temp_dir().join(format!("long-managed-profile-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&profile).unwrap();
        std::fs::write(profile.join("auth.json"), b"untouched").unwrap();
        let root = PathBuf::from(format!(
            "/private/tmp/mp-test-{}",
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        ));
        let alias = desktop_home_alias_in(&root, &profile).unwrap();
        assert_eq!(
            alias.canonicalize().unwrap(),
            profile.canonicalize().unwrap()
        );
        assert!(
            alias
                .join("app-server-control/app-server-control.sock")
                .as_os_str()
                .len()
                < 104
        );
        assert_eq!(
            std::fs::read(profile.join("auth.json")).unwrap(),
            b"untouched"
        );
        assert_eq!(desktop_home_alias_in(&root, &profile).unwrap(), alias);
        std::fs::remove_dir_all(&root).unwrap();
        std::fs::remove_dir_all(profile).unwrap();
    }
}
