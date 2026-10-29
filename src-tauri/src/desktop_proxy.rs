//! Desktop-only transport adapter. The native UI retains its tool runtime and CLI overrides.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
static STOP: AtomicBool = AtomicBool::new(false);
static CHILD: AtomicI32 = AtomicI32::new(0);
#[cfg(unix)]
extern "C" fn stop_signal(_: libc::c_int) {
    STOP.store(true, Ordering::Relaxed);
    let pid = CHILD.load(Ordering::Relaxed);
    if pid > 0 {
        unsafe {
            libc::kill(pid, libc::SIGTERM);
        }
    }
}
#[cfg(unix)]
use std::os::unix::{
    fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    net::UnixStream,
    process::CommandExt,
};

pub const RECORD: &str = ".michael-desktop-runtime-v1.json";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub version: u32,
    pub nonce: String,
    pub proxy_pid: i32,
    pub frontend_pid: i32,
    pub backend_pid: i32,
    pub manager_executable: PathBuf,
    pub native_binary: PathBuf,
    pub profile_home: PathBuf,
    pub shared_home: PathBuf,
    pub control_socket: PathBuf,
}

#[cfg(unix)]
pub fn private_root() -> Result<PathBuf> {
    let root = PathBuf::from(format!("/private/tmp/michael-profiles-{}", unsafe {
        libc::geteuid()
    }));
    match fs::DirBuilder::new().mode(0o700).create(&root) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    let meta = fs::symlink_metadata(&root)?;
    anyhow::ensure!(
        meta.is_dir()
            && !meta.file_type().is_symlink()
            && meta.uid() == unsafe { libc::geteuid() }
            && meta.permissions().mode() & 0o077 == 0,
        "Unsafe desktop runtime folder"
    );
    Ok(root)
}

#[cfg(unix)]
pub fn read_record(home: &Path) -> Result<Record> {
    let path = home.join(RECORD);
    let meta = fs::symlink_metadata(&path)?;
    anyhow::ensure!(
        meta.is_file()
            && !meta.file_type().is_symlink()
            && meta.len() < 16384
            && meta.uid() == unsafe { libc::geteuid() }
            && meta.permissions().mode() & 0o077 == 0,
        "Unsafe desktop runtime record"
    );
    let record: Record = serde_json::from_slice(&fs::read(path)?)?;
    anyhow::ensure!(
        record.version == 1
            && record.profile_home == home.canonicalize()?
            && record.control_socket.parent() == Some(private_root()?.as_path())
            && record
                .control_socket
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("rpc-")),
        "Desktop runtime record does not match this profile"
    );
    Ok(record)
}

#[cfg(not(unix))]
pub fn read_record(_home: &Path) -> Result<Record> {
    anyhow::bail!("Native desktop transport requires macOS")
}

#[cfg(unix)]
pub fn wrapper(native: &Path, home: &Path, shared: &Path) -> Result<PathBuf> {
    use sha2::{Digest, Sha256};
    let exe = std::env::current_exe()?.canonicalize()?;
    let root = private_root()?;
    let identity = format!(
        "{}\n{}\n{}\n{}",
        exe.display(),
        native.display(),
        home.display(),
        shared.display()
    );
    let path = root.join(format!(
        "cli-{}",
        &format!("{:x}", Sha256::digest(identity.as_bytes()))[..20]
    ));
    let temporary = root.join(format!("wrapper-{}", uuid::Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o700)
        .open(&temporary)?;
    let quoted = exe.to_string_lossy().replace('\'', "'\\''");
    let native = native.to_string_lossy().replace('\'', "'\\''");
    let home = home.to_string_lossy().replace('\'', "'\\''");
    let shared = shared.to_string_lossy().replace('\'', "'\\''");
    write!(file,"#!/bin/sh\nexport CODEX_HOME='{home}'\nexport CODEX_SQLITE_HOME='{shared}'\nexec '{quoted}' --desktop-runtime-proxy '{native}' \"$@\"\n")?;
    file.sync_all()?;
    fs::rename(temporary, &path)?;
    Ok(path)
}

fn is_main_server(args: &[String], origin: Option<&str>) -> bool {
    if args.iter().any(|arg| {
        ["--print-startup-config", "--help", "-h", "--version", "-V"].contains(&arg.as_str())
    }) {
        return false;
    }
    let Some(index) = args.iter().position(|arg| arg == "app-server") else {
        return false;
    };
    !args.get(index + 1).is_some_and(|a| !a.starts_with('-'))
        && origin.is_none_or(|s| s == "Codex Desktop" || s == "codex_desktop")
}
fn backend_args(args: &[String], socket: &str) -> Result<Vec<String>> {
    let mut output = Vec::<String>::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--stdio" {
            index += 1;
            continue;
        }
        if arg == "--listen" {
            anyhow::ensure!(
                args.get(index + 1).is_some_and(|v| v == "stdio://"),
                "Desktop requested an unsupported transport"
            );
            index += 2;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--listen=") {
            anyhow::ensure!(
                value == "stdio://",
                "Desktop requested an unsupported transport"
            );
            index += 1;
            continue;
        }
        output.push(arg.clone());
        index += 1;
    }
    output.extend([
        "-c".into(),
        "cli_auth_credentials_store=\"file\"".into(),
        "--listen".into(),
        format!("unix://{socket}"),
    ]);
    Ok(output)
}
#[cfg(unix)]
struct RuntimeGuard {
    child: Child,
    record: Record,
    _lock: File,
}
#[cfg(unix)]
impl Drop for RuntimeGuard {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::kill(self.child.id() as i32, libc::SIGTERM);
            }
            let until = Instant::now() + Duration::from_secs(5);
            while Instant::now() < until && self.child.try_wait().ok().flatten().is_none() {
                std::thread::sleep(Duration::from_millis(50));
            }
            if self.child.try_wait().ok().flatten().is_none() {
                let _ = self.child.kill();
            }
            let _ = self.child.wait();
        }
        if read_record(&self.record.profile_home)
            .ok()
            .is_some_and(|r| r.nonce == self.record.nonce)
        {
            let _ = fs::remove_file(self.record.profile_home.join(RECORD));
        }
        CHILD.store(0, Ordering::Relaxed);
        let _ = fs::remove_file(&self.record.control_socket);
    }
}

/// This entry point runs before Tauri and never creates a manager/webview instance.
#[cfg(unix)]
pub fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(3).collect();
    let binary =
        PathBuf::from(std::env::args_os().nth(2).context("Missing native CLI")?).canonicalize()?;
    if !is_main_server(
        &args,
        std::env::var("CODEX_INTERNAL_ORIGINATOR_OVERRIDE")
            .ok()
            .as_deref(),
    ) {
        return Err(Command::new(binary).args(args).exec().into());
    }
    let home = PathBuf::from(std::env::var_os("CODEX_HOME").context("Missing desktop profile")?)
        .canonicalize()?;
    let shared =
        PathBuf::from(std::env::var_os("CODEX_SQLITE_HOME").context("Missing desktop catalog")?)
            .canonicalize()?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(home.join(".michael-desktop-runtime.lock"))?;
    anyhow::ensure!(
        unsafe {
            libc::flock(
                std::os::fd::AsRawFd::as_raw_fd(&lock),
                libc::LOCK_EX | libc::LOCK_NB,
            )
        } == 0,
        "This profile already has a desktop backend"
    );
    let socket = private_root()?.join(format!("rpc-{}.sock", uuid::Uuid::new_v4().simple()));
    let mut command = Command::new(&binary);
    command
        .args(backend_args(
            &args,
            socket.to_str().context("Invalid socket path")?,
        )?)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    for key in [
        "OPENAI_API_KEY",
        "CODEX_API_KEY",
        "CODEX_ACCESS_TOKEN",
        "ACCESS_TOKEN",
    ] {
        command.env_remove(key);
    }
    let record = Record {
        version: 1,
        nonce: uuid::Uuid::new_v4().to_string(),
        proxy_pid: std::process::id() as i32,
        frontend_pid: unsafe { libc::getppid() },
        backend_pid: 0,
        manager_executable: std::env::current_exe()?.canonicalize()?,
        native_binary: binary,
        profile_home: home,
        shared_home: shared,
        control_socket: socket,
    };
    unsafe {
        libc::signal(
            libc::SIGTERM,
            stop_signal as *const () as libc::sighandler_t,
        );
        libc::signal(libc::SIGINT, stop_signal as *const () as libc::sighandler_t);
    }
    let child = command
        .spawn()
        .context("Could not start native desktop backend")?;
    CHILD.store(child.id() as i32, Ordering::Relaxed);
    let mut guard = RuntimeGuard {
        record: Record {
            backend_pid: child.id() as i32,
            ..record
        },
        child,
        _lock: lock,
    };
    let until = Instant::now() + Duration::from_secs(30);
    let stream = loop {
        anyhow::ensure!(
            !STOP.load(Ordering::Relaxed),
            "Desktop closed during startup"
        );
        if let Ok(stream) = UnixStream::connect(&guard.record.control_socket) {
            break stream;
        }
        anyhow::ensure!(
            guard.child.try_wait()?.is_none(),
            "Native desktop backend exited at startup"
        );
        anyhow::ensure!(
            Instant::now() < until,
            "Native desktop backend did not start"
        );
        std::thread::sleep(Duration::from_millis(30));
    };
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    let (mut ws, _) = tungstenite::client::client_with_config(
        "ws://localhost/rpc",
        stream,
        Some(tungstenite::protocol::WebSocketConfig {
            max_message_size: Some(32 * 1024 * 1024),
            max_frame_size: Some(32 * 1024 * 1024),
            max_write_buffer_size: 64 * 1024 * 1024,
            ..Default::default()
        }),
    )
    .map_err(|_| anyhow::anyhow!("Native desktop transport handshake failed"))?;
    ws.get_mut().set_read_timeout(None)?;
    ws.get_mut().set_write_timeout(None)?;
    ws.get_mut().set_nonblocking(true)?;
    let temporary = guard
        .record
        .profile_home
        .join(format!(".runtime-{}", guard.record.nonce));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)?;
    file.write_all(&serde_json::to_vec(&guard.record)?)?;
    file.sync_all()?;
    fs::rename(temporary, guard.record.profile_home.join(RECORD))?;
    let (tx, rx) = std::sync::mpsc::sync_channel(8);
    std::thread::spawn(move || {
        let mut input = std::io::stdin().lock();
        loop {
            let mut line = Vec::new();
            match input
                .by_ref()
                .take(32 * 1024 * 1024 + 1)
                .read_until(b'\n', &mut line)
            {
                Ok(0) | Err(_) => break,
                Ok(_) if line.len() > 32 * 1024 * 1024 || !line.ends_with(b"\n") => break,
                Ok(_) => {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            }
        }
    });
    let mut out = std::io::stdout().lock();
    let mut pending = None;
    loop {
        if STOP.load(Ordering::Relaxed) {
            return Ok(());
        }
        if pending.is_none() {
            match rx.try_recv() {
                Ok(bytes) => pending = Some(tungstenite::Message::Text(String::from_utf8(bytes)?)),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return Ok(()),
                Err(std::sync::mpsc::TryRecvError::Empty) => (),
            }
        }
        if let Some(message) = pending.take() {
            match ws.send(message) {
                Ok(()) => (),
                Err(tungstenite::Error::WriteBufferFull(message)) => pending = Some(message),
                Err(tungstenite::Error::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => (), // queued by tungstenite; do not replay
                Err(_) => anyhow::bail!("Native desktop write failed"),
            }
        }
        match ws.flush() {
            Ok(()) => (),
            Err(tungstenite::Error::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => (),
            Err(_) => anyhow::bail!("Native desktop flush failed"),
        }
        match ws.read() {
            Ok(tungstenite::Message::Text(text)) => {
                out.write_all(text.as_bytes())?;
                out.write_all(b"\n")?;
                out.flush()?;
            }
            Ok(tungstenite::Message::Close(_)) => return Ok(()),
            Ok(_) => (),
            Err(tungstenite::Error::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(3))
            }
            Err(_) => anyhow::bail!("Native desktop connection ended"),
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    #[test]
    fn wrapper_pins_profile_catalog_and_is_distinct_per_destination() {
        let first = wrapper(
            Path::new("/native"),
            Path::new("/profile-a"),
            Path::new("/shared-a"),
        )
        .unwrap();
        let contents = fs::read_to_string(&first).unwrap();
        let second = wrapper(
            Path::new("/native"),
            Path::new("/profile-b"),
            Path::new("/shared-b"),
        )
        .unwrap();
        assert_ne!(first, second);
        assert!(contents.contains("export CODEX_HOME='/profile-a'"));
        assert!(contents.contains("export CODEX_SQLITE_HOME='/shared-a'"));
        assert_eq!(contents, fs::read_to_string(&first).unwrap());
        fs::remove_file(first).unwrap();
        fs::remove_file(second).unwrap();
    }

    #[test]
    fn reaps_own_term_ignoring_child_before_native_gui_deadline() {
        use std::io::BufRead;
        let home = std::env::temp_dir().join(format!("proxy-cleanup-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&home).unwrap();
        let home = home.canonicalize().unwrap();
        let mut child = Command::new("/bin/sh")
            .args(["-c", "trap '' TERM; echo ready; while :; do sleep 1; done"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut ready = String::new();
        std::io::BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut ready)
            .unwrap();
        assert_eq!(ready.trim(), "ready");
        let pid = child.id() as i32;
        let record = Record {
            version: 1,
            nonce: uuid::Uuid::new_v4().to_string(),
            proxy_pid: std::process::id() as i32,
            frontend_pid: 0,
            backend_pid: pid,
            manager_executable: PathBuf::new(),
            native_binary: PathBuf::new(),
            profile_home: home.clone(),
            shared_home: home.clone(),
            control_socket: private_root()
                .unwrap()
                .join(format!("rpc-{}.sock", uuid::Uuid::new_v4())),
        };
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(home.join(RECORD))
            .unwrap();
        file.write_all(&serde_json::to_vec(&record).unwrap())
            .unwrap();
        let lock = File::open(home.join(RECORD)).unwrap();
        let now = Instant::now();
        drop(RuntimeGuard {
            child,
            record,
            _lock: lock,
        });
        assert!(now.elapsed() < Duration::from_secs(7));
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        assert!(!home.join(RECORD).exists());
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn preserves_native_overrides_and_replaces_only_stdio() {
        let args = vec![
            "-c",
            "features.code_mode_host=true",
            "app-server",
            "--analytics-default-enabled",
            "--listen",
            "stdio://",
            "-c",
            "native_tools=true",
        ]
        .into_iter()
        .map(Into::into)
        .collect::<Vec<String>>();
        let result = backend_args(&args, "/private/tmp/test.sock").unwrap();
        assert!(result.windows(2).any(|w| w == ["-c", "native_tools=true"]));
        assert!(result
            .windows(2)
            .any(|w| w == ["--listen", "unix:///private/tmp/test.sock"]));
        assert!(!result.iter().any(|v| v == "stdio://"));
        assert!(backend_args(
            &["app-server".into(), "--listen=ws://localhost:1".into()],
            "/x"
        )
        .is_err());
    }
    #[test]
    fn delegates_native_helpers_and_version_probes() {
        assert!(!is_main_server(
            &["app-server".into(), "--print-startup-config".into()],
            Some("Codex Desktop")
        ));
        assert!(!is_main_server(
            &["--version".into()],
            Some("Codex Desktop")
        ));
        assert!(!is_main_server(
            &["app-server".into(), "daemon".into(), "version".into()],
            Some("Codex Desktop")
        ));
        assert!(!is_main_server(
            &["app-server".into()],
            Some("codex_latex_preview")
        ));
        assert!(is_main_server(
            &["app-server".into()],
            Some("Codex Desktop")
        ));
    }
}
