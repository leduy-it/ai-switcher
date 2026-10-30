//! Explicit replacement of one idle profile credential; bulk imports remain append-only.
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub fn current_auth(profile: &Path) -> Result<Option<Vec<u8>>> {
    let path = profile.join("auth.json");
    let meta = match fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    anyhow::ensure!(
        meta.is_file() && !meta.file_type().is_symlink() && meta.len() <= 1024 * 1024,
        "The existing auth.json must be a regular credential file"
    );
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut bytes = Vec::new();
    options
        .open(path)?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() <= 1024 * 1024,
        "The existing auth.json is too large"
    );
    Ok(Some(bytes))
}
pub fn preview_fingerprint(id: &str, old: &[u8], incoming: &[u8]) -> String {
    let mut digest = Sha256::new();
    for part in [id.as_bytes(), old, incoming] {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part);
    }
    format!("{:x}", digest.finalize())
}
fn stage(profile: &Path, contents: &[u8]) -> Result<PathBuf> {
    let path = profile.join(format!(".auth-import-{}.tmp", uuid::Uuid::new_v4()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> Result<()> {
        let mut file = options.open(&path)?;
        file.write_all(contents)?;
        file.sync_all()?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&path);
        return Err(error);
    }
    Ok(path)
}
/// Keep an owner-only backup, then replace only auth.json. Metadata failure rolls back our own
/// unchanged write; an external writer is never overwritten during rollback.
pub fn replace(
    profile: &Path,
    id: &str,
    incoming: &[u8],
    fingerprint: &str,
    validate: impl FnOnce() -> Result<()>,
    commit: impl FnOnce() -> Result<()>,
) -> Result<Option<PathBuf>> {
    let original = current_auth(profile)?;
    anyhow::ensure!(
        preview_fingerprint(id, original.as_deref().unwrap_or_default(), incoming) == fingerprint,
        "The source or profile credential changed after preview. Preview the JSON again"
    );
    fs::create_dir_all(profile)?;
    let backup = if let Some(bytes) = &original {
        let folder = profile.join(".auth-import-backups");
        #[cfg(unix)]
        {
            use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
            match fs::DirBuilder::new().mode(0o700).create(&folder) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(e) => return Err(e.into()),
            };
            let meta = fs::symlink_metadata(&folder)?;
            anyhow::ensure!(
                meta.is_dir()
                    && !meta.file_type().is_symlink()
                    && meta.uid() == unsafe { libc::geteuid() }
                    && meta.permissions().mode() & 0o077 == 0,
                "The credential backup folder must be private"
            );
        }
        #[cfg(not(unix))]
        fs::create_dir_all(&folder)?;
        let path = folder.join(format!("auth-{}.json", uuid::Uuid::new_v4()));
        let temp = stage(&folder, bytes)?;
        fs::rename(temp, &path)?;
        fs::File::open(&folder)?.sync_all()?;
        Some(path)
    } else {
        None
    };
    let temp = stage(profile, incoming)?;
    let before = (|| -> Result<()> {
        validate()?;
        anyhow::ensure!(
            current_auth(profile)? == original,
            "The profile credential changed during import; preview again"
        );
        Ok(())
    })();
    if let Err(error) = before {
        let _ = fs::remove_file(temp);
        return Err(error);
    }
    if let Err(error) = fs::rename(&temp, profile.join("auth.json")) {
        let _ = fs::remove_file(temp);
        return Err(error.into());
    }
    if let Err(error) = (|| -> Result<()> {
        fs::File::open(profile)?.sync_all()?;
        commit()
    })() {
        if current_auth(profile)?.as_deref() == Some(incoming) {
            if let Some(old) = original {
                let temp = stage(profile, &old)?;
                fs::rename(temp, profile.join("auth.json"))?;
            } else {
                fs::remove_file(profile.join("auth.json"))?;
            }
            fs::File::open(profile)?.sync_all()?;
            return Err(error).context("Credential import rolled back because profile metadata could not be saved; private backup retained");
        }
        anyhow::bail!("Another client changed auth during import. Its credential was preserved; the original private backup remains in this profile's .auth-import-backups folder");
    }
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn active_owner_or_changed_target_cannot_be_overwritten() {
        let root = std::env::temp_dir().join(format!("codex-replace-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("auth.json"), b"old").unwrap();
        let token = preview_fingerprint("profile", b"old", b"new");
        assert!(replace(
            &root,
            "profile",
            b"new",
            &token,
            || anyhow::bail!("profile in use"),
            || panic!("must not commit")
        )
        .is_err());
        assert_eq!(fs::read(root.join("auth.json")).unwrap(), b"old");
        fs::write(root.join("auth.json"), b"external").unwrap();
        assert!(replace(
            &root,
            "profile",
            b"new",
            &token,
            || Ok(()),
            || panic!("must not commit")
        )
        .is_err());
        assert_eq!(fs::read(root.join("auth.json")).unwrap(), b"external");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn failed_metadata_commit_preserves_an_external_writers_auth() {
        let root = std::env::temp_dir().join(format!("codex-replace-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("auth.json"), b"old").unwrap();
        let token = preview_fingerprint("profile", b"old", b"new");
        assert!(replace(
            &root,
            "profile",
            b"new",
            &token,
            || Ok(()),
            || {
                fs::write(root.join("auth.json"), b"external")?;
                anyhow::bail!("state failed")
            }
        )
        .is_err());
        assert_eq!(fs::read(root.join("auth.json")).unwrap(), b"external");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[cfg(unix)]
    fn refuses_auth_symlinks_and_keeps_their_source_untouched() {
        let root = std::env::temp_dir().join(format!("codex-replace-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("source.json"), b"source").unwrap();
        std::os::unix::fs::symlink(root.join("source.json"), root.join("auth.json")).unwrap();
        assert!(current_auth(&root).is_err());
        assert_eq!(fs::read(root.join("source.json")).unwrap(), b"source");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn replacement_preserves_profile_and_retains_private_old_auth() {
        let root = std::env::temp_dir().join(format!("codex-replace-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("auth.json"), b"old-auth").unwrap();
        std::fs::write(root.join("config.toml"), b"existing-config").unwrap();
        let token = preview_fingerprint("profile", b"old-auth", b"new-auth");
        let backup = replace(&root, "profile", b"new-auth", &token, || Ok(()), || Ok(())).unwrap();
        assert_eq!(std::fs::read(root.join("auth.json")).unwrap(), b"new-auth");
        assert_eq!(std::fs::read(backup.unwrap()).unwrap(), b"old-auth");
        assert_eq!(
            std::fs::read(root.join("config.toml")).unwrap(),
            b"existing-config"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn changed_preview_or_failed_metadata_save_keeps_original_auth() {
        let root = std::env::temp_dir().join(format!("codex-replace-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("auth.json"), b"old").unwrap();
        let token = preview_fingerprint("profile", b"old", b"new");
        assert!(replace(
            &root,
            "profile",
            b"different-source",
            &token,
            || Ok(()),
            || Ok(())
        )
        .is_err());
        assert!(replace(
            &root,
            "profile",
            b"new",
            &token,
            || Ok(()),
            || anyhow::bail!("simulated state write failure")
        )
        .is_err());
        assert_eq!(std::fs::read(root.join("auth.json")).unwrap(), b"old");
        std::fs::remove_dir_all(root).unwrap();
    }
}
