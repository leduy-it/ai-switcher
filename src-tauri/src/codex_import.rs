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
    let raw = raw.trim_start_matches('\u{feff}').trim();
    anyhow::ensure!(!raw.is_empty(), "Paste JSON or choose a file first");
    let auth: serde_json::Value = serde_json::from_str(raw).map_err(|error| {
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
    Ok((raw.to_string(), auth))
}
