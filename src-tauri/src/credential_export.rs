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

pub fn save(
    input: CredentialsExportInput,
    profiles: Vec<(Account, PathBuf)>,
    usage: Option<UsageReport>,
) -> Result<CredentialsExportResult> {
    if !input.path.is_absolute() {
        anyhow::bail!("Choose an absolute export file path");
    }
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
        "scope": input.tool_id, "includesHiddenAccounts": input.include_hidden,
        "usageAtExport": usage, "providers": providers,
        "usageAttribution": "Quota is per credential. Session email records the original account when known; provider totals may include shared or unattributed sessions.",
    });
    let parent = input.path.parent().context("Choose a destination folder")?;
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
        file.write_all(&serde_json::to_vec_pretty(&document)?)?;
        file.sync_all()?;
        fs::rename(&temporary, &input.path).context("Couldn't save the export file")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    Ok(CredentialsExportResult {
        path: input.path,
        account_count,
        warning_count,
    })
}
