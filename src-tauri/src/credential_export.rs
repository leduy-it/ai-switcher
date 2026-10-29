//! Explicit, local credential backups. Never return secret material to a webview or log.
use crate::models::{
    Account, CredentialsExportInput, CredentialsExportResult, ToolId, UsageReport,
};
use anyhow::{Context, Result};
use base64::Engine;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

fn source(raw: String) -> Value {
    let parsed = serde_json::from_str::<Value>(&raw).ok();
    json!({ "raw": raw, "fields": parsed })
}

fn read_source(
    dir: &Path,
    name: &str,
    files: &mut BTreeMap<String, Value>,
    warnings: &mut Vec<String>,
) {
    let path = dir.join(name);
    if !path.exists() {
        return;
    }
    match fs::read_to_string(&path) {
        Ok(raw) => {
            files.insert(name.to_string(), source(raw));
        }
        Err(_) => warnings.push(format!("Could not read {name}")),
    }
}

/// Secret-bearing Rust-only payload: deliberately not serializable to the webview.
pub struct NativeCodexDocument {
    pub bytes: Vec<u8>,
    pub account_count: usize,
    pub warning_count: usize,
}

pub fn native_codex_document(
    profiles: Vec<(Account, PathBuf)>,
    account_id: Option<&str>,
    include_hidden: bool,
) -> Result<NativeCodexDocument> {
    let profiles: Vec<_> = profiles
        .into_iter()
        .filter(|(account, _)| {
            account.tool_id == ToolId::Codex
                && (include_hidden || !account.hidden)
                && account_id.is_none_or(|id| id == account.id)
        })
        .collect();
    if account_id.is_some() {
        anyhow::ensure!(
            profiles.len() == 1,
            "The selected Codex account is no longer available"
        );
    }
    let mut auths = Vec::new();
    let mut single_raw = None;
    let mut warning_count = 0;
    for (account, dir) in &profiles {
        if account.api_provider.is_some() {
            warning_count += 1;
            continue;
        }
        let parsed = (|| -> Result<(String, Value)> {
            use std::io::Read;
            let mut raw = String::new();
            fs::File::open(dir.join("auth.json"))?
                .take(1024 * 1024 + 1)
                .read_to_string(&mut raw)?;
            anyhow::ensure!(
                raw.len() <= 1024 * 1024,
                "Codex auth.json is larger than 1 MB"
            );
            let (_, auth) = crate::codex_import::load(&crate::models::CodexAuthSourceInput {
                auth_file_path: None,
                auth_json: Some(raw.clone()),
            })?;
            Ok((raw, auth))
        })();
        match parsed {
            Ok((raw, auth)) => {
                if account_id.is_some() {
                    single_raw = Some(raw.into_bytes());
                }
                auths.push(auth);
            }
            Err(_) => warning_count += 1,
        }
    }
    anyhow::ensure!(
        !auths.is_empty(),
        "No valid Codex OAuth auth.json is available; API/proxy accounts use credential backups"
    );
    let account_count = auths.len();
    let bytes = match single_raw {
        Some(raw) => raw,
        None => serde_json::to_vec_pretty(&auths)?,
    };
    Ok(NativeCodexDocument {
        bytes,
        account_count,
        warning_count,
    })
}

pub fn save(
    input: CredentialsExportInput,
    profiles: Vec<(Account, PathBuf)>,
    usage: Option<UsageReport>,
) -> Result<CredentialsExportResult> {
    if !input.path.is_absolute() {
        anyhow::bail!("Choose an absolute export file path");
    }
    if input.account_id.is_some() {
        anyhow::ensure!(
            input.tool_id == Some(ToolId::Codex),
            "Choose Codex for a single-account export"
        );
    }
    let profiles: Vec<_> = profiles
        .into_iter()
        .filter(|(account, _)| {
            input
                .tool_id
                .as_ref()
                .is_none_or(|tool| tool == &account.tool_id)
                && (input.include_hidden || !account.hidden)
                && input.account_id.as_ref().is_none_or(|id| id == &account.id)
        })
        .collect();
    if input.account_id.is_some() {
        anyhow::ensure!(
            profiles.len() == 1,
            "The selected Codex account is no longer available"
        );
    }
    if input.native_codex || input.account_id.is_some() {
        anyhow::ensure!(
            input.tool_id == Some(ToolId::Codex),
            "Choose Codex for native auth export"
        );
        let NativeCodexDocument {
            bytes,
            account_count,
            warning_count,
        } = native_codex_document(profiles, input.account_id.as_deref(), input.include_hidden)?;
        write_private(&input.path, &bytes)?;
        return Ok(CredentialsExportResult {
            path: input.path,
            account_count,
            warning_count,
        });
    }
    // Shared usage can contain other identities and local project paths. Account quota remains.
    let usage = if input.account_id.is_some() {
        None
    } else {
        usage
    };
    let mut providers: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut warning_count = 0;
    let account_count = profiles.len();
    for (account, dir) in profiles {
        let mut files = BTreeMap::new();
        let mut warnings = Vec::new();
        let mut credentials: Option<Value> = None;
        let mut claims = BTreeMap::<String, Value>::new();
        match account.tool_id {
            ToolId::Codex => {
                read_source(&dir, "auth.json", &mut files, &mut warnings);
                read_source(&dir, "config.toml", &mut files, &mut warnings);
                credentials = files
                    .get("auth.json")
                    .and_then(|file| file.get("fields"))
                    .filter(|value| !value.is_null())
                    .cloned();
                if let Some(auth) = &credentials {
                    for name in ["id_token", "access_token"] {
                        if let Some(payload) = auth
                            .pointer(&format!("/tokens/{name}"))
                            .and_then(Value::as_str)
                            .and_then(|token| token.split('.').nth(1))
                            .and_then(|part| {
                                base64::engine::general_purpose::URL_SAFE_NO_PAD
                                    .decode(part)
                                    .ok()
                            })
                            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                        {
                            claims.insert(name.into(), payload);
                        }
                    }
                }
            }
            ToolId::Claude => {
                read_source(&dir, ".credentials.json", &mut files, &mut warnings);
                if let Some(raw) = crate::quota::claude_credentials_blob(&dir) {
                    let blob = source(raw);
                    credentials = blob.get("fields").filter(|value| !value.is_null()).cloned();
                    files.insert("claude-credentials.json".into(), blob);
                }
                let service = format!(
                    "Claude Code-credentials-{}",
                    crate::quota::claude_keychain_suffix(&dir)
                );
                if let Some(raw) = crate::quota::read_keychain_blob(&service) {
                    files.insert("claude-profile.keychain".into(), source(raw));
                }
                read_source(&dir, "settings.json", &mut files, &mut warnings);
                read_source(&dir, "settings.local.json", &mut files, &mut warnings);
            }
            ToolId::Cursor => {
                let name = if account.is_default {
                    "auth.json"
                } else {
                    ".cursor/auth.json"
                };
                read_source(&dir, name, &mut files, &mut warnings);
                credentials = files
                    .get(name)
                    .and_then(|file| file.get("fields"))
                    .filter(|value| !value.is_null())
                    .cloned();
                if credentials.is_none() && account.is_default {
                    if let Some(token) = crate::quota::read_keychain_blob("cursor-access-token") {
                        credentials = Some(json!({ "accessToken": token }));
                        files.insert("cursor-access-token.keychain".into(), source(token));
                    }
                }
            }
            ToolId::Opencode => {
                read_source(&dir, "opencode/auth.json", &mut files, &mut warnings);
                credentials = files
                    .get("opencode/auth.json")
                    .and_then(|file| file.get("fields"))
                    .filter(|value| !value.is_null())
                    .cloned();
            }
            ToolId::Antigravity => {
                read_source(&dir, "oauthToken.sql", &mut files, &mut warnings);
                read_source(&dir, "profileUrl.sql", &mut files, &mut warnings);
                let token = files
                    .get("oauthToken.sql")
                    .and_then(|file| file.get("raw"))
                    .cloned()
                    .or_else(|| {
                        account
                            .is_default
                            .then(crate::tools::antigravity_current_token)
                            .flatten()
                            .map(Value::String)
                    });
                if let Some(token) = token {
                    credentials = Some(
                        json!({ "oauthTokenSqlLiteral": token, "profileUrlSqlLiteral": files.get("profileUrl.sql").and_then(|file| file.get("raw")) }),
                    );
                }
            }
        }
        // API profiles carry their key outside the OAuth file; retain it and their provider config.
        read_source(&dir, "api_key", &mut files, &mut warnings);
        if account.api_provider.is_some() && account.tool_id == ToolId::Claude {
            let key = files
                .get("settings.json")
                .and_then(|file| file.get("fields"))
                .and_then(|settings| settings.pointer("/env/ANTHROPIC_AUTH_TOKEN"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|key| !key.is_empty());
            if let Some(key) = key {
                credentials = Some(json!({ "apiKey": key }));
            }
        }
        if let Some(key) = files
            .get("api_key")
            .and_then(|file| file.get("raw"))
            .and_then(Value::as_str)
        {
            credentials = Some(json!({ "apiKey": key.trim(), "oauth": credentials }));
        }
        if credentials.is_none() {
            warnings.push(
                "Credential source is missing or unavailable; metadata is still included".into(),
            );
        }
        warning_count += warnings.len();
        let tool_key = account.tool_id.as_str().to_string();
        let matched_sessions = usage
            .as_ref()
            .and_then(|report| {
                report
                    .tools
                    .iter()
                    .find(|tool| tool.tool_id == account.tool_id)
            })
            .map(|tool| {
                tool.sessions
                    .iter()
                    .filter(|session| {
                        account.account_email.as_ref().is_some_and(|email| {
                            session
                                .account_email
                                .as_ref()
                                .is_some_and(|value| value.eq_ignore_ascii_case(email))
                        })
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let raw_codex_auth = if account.tool_id == ToolId::Codex {
            files
                .get("auth.json")
                .and_then(|file| file.get("raw"))
                .cloned()
        } else {
            None
        };
        providers.entry(tool_key).or_default().push(json!({
            "account": account, "email": account.account_email,
            "credentials": credentials, "credentialFiles": files, "decodedTokenClaims": claims,
            "rawCodexAuth": raw_codex_auth, "quotaAtExport": account.quota,
            "attributedSessions": matched_sessions, "warnings": warnings,
        }));
    }
    let mut usage = usage;
    if let (Some(tool_id), Some(report)) = (&input.tool_id, &mut usage) {
        report.tools.retain(|tool| &tool.tool_id == tool_id);
    }
    let document = json!({
        "format": "michael-le-profiles-credentials", "schemaVersion": 1,
        "appVersion": env!("CARGO_PKG_VERSION"), "exportedAt": chrono::Utc::now().to_rfc3339(),
        "scope": input.tool_id, "accountId": input.account_id, "includesHiddenAccounts": input.include_hidden,
        "usageAtExport": usage, "providers": providers,
        "usageAttribution": "Quota is per credential. Session email records the original account when known; provider totals may include shared or unattributed sessions.",
    });
    write_private(&input.path, &serde_json::to_vec_pretty(&document)?)?;
    Ok(CredentialsExportResult {
        path: input.path,
        account_count,
        warning_count,
    })
}

pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("Choose a destination folder")?;
    let temporary = parent.join(format!(".credential-export-{}.tmp", uuid::Uuid::new_v4()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> Result<()> {
        let mut file = options
            .open(&temporary)
            .context("Couldn't create the export file")?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path).context("Couldn't save the export file")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(id: &str, tool: &str, hidden: bool) -> Account {
        serde_json::from_value(json!({
            "id": id, "toolId": tool, "name": format!("Profile {id}"),
            "accountEmail": format!("{id}@example.invalid"), "state": "idle",
            "fingerprint": format!("profile:{id}"), "createdAt": "2026-10-29T00:00:00Z",
            "updatedAt": "2026-10-29T00:00:00Z", "hidden": hidden
        }))
        .unwrap()
    }

    #[test]
    fn clipboard_document_matches_download_scope_without_wrapping_auth() {
        let dir = std::env::temp_dir().join(format!("native-copy-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let auth =
            json!({"tokens":{"access_token":"synthetic","refresh_token":"synthetic-refresh"}});
        fs::write(dir.join("auth.json"), serde_json::to_vec(&auth).unwrap()).unwrap();
        let profiles = vec![
            (account("chosen", "codex", true), dir.clone()),
            (account("other", "claude", false), dir.clone()),
        ];
        let single = native_codex_document(profiles.clone(), Some("chosen"), true).unwrap();
        assert_eq!(single.account_count, 1);
        assert_eq!(
            serde_json::from_slice::<Value>(&single.bytes).unwrap(),
            auth
        );
        let all = native_codex_document(profiles.clone(), None, true).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&all.bytes).unwrap(),
            json!([auth])
        );
        assert!(native_codex_document(profiles, Some("chosen"), false).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn single_account_export_round_trips_without_other_accounts_or_shared_usage() {
        let dir = std::env::temp_dir().join(format!("profile-export-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let raw = r#"{"tokens":{"access_token":"synthetic-access","refresh_token":"synthetic-refresh","account_id":"synthetic-id"}}"#;
        fs::write(dir.join("auth.json"), raw).unwrap();
        let path = dir.join("share.json");
        let input: CredentialsExportInput = serde_json::from_value(json!({
            "path": path, "toolId": "codex", "accountId": "chosen", "includeHidden": true
        }))
        .unwrap();
        let report = UsageReport {
            tools: vec![],
            generated_at: "private-shared-usage".into(),
            price_status: "unavailable".into(),
            price_updated_at: None,
        };
        let result = save(
            input,
            vec![
                (account("other", "codex", false), dir.clone()),
                (account("chosen", "codex", true), dir.clone()),
                (account("claude", "claude", false), dir.clone()),
            ],
            Some(report),
        )
        .unwrap();
        assert_eq!(result.account_count, 1);
        let document: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(document, serde_json::from_str::<Value>(raw).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), raw);
        let (imported, _) = crate::codex_import::load(&crate::models::CodexAuthSourceInput {
            auth_file_path: Some(path.clone()),
            auth_json: None,
        })
        .unwrap();
        assert_eq!(imported, raw);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bulk_native_export_is_array_and_imports_valid_auth_objects() {
        let dir = std::env::temp_dir().join(format!("bulk-export-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let auth = json!({"tokens":{"access_token":"synthetic", "refresh_token":"synthetic-refresh"}, "last_refresh":"2026-10-29", "extra":{"preserve":true}});
        fs::write(dir.join("auth.json"), serde_json::to_vec(&auth).unwrap()).unwrap();
        let path = dir.join("all.json");
        let input = serde_json::from_value(
            json!({"path":path,"toolId":"codex","nativeCodex":true,"includeHidden":true}),
        )
        .unwrap();
        let result = save(
            input,
            vec![
                (account("valid", "codex", false), dir.clone()),
                (account("missing", "codex", true), dir.join("missing")),
            ],
            None,
        )
        .unwrap();
        assert_eq!(result.account_count, 1);
        assert_eq!(result.warning_count, 1);
        let contents: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(contents, json!([auth]));
        let backup = crate::credential_import::load(&path, Some(&ToolId::Codex)).unwrap();
        assert_eq!(backup.accounts.len(), 1);
        assert_eq!(backup.invalid_count, 0);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn legacy_export_remains_schema_v1_without_new_request_fields() {
        let dir = std::env::temp_dir().join(format!("legacy-export-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("all.json");
        let input =
            serde_json::from_value(json!({"path":path,"toolId":null,"includeHidden":false}))
                .unwrap();
        let result = save(
            input,
            vec![
                (account("shown", "codex", false), dir.clone()),
                (account("hidden", "codex", true), dir.clone()),
            ],
            None,
        )
        .unwrap();
        assert_eq!(result.account_count, 1);
        assert!(result.warning_count > 0);
        let contents: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(contents["schemaVersion"], 1);
        assert_eq!(contents["providers"]["codex"].as_array().unwrap().len(), 1);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_single_account_scope_does_not_write_a_file() {
        let path =
            std::env::temp_dir().join(format!("missing-export-{}.json", uuid::Uuid::new_v4()));
        for (tool, id, include_hidden) in [
            (Some("codex"), "missing", true),
            (Some("claude"), "chosen", true),
            (None, "chosen", true),
            (Some("codex"), "chosen", false),
        ] {
            let input = serde_json::from_value(json!({"path": path, "toolId": tool, "accountId": id, "includeHidden": include_hidden})).unwrap();
            assert!(save(
                input,
                vec![(account("chosen", "codex", true), path.clone())],
                None
            )
            .is_err());
            assert!(!path.exists());
        }
    }
}
