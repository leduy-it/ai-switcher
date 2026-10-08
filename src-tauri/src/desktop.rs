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
    let Ok(output) = Command::new("/bin/ps")
        .args(["-ax", "-o", "pid=,comm="])
        .output()
    else {
        return vec![];
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let end = line.find(char::is_whitespace)?;
            Some((line[..end].parse().ok()?, PathBuf::from(line[end..].trim())))
        })
        .collect()
}

/// Read only the two non-secret launch paths. Never expose/log the process environment.
#[cfg(target_os = "macos")]
fn launch_paths(pid: i32) -> Option<BTreeMap<String, PathBuf>> {
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
        for _ in 0..argc {
            cursor += bytes.get(cursor..)?.iter().position(|b| *b == 0)? + 1;
        }
        let mut paths = BTreeMap::new();
        for entry in bytes.get(cursor..)?.split(|b| *b == 0) {
            for key in ["CODEX_HOME", "CODEX_SQLITE_HOME"] {
                if let Some(value) = entry.strip_prefix(format!("{key}=").as_bytes()) {
                    let value = std::str::from_utf8(value).ok()?;
                    if !value.is_empty() {
                        paths.insert(key.to_string(), PathBuf::from(value));
                    }
                }
            }
        }
        Some(paths)
    }
}

#[cfg(not(target_os = "macos"))]
fn launch_paths(_pid: i32) -> Option<BTreeMap<String, PathBuf>> {
    None
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

pub fn runtime() -> Vec<DesktopRuntime> {
    let processes = processes();
    [DesktopApp::Codex, DesktopApp::Chatgpt]
        .into_iter()
        .flat_map(|app| {
            let path = executable(&app);
            let pids: Vec<_> = processes
                .iter()
                .filter(|(_, p)| *p == path)
                .map(|(pid, _)| *pid)
                .collect();
            let pids: Vec<Option<i32>> = if pids.is_empty() {
                vec![None]
            } else {
                pids.into_iter().map(Some).collect()
            };
            pids.into_iter().map(move |pid| {
                let paths = pid.and_then(launch_paths);
                DesktopRuntime {
                    installed: path.exists(),
                    app: app.clone(),
                    running: pid.is_some(),
                    pid,
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
    let raw = std::fs::read_to_string(profile.join("app-server-daemon/daemon.pid")).ok()?;
    let pid = raw.trim().parse().ok()?;
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

pub fn apply(
    settings: &DesktopSyncSettings,
    profile: &Path,
    shared: &Path,
    api: bool,
) -> Result<()> {
    #[cfg(not(target_os = "macos"))]
    anyhow::bail!("Desktop switching currently supports macOS");
    #[cfg(target_os = "macos")]
    {
        anyhow::ensure!(!api, "Desktop handoff currently supports subscription profiles. API profiles remain available through their CLI launcher");
        let selected = bundle(&settings.app);
        anyhow::ensure!(
            executable(&settings.app).exists(),
            "The selected desktop app is not installed"
        );
        validate_catalog_config(profile, shared)?;
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
        for runtime in runtime().into_iter().filter(|r| r.running) {
            let home = runtime.profile_home.context(
                "Desktop profile is unknown; finish its work and quit it before applying",
            )?;
            let mut client = crate::desktop_rpc::Client::connect(&home)?;
            anyhow::ensure!(!client.threads()?.iter().any(|t| t["status"]["type"] == "active"), "New work started before the handoff. Desktop was left running; retry at a safe point");
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
        let session_home = if api { profile } else { shared };
        // A CLI daemon can outlive its desktop and keep an old SQLite home in memory. Restart
        // only the selected profile's backend when its catalog location is wrong.
        if let Ok(raw_pid) = std::fs::read_to_string(profile.join("app-server-daemon/daemon.pid")) {
            if let Ok(pid) = raw_pid.trim().parse::<i32>() {
                if let Some(paths) = launch_paths(pid) {
                    let home = paths.get("CODEX_HOME").cloned().unwrap_or_else(|| {
                        crate::tools::default_config_dir(&crate::models::ToolId::Codex)
                    });
                    let sqlite = paths
                        .get("CODEX_SQLITE_HOME")
                        .cloned()
                        .unwrap_or_else(|| home.clone());
                    if home == profile && sqlite != session_home {
                        let binary = match settings.app {
                            DesktopApp::Codex => selected.join("Contents/Resources/codex"),
                            DesktopApp::Chatgpt => selected.join(
                                "Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex",
                            ),
                        };
                        let mut restart = Command::new(binary);
                        restart
                            .args(["app-server", "daemon", "restart"])
                            .env("CODEX_HOME", profile)
                            .env("CODEX_SQLITE_HOME", session_home)
                            .stdout(Stdio::null())
                            .stderr(Stdio::null());
                        for key in ["OPENAI_API_KEY", "CODEX_API_KEY", "CODEX_ACCESS_TOKEN"] {
                            restart.env_remove(key);
                        }
                        let mut child = restart
                            .spawn()
                            .context("Could not restart the selected profile backend")?;
                        let deadline = Instant::now() + Duration::from_secs(20);
                        loop {
                            if let Some(status) = child.try_wait()? {
                                anyhow::ensure!(status.success(), "The profile backend could not adopt the shared catalog; desktop handoff stopped");
                                break;
                            }
                            if Instant::now() >= deadline {
                                let _ = child.kill();
                                let _ = child.wait();
                                anyhow::bail!("Backend restart timed out; desktop handoff stopped");
                            }
                            std::thread::sleep(Duration::from_millis(200));
                        }
                    }
                }
            }
        }
        let user_data = crate::tools::home_dir().join("Library/Application Support/Codex");
        std::fs::create_dir_all(&user_data)?;
        let mut command = Command::new("/usr/bin/open");
        command
            .args(["-n", "--env"])
            .arg(format!("CODEX_HOME={}", profile.display()))
            .arg("--env")
            .arg(format!("CODEX_SQLITE_HOME={}", session_home.display()))
            // Explicit userData also makes these versions preserve CODEX_HOME during shell env loading.
            .arg("--env")
            .arg(format!(
                "CODEX_ELECTRON_USER_DATA_PATH={}",
                user_data.display()
            ));
        // Never inherit a gateway key from the Switcher's launching shell.
        for key in ["OPENAI_API_KEY", "CODEX_API_KEY", "CODEX_ACCESS_TOKEN"] {
            command.env_remove(key);
        }
        let status = command
            .arg(selected)
            .arg("--args")
            .arg(format!("--user-data-dir={}", user_data.display()))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
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
