//! Parse user-supplied credentials without returning token values or contacting the provider.
use crate::models::CodexAuthSourceInput;
use anyhow::{Context, Result};

pub fn load(source: &CodexAuthSourceInput) -> Result<(String, serde_json::Value)> {
    let raw = match (&source.auth_file_path, &source.auth_json) {
        (Some(path), None) => {
            let metadata =
                std::fs::metadata(path).context("Couldn't read the selected JSON file")?;
            anyhow::ensure!(metadata.is_file(), "Choose a Codex auth.json file");
            anyhow::ensure!(
                metadata.len() <= 1024 * 1024,
                "The selected JSON is larger than 1 MB"
            );
            std::fs::read_to_string(path).context("Couldn't read the selected JSON file")?
        }
        (None, Some(text)) => text.clone(),
        _ => anyhow::bail!("Use one source: pasted JSON or a selected file"),
    };
    anyhow::ensure!(raw.len() <= 1024 * 1024, "The JSON is larger than 1 MB");
    let raw = normalize_json_whitespace(raw.trim_start_matches('\u{feff}').trim());
    anyhow::ensure!(!raw.is_empty(), "Paste JSON or choose a file first");
    let auth: serde_json::Value = serde_json::from_str(&raw).map_err(|error| {
        anyhow::anyhow!(
            "Invalid JSON at line {}, column {}. Check brackets, quotes and commas",
            error.line(),
            error.column()
        )
    })?;
    anyhow::ensure!(auth.is_object(), "Expected a Codex auth.json object");
    anyhow::ensure!(
        auth["auth_mode"] != "apikey"
            && !auth["OPENAI_API_KEY"]
                .as_str()
                .is_some_and(|key| !key.trim().is_empty()),
        "These are API credentials; add them using API / Proxy"
    );
    let tokens = auth
        .get("tokens")
        .context("This JSON does not contain Codex OAuth tokens. Use a raw Codex auth.json")?;
    for key in ["access_token", "refresh_token"] {
        anyhow::ensure!(
            tokens[key]
                .as_str()
                .is_some_and(|token| !token.trim().is_empty()),
            "Codex OAuth JSON is missing {key}"
        );
    }
    Ok((raw, auth))
}

/// Rich-text clipboards can replace indentation with NBSP/narrow NBSP. Repair only whitespace
/// outside JSON strings: credential bytes, escaped quotes and backslashes must remain unchanged.
pub(crate) fn normalize_json_whitespace(raw: &str) -> String {
    let mut in_string = false;
    let mut escaped = false;
    raw.chars()
        .map(|ch| {
            if in_string {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    in_string = false;
                }
                ch
            } else if ch == '"' {
                in_string = true;
                ch
            } else if ch.is_whitespace() && !matches!(ch, ' ' | '\t' | '\n' | '\r') {
                ' '
            } else {
                ch
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(raw: &str) -> CodexAuthSourceInput {
        CodexAuthSourceInput {
            auth_file_path: None,
            auth_json: Some(raw.to_string()),
        }
    }

    fn auth_json() -> String {
        serde_json::to_string_pretty(&serde_json::json!({
            "auth_mode": "chatgpt",
            "OPENAI_API_KEY": null,
            "tokens": {
                "access_token": "synthetic-access",
                "refresh_token": "synthetic-refresh",
                "id_token": "synthetic-id",
                "account_id": "synthetic-workspace"
            }
        }))
        .unwrap()
    }

    #[test]
    fn clipboard_whitespace_is_repaired_and_persisted_as_valid_json() {
        for space in ['\u{00a0}', '\u{202f}', '\u{2009}', '\u{3000}'] {
            let original = auth_json();
            let pasted = original.replace(' ', &space.to_string());
            let (stored, parsed) = load(&source(&pasted)).unwrap();
            assert_eq!(stored, original);
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&stored).unwrap(),
                parsed
            );
            assert_eq!(parsed["tokens"]["access_token"], "synthetic-access");
        }
    }

    #[test]
    fn whitespace_and_escape_sequences_inside_strings_are_preserved() {
        let value = serde_json::json!({
            "tokens": {
                "access_token": "synthetic\u{00a0}access\\\"\u{202f}end",
                "refresh_token": "synthetic\\\\\u{00a0}refresh"
            },
            "display_name": "Example\u{00a0}User"
        });
        let original = serde_json::to_string(&value).unwrap();
        let (stored, parsed) = load(&source(&format!("\u{00a0}{original}\u{202f}"))).unwrap();
        assert_eq!(stored, original);
        assert_eq!(parsed, value);
    }

    #[test]
    fn bom_and_standard_whitespace_remain_supported() {
        let original = auth_json();
        assert_eq!(
            load(&source(&format!("\u{feff}\r\n{original}\n")))
                .unwrap()
                .0,
            original
        );
    }

    #[test]
    fn malformed_input_does_not_disclose_secret_contents() {
        let error = load(&source("{\u{00a0}\"private-marker\": secret-marker }")).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("Invalid JSON at line"));
        assert!(!message.contains("private-marker"));
        assert!(!message.contains("secret-marker"));
    }

    #[test]
    fn invalid_fields_and_oversized_sources_remain_rejected() {
        for raw in [
            "[]",
            "{}",
            r#"{"tokens":{"access_token":" ","refresh_token":"synthetic"}}"#,
            r#"{"tokens":{"access_token":"synthetic"}}"#,
            r#"{"auth_mode":"apikey","OPENAI_API_KEY":"synthetic-api-key"}"#,
        ] {
            assert!(load(&source(raw)).is_err());
        }
        assert!(load(&source(&" ".repeat(1024 * 1024 + 1))).is_err());
    }

    #[test]
    fn file_import_uses_same_repair_without_changing_the_source() {
        let dir = std::env::temp_dir().join(format!("codex-import-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("auth.json");
        let raw = auth_json().replace(' ', "\u{00a0}");
        std::fs::write(&path, &raw).unwrap();
        let input = CodexAuthSourceInput {
            auth_file_path: Some(path.clone()),
            auth_json: None,
        };
        let (stored, _) = load(&input).unwrap();
        assert_eq!(stored, auth_json());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), raw);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
