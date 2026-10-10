//! Import the versioned credential-backup JSON written by `credential_export`.
//!
//! Credential values stay in this backend module; preview DTOs contain metadata only.
use crate::models::{ApiProvider, CodexAuthSourceInput, ToolId};
use anyhow::{Context, Result};
use serde_json::Value;
use std::{fs, path::Path};

const MAX_IMPORT_BYTES: u64 = 100 * 1024 * 1024;

pub enum ImportedCredential {
    CodexOAuth {
        raw_auth: String,
        account_id: Option<String>,
        user_id: Option<String>,
    },
    ClaudeOAuth {
        raw_credentials: String,
    },
    ApiProxy {
        api_key: String,
        provider: ApiProvider,
    },
}

pub struct ImportedAccount {
    pub tool_id: ToolId,
    pub name: String,
    pub email: Option<String>,
    pub credential: ImportedCredential,
}

pub struct CredentialBackup {
    pub accounts: Vec<ImportedAccount>,
    pub unsupported_count: usize,
    pub unavailable_count: usize,
    pub invalid_count: usize,
}

pub fn load(path: &Path, scope: Option<&ToolId>) -> Result<CredentialBackup> {
    anyhow::ensure!(
        path.is_absolute(),
        "Choose an absolute credential backup path"
    );
    let metadata = fs::metadata(path).context("Couldn't read the selected JSON file")?;
    anyhow::ensure!(metadata.is_file(), "Choose a credential backup JSON file");
    anyhow::ensure!(
        metadata.len() <= MAX_IMPORT_BYTES,
        "The credential backup is larger than 100 MB"
    );
    let bytes = fs::read(path).context("Couldn't read the selected JSON file")?;
    let document: Value = serde_json::from_slice(&bytes).map_err(|error| {
        anyhow::anyhow!(
            "Invalid credential backup JSON at line {}, column {}",
            error.line(),
            error.column()
        )
    })?;
    anyhow::ensure!(
        document.get("format").and_then(Value::as_str) == Some("michael-le-profiles-credentials"),
        "This is not a Michael Le Profiles credential backup"
    );
    anyhow::ensure!(
        document.get("schemaVersion").and_then(Value::as_u64) == Some(1),
        "This credential backup format version is not supported"
    );
    let providers = document
        .get("providers")
        .and_then(Value::as_object)
        .context("Credential backup has no provider accounts")?;

    let mut backup = CredentialBackup {
        accounts: Vec::new(),
        unsupported_count: 0,
        unavailable_count: 0,
        invalid_count: 0,
    };
    for (provider, entries) in providers {
        if scope.is_some_and(|selected| selected.as_str() != provider) {
            continue;
        }
        let tool_id = match provider.as_str() {
            "codex" => ToolId::Codex,
            "claude" => ToolId::Claude,
            _ => {
                backup.unsupported_count += entries.as_array().map_or(1, Vec::len);
                continue;
            }
        };
        let Some(entries) = entries.as_array() else {
            backup.invalid_count += 1;
            continue;
        };
        for entry in entries {
            match parse_account(&tool_id, entry) {
                Ok(Some(account)) => backup.accounts.push(account),
                Ok(None) => backup.unavailable_count += 1,
                Err(()) => backup.invalid_count += 1,
            }
        }
    }
    Ok(backup)
}

fn parse_account(
    tool_id: &ToolId,
    entry: &Value,
) -> std::result::Result<Option<ImportedAccount>, ()> {
    let account = entry.get("account").ok_or(())?;
    if account.get("toolId").and_then(Value::as_str) != Some(tool_id.as_str()) {
        return Err(());
    }
    let name = account
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .chars()
        .filter(|character| !character.is_control())
        .take(20)
        .collect::<String>()
        .trim()
        .to_string();
    let email = account
        .get("accountEmail")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|email| {
            !email.is_empty() && email.len() <= 320 && !email.chars().any(char::is_control)
        })
        .map(ToString::to_string);
    let api_provider = account
        .get("apiProvider")
        .filter(|value| !value.is_null())
        .map(|value| serde_json::from_value::<ApiProvider>(value.clone()).map_err(|_| ()))
        .transpose()?;
    let files = entry.get("credentialFiles").and_then(Value::as_object);
    let credential_data = entry.get("credentials");

    if let Some(provider) = api_provider {
        let api_key = credential_data
            .and_then(|value| value.get("apiKey"))
            .and_then(Value::as_str)
            .or_else(|| file_raw(files, "api_key"))
            .or_else(|| {
                files?
                    .get("settings.json")?
                    .get("fields")?
                    .pointer("/env/ANTHROPIC_AUTH_TOKEN")?
                    .as_str()
            })
            .map(str::trim)
            .filter(|value| !value.is_empty() && value.len() <= 16 * 1024)
            .ok_or(())?;
        let mut provider = provider;
        provider.base_url = provider.base_url.trim().trim_end_matches('/').to_string();
        provider.model = provider.model.trim().to_string();
        if !provider.base_url.starts_with("https://")
            || provider.base_url.len() > 2048
            || provider.model.is_empty()
            || provider.model.len() > 512
        {
            return Err(());
        }
        return Ok(Some(ImportedAccount {
            tool_id: tool_id.clone(),
            name,
            email,
            credential: ImportedCredential::ApiProxy {
                api_key: api_key.to_string(),
                provider,
            },
        }));
    }

    match tool_id {
        ToolId::Codex => {
            let fallback = credential_data
                .filter(|value| value.is_object())
                .and_then(|value| serde_json::to_string(value).ok());
            let raw = file_raw(files, "auth.json")
                .or_else(|| entry.get("rawCodexAuth").and_then(Value::as_str))
                .map(ToString::to_string)
                .or(fallback);
            let raw = match raw {
                Some(raw) if !raw.trim().is_empty() => raw,
                _ => return Ok(None),
            };
            let source = CodexAuthSourceInput {
                auth_file_path: None,
                auth_json: Some(raw),
            };
            let (raw_auth, auth) = crate::codex_import::load(&source).map_err(|_| ())?;
            let account_id = crate::quota::codex_account_id_from_auth(&auth);
            let user_id = crate::quota::codex_user_id_from_auth(&auth);
            Ok(Some(ImportedAccount {
                tool_id: ToolId::Codex,
                name,
                email: crate::quota::codex_account_email_from_auth(&auth).or(email),
                credential: ImportedCredential::CodexOAuth {
                    raw_auth,
                    account_id,
                    user_id,
                },
            }))
        }
        ToolId::Claude => {
            let fallback = credential_data
                .filter(|value| value.is_object())
                .and_then(|value| serde_json::to_string(value).ok());
            let raw = file_raw(files, "claude-credentials.json")
                .or_else(|| file_raw(files, ".credentials.json"))
                .or_else(|| file_raw(files, "claude-profile.keychain"))
                .map(ToString::to_string)
                .or(fallback);
            let Some(raw) = raw.filter(|value| !value.trim().is_empty()) else {
                return Ok(None);
            };
            if raw.len() > 1024 * 1024 {
                return Err(());
            }
            let credentials: Value = serde_json::from_str(&raw).map_err(|_| ())?;
            let oauth = credentials.get("claudeAiOauth").ok_or(())?;
            for field in ["accessToken", "refreshToken"] {
                if !oauth
                    .get(field)
                    .and_then(Value::as_str)
                    .is_some_and(|token| !token.trim().is_empty())
                {
                    return Err(());
                }
            }
            Ok(Some(ImportedAccount {
                tool_id: ToolId::Claude,
                name,
                email,
                credential: ImportedCredential::ClaudeOAuth {
                    raw_credentials: raw,
                },
            }))
        }
        _ => Err(()),
    }
}

fn file_raw<'a>(
    files: Option<&'a serde_json::Map<String, Value>>,
    filename: &str,
) -> Option<&'a str> {
    files?.get(filename)?.get("raw")?.as_str()
}
