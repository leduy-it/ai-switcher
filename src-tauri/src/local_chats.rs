//! Find existing chats without migrating storage or controlling a running backend.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf, process::Command};

const LIMIT: usize = 100;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalChat {
    pub thread_id: String,
    pub name: String,
    pub cwd: String,
    pub updated_at: i64,
    pub archived: bool,
    pub available_in_shared_store: bool,
    pub profiles: Vec<String>,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub chats: Vec<LocalChat>,
    pub warnings: Vec<String>,
    pub has_more: bool,
}

fn read_rows(database: &std::path::Path, statement: &str) -> Result<Vec<Value>> {
    let output = Command::new("/usr/bin/sqlite3")
        .args([
            "-batch",
            "-bail",
            "-readonly",
            "-json",
            "-cmd",
            ".timeout 1000",
        ])
        .arg(database)
        .arg(statement)
        .output()
        .context("SQLite is unavailable")?;
    anyhow::ensure!(
        output.status.success(),
        "Catalog is busy or uses an unsupported schema"
    );
    if output.stdout.is_empty() {
        return Ok(vec![]);
    }
    serde_json::from_slice(&output.stdout).context("Unsupported catalog response")
}

fn catalog_select(database: &std::path::Path) -> Result<(String, Vec<&'static str>)> {
    let schema = read_rows(database, "PRAGMA table_info(threads);")?;
    let has = |name: &str| schema.iter().any(|column| column["name"] == name);
    anyhow::ensure!(
        ["id", "title", "cwd", "updated_at", "archived"]
            .iter()
            .all(|name| has(name)),
        "Unsupported catalog schema"
    );
    let title = if has("name") {
        "coalesce(nullif(name,''),substr(title,1,200))"
    } else {
        "substr(title,1,200)"
    };
    let fields = [
        "title",
        "cwd",
        "id",
        "name",
        "preview",
        "first_user_message",
    ]
    .into_iter()
    .filter(|name| has(name))
    .collect();
    Ok((
        format!("SELECT id,{title} AS title,cwd,updated_at,archived FROM threads"),
        fields,
    ))
}

fn query_rows(database: &std::path::Path, query: &str) -> Result<Vec<Value>> {
    let (select, fields) = catalog_select(database)?;
    // Search each word across stored title/topic metadata. Bodies are never returned to the UI.
    // Quote SQL values and treat LIKE wildcards as literal search characters.
    let clauses: Vec<_> = query
        .split_whitespace()
        .map(|word| {
            let pattern = format!(
                "%{}%",
                word.replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            )
            .replace('\'', "''");
            format!(
                "({})",
                fields
                    .iter()
                    .map(|field| format!("{field} LIKE '{pattern}' ESCAPE '\\'"))
                    .collect::<Vec<_>>()
                    .join(" OR ")
            )
        })
        .collect();
    let predicate = if clauses.is_empty() {
        "1".into()
    } else {
        clauses.join(" AND ")
    };
    let statement = format!(
        "{select} WHERE {predicate} ORDER BY updated_at DESC,id LIMIT {};",
        LIMIT + 1
    );
    read_rows(database, &statement)
}

/// The first source is the shared catalog. Its existing metadata wins over stale profile copies.
pub fn search(
    sources: Vec<(String, PathBuf)>,
    query: &str,
    include_archived: bool,
) -> Result<SearchResult> {
    let query = query.trim();
    anyhow::ensure!(
        query.chars().count() <= 200,
        "Search is limited to 200 characters"
    );
    let mut result = SearchResult::default();
    let mut chats: BTreeMap<String, (bool, LocalChat)> = BTreeMap::new();
    let shared_catalog = sources.first().map(|(_, home)| home.join("state_5.sqlite"));
    for (index, (label, home)) in sources.into_iter().enumerate() {
        let database = home.join("state_5.sqlite");
        if !database.is_file() {
            continue;
        }
        let rows = match query_rows(&database, query) {
            Ok(rows) => rows,
            Err(_) => {
                result.warnings.push(format!(
                    "{label}: catalog is busy or unsupported; other results are still available."
                ));
                continue;
            }
        };
        result.has_more |= rows.len() > LIMIT;
        for row in rows {
            let Some(id) = row["id"]
                .as_str()
                .filter(|id| uuid::Uuid::parse_str(id).is_ok())
            else {
                continue;
            };
            let chat = LocalChat {
                thread_id: id.into(),
                name: row["title"]
                    .as_str()
                    .filter(|title| !title.trim().is_empty())
                    .unwrap_or("Untitled chat")
                    .into(),
                cwd: row["cwd"].as_str().unwrap_or_default().into(),
                updated_at: row["updated_at"].as_i64().unwrap_or_default(),
                archived: row["archived"].as_i64().unwrap_or_default() != 0,
                available_in_shared_store: index == 0,
                profiles: vec![label.clone()],
            };
            if let Some((shared, existing)) = chats.get_mut(id) {
                let mut profiles = existing.profiles.clone();
                if !profiles.contains(&label) {
                    profiles.push(label.clone());
                }
                if !*shared && chat.updated_at > existing.updated_at {
                    *existing = chat;
                }
                existing.profiles = profiles;
            } else {
                chats.insert(id.into(), (index == 0, chat));
            }
        }
    }
    // A title search may find a stale profile copy under its former name. Resolve its current
    // metadata from the shared store before applying the archive filter or displaying it.
    let profile_only: Vec<_> = chats
        .iter()
        .filter(|(_, (shared, _))| !shared)
        .map(|(id, _)| format!("'{id}'"))
        .collect();
    if let Some(database) = shared_catalog.filter(|path| path.is_file() && !profile_only.is_empty())
    {
        let current_rows = (|| -> Result<Vec<Value>> {
            let (select, _) = catalog_select(&database)?;
            read_rows(
                &database,
                &format!("{select} WHERE id IN ({});", profile_only.join(",")),
            )
        })();
        match current_rows {
            Ok(rows) => {
                for row in rows {
                    if let Some((shared, chat)) =
                        row["id"].as_str().and_then(|id| chats.get_mut(id))
                    {
                        *shared = true;
                        chat.name = row["title"]
                            .as_str()
                            .filter(|s| !s.trim().is_empty())
                            .unwrap_or("Untitled chat")
                            .into();
                        chat.cwd = row["cwd"].as_str().unwrap_or_default().into();
                        chat.updated_at = row["updated_at"].as_i64().unwrap_or_default();
                        chat.archived = row["archived"].as_i64().unwrap_or_default() != 0;
                        chat.available_in_shared_store = true;
                    }
                }
            }
            Err(_) => result.warnings.push(
                "Current shared metadata could not be checked; profile results may be stale."
                    .into(),
            ),
        }
    }
    result.chats = chats
        .into_values()
        .map(|(_, chat)| chat)
        .filter(|chat| include_archived || !chat.archived)
        .collect();
    result.chats.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then(a.thread_id.cmp(&b.thread_id))
    });
    result.has_more |= result.chats.len() > LIMIT;
    result.chats.truncate(LIMIT);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        io::{BufRead, BufReader, Write},
        process::Stdio,
    };

    fn fixture(home: &std::path::Path, rows: &str) -> Result<()> {
        fs::create_dir_all(home)?;
        let status = Command::new("/usr/bin/sqlite3").arg(home.join("state_5.sqlite"))
            .arg(format!("PRAGMA journal_mode=WAL; CREATE TABLE threads(id TEXT PRIMARY KEY,title TEXT,cwd TEXT,updated_at INTEGER,archived INTEGER); {rows}"))
            .stdout(Stdio::null()).status()?;
        anyhow::ensure!(status.success(), "Fixture creation failed");
        Ok(())
    }

    #[test]
    fn finds_profile_only_chats_and_keeps_shared_metadata_and_archive_state() -> Result<()> {
        let root = std::env::temp_dir().join(format!("local-chats-{}", uuid::Uuid::new_v4()));
        let shared = root.join("shared");
        let profile = root.join("profile");
        fixture(&shared, "INSERT INTO threads VALUES('00000000-0000-0000-0000-000000000001','Shared name','/project',10,1);")?;
        fixture(&profile, "INSERT INTO threads VALUES('00000000-0000-0000-0000-000000000001','Stale profile name','/old',20,0),('00000000-0000-0000-0000-000000000002','100%_ O''Brien','/project',30,0);")?;
        let sources = || {
            vec![
                ("Shared".into(), shared.clone()),
                ("Hidden profile".into(), profile.clone()),
            ]
        };
        let found = search(sources(), "", false)?;
        assert_eq!(found.chats.len(), 1);
        assert_eq!(found.chats[0].name, "100%_ O'Brien");
        assert_eq!(search(sources(), "%_ O'Brien", true)?.chats.len(), 1);
        assert!(search(sources(), "100x", true)?.chats.is_empty());
        let all = search(sources(), "", true)?;
        assert_eq!(all.chats[1].name, "Shared name");
        assert_eq!(all.chats[1].profiles.len(), 2);
        assert!(all.chats[1].archived);
        assert!(all.chats[1].available_in_shared_store);
        assert!(!all.chats[0].available_in_shared_store);
        assert!(search(sources(), "Stale profile name", false)?
            .chats
            .is_empty());
        assert_eq!(
            search(sources(), "Stale profile name", true)?.chats[0].name,
            "Shared name"
        );
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn reads_committed_chat_while_a_writer_is_active_without_migration() -> Result<()> {
        let root = std::env::temp_dir().join(format!("local-chats-{}", uuid::Uuid::new_v4()));
        fixture(&root, "INSERT INTO threads VALUES('00000000-0000-0000-0000-000000000001','Working chat','/project',10,0);")?;
        let mut writer = Command::new("/usr/bin/sqlite3")
            .arg(root.join("state_5.sqlite"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let input = writer.stdin.as_mut().unwrap();
        input.write_all(
            b"BEGIN IMMEDIATE; UPDATE threads SET title='Uncommitted'; SELECT 'ready';\n",
        )?;
        input.flush()?;
        let mut ready = String::new();
        BufReader::new(writer.stdout.take().unwrap()).read_line(&mut ready)?;
        assert_eq!(ready.trim(), "ready");
        let result = search(
            vec![("Live profile".into(), root.clone())],
            "Working",
            false,
        )?;
        assert!(result.warnings.is_empty());
        assert_eq!(result.chats[0].name, "Working chat");
        assert!(!root.join("backups").exists());
        writer
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"ROLLBACK;\n.quit\n")?;
        drop(writer.stdin.take());
        assert!(writer.wait()?.success());
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn searches_topic_metadata_and_reports_partial_catalog_failures_and_limits() -> Result<()> {
        let root = std::env::temp_dir().join(format!("local-chats-{}", uuid::Uuid::new_v4()));
        let shared = root.join("shared");
        let broken = root.join("unsupported");
        fixture(&shared, "INSERT INTO threads VALUES('00000000-0000-0000-0000-000000000001','Original prompt','/project',1000,0); ALTER TABLE threads ADD COLUMN name TEXT; ALTER TABLE threads ADD COLUMN preview TEXT; UPDATE threads SET name='Draft UI plan',preview='Welcome and platform-level overview'; WITH RECURSIVE n(x) AS (VALUES(2) UNION ALL SELECT x+1 FROM n WHERE x<105) INSERT INTO threads(id,title,cwd,updated_at,archived) SELECT printf('00000000-0000-0000-0000-%012d',x),'Other chat','/project',x,0 FROM n;")?;
        fs::create_dir_all(&broken)?;
        fs::write(broken.join("state_5.sqlite"), "unsupported")?;
        let sources = || {
            vec![
                ("Shared".into(), shared.clone()),
                ("Older profile".into(), broken.clone()),
            ]
        };
        let found = search(sources(), "platform overview", false)?;
        assert_eq!(found.chats.len(), 1);
        assert_eq!(found.chats[0].name, "Draft UI plan");
        let serialized = serde_json::to_string(&found)?;
        assert!(!serialized.contains("Welcome and"));
        assert_eq!(found.warnings.len(), 1);
        let recent = search(sources(), "", false)?;
        assert_eq!(recent.chats.len(), LIMIT);
        assert!(recent.has_more);
        assert_eq!(
            fs::read_to_string(broken.join("state_5.sqlite"))?,
            "unsupported"
        );
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
