//! Add missing local catalog/history records. Live SQLite files are never copied or symlinked.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub profiles: usize,
    pub catalog_threads: usize,
    pub history_threads: usize,
    pub conflicts: usize,
    pub pending_projection: usize,
    pub unavailable_rollouts: usize,
    pub backups: Vec<PathBuf>,
}
impl Report {
    pub fn include(&mut self, other: Self) {
        self.profiles += other.profiles;
        self.catalog_threads += other.catalog_threads;
        self.history_threads += other.history_threads;
        self.conflicts += other.conflicts;
        self.pending_projection += other.pending_projection;
        self.unavailable_rollouts += other.unavailable_rollouts;
        self.backups.extend(other.backups);
    }
}

fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
fn identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
fn sql(db: &Path, statement: &str, readonly: bool) -> Result<Vec<Value>> {
    anyhow::ensure!(
        db.is_file(),
        "Session database is missing; initialize it in its original desktop before repair"
    );
    let mut command = Command::new("/usr/bin/sqlite3");
    command.args(["-batch", "-bail", "-json", "-cmd", ".timeout 10000"]);
    // A WAL database without its WAL/SHM cannot always open read-only. Let SQLite initialize
    // its own auxiliary files in that case, using an existing-file connection. Source SQL only
    // reads content (and writes TEMP tables/snapshots); never use immutable=1 to bypass WAL.
    let has_wal = PathBuf::from(format!("{}-wal", db.display())).exists();
    let mode = if readonly && has_wal { "ro" } else { "rw" };
    let encoded: String = db
        .to_string_lossy()
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    let mut child = command
        .arg(format!("file:{encoded}?mode={mode}"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("SQLite is unavailable")?;
    child
        .stdin
        .take()
        .context("Missing SQLite input")?
        .write_all(statement.as_bytes())?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        // Statements only contain identifiers, UUIDs and paths, never row contents or tokens.
        // Keep the first diagnostic line; omit SQLite's echoed SQL/context.
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        let first = diagnostic
            .lines()
            .next()
            .unwrap_or("SQLite did not report a reason");
        anyhow::bail!("Session catalog operation failed ({first}); source files and private backups were preserved");
    }
    if output.stdout.is_empty() {
        Ok(vec![])
    } else {
        serde_json::from_slice(&output.stdout).context("Unexpected session catalog response")
    }
}
fn attach(path: &Path, name: &str) -> String {
    format!(
        "ATTACH {} AS {};\n",
        literal(&path.to_string_lossy()),
        identifier(name)
    )
}
fn columns(db: &Path, table: &str) -> Result<Vec<Value>> {
    sql(
        db,
        &format!("PRAGMA table_info({});", identifier(table)),
        true,
    )
}
fn copy_columns(source: &Path, target: &Path, table: &str) -> Result<String> {
    let source = columns(source, table)?;
    let target = columns(target, table)?;
    anyhow::ensure!(!source.is_empty() && !target.is_empty(), "Unsupported session schema ({table}); open both profiles with the same desktop version before migrating");
    let names: Vec<_> = source.iter().filter_map(|c| c["name"].as_str()).collect();
    anyhow::ensure!(
        names
            .iter()
            .all(|name| target.iter().any(|c| c["name"] == *name)),
        "Destination session schema would lose fields ({table}); update its desktop first"
    );
    anyhow::ensure!(
        target
            .iter()
            .all(|c| names.contains(&c["name"].as_str().unwrap_or_default())
                || c["notnull"] == 0
                || !c["dflt_value"].is_null()),
        "Destination session schema requires unknown fields ({table})"
    );
    Ok(names
        .into_iter()
        .map(identifier)
        .collect::<Vec<_>>()
        .join(","))
}
fn private_directory(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
fn private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut out = options.open(path)?;
    out.write_all(bytes)?;
    out.sync_all()?;
    Ok(())
}
fn backup(source: &Path, destination: &Path) -> Result<()> {
    anyhow::ensure!(
        !destination.exists(),
        "Private backup path already exists; it was not replaced"
    );
    // VACUUM INTO takes a consistent SQLite snapshot, including committed WAL content.
    sql(
        source,
        &format!(
            "VACUUM main INTO {};",
            literal(&destination.to_string_lossy())
        ),
        false,
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(destination, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::File::open(destination)?.sync_all()?;
    Ok(())
}
fn database(home: &Path, prefix: &str, supported: &str) -> Result<PathBuf> {
    if let Ok(entries) = std::fs::read_dir(home) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(prefix) && name.ends_with(".sqlite") && name != supported {
                let number = name
                    .trim_start_matches(prefix)
                    .trim_end_matches(".sqlite")
                    .parse::<u32>()
                    .unwrap_or(0);
                let expected = supported
                    .trim_start_matches(prefix)
                    .trim_end_matches(".sqlite")
                    .parse::<u32>()
                    .unwrap_or(0);
                anyhow::ensure!(
                    number <= expected,
                    "A newer session database needs migration support; source data was left intact"
                );
            }
        }
    }
    Ok(home.join(supported))
}
fn history_exists(alias: &str, id: &str) -> String {
    [
        "thread_turns",
        "thread_items",
        "thread_realtime_items",
        "thread_history_projection_state",
    ]
    .iter()
    .map(|table| format!("EXISTS(SELECT 1 FROM {alias}.{table} h WHERE h.thread_id={id})"))
    .collect::<Vec<_>>()
    .join(" OR ")
}
fn history_missing() -> String {
    let keys = [
        ("thread_turns", "d.turn_id=s.turn_id"),
        (
            "thread_items",
            "d.turn_id=s.turn_id AND d.item_id=s.item_id",
        ),
        ("thread_realtime_items", "d.item_id=s.item_id"),
    ];
    let mut checks: Vec<_> = keys.iter().map(|(table, key)| format!("EXISTS(SELECT 1 FROM old_history.{table} s WHERE s.thread_id=e.id AND NOT EXISTS(SELECT 1 FROM main.{table} d WHERE d.thread_id=s.thread_id AND {key}))")).collect();
    checks.push("EXISTS(SELECT 1 FROM old_history.thread_items s JOIN main.thread_items d ON d.thread_id=s.thread_id AND d.turn_id=s.turn_id AND d.item_id=s.item_id WHERE s.thread_id=e.id AND s.updated_at_ordinal>d.updated_at_ordinal AND s.item_json<>d.item_json)".into());
    checks.push("EXISTS(SELECT 1 FROM old_history.thread_turns s JOIN main.thread_turns d ON d.thread_id=s.thread_id AND d.turn_id=s.turn_id WHERE s.thread_id=e.id AND coalesce(s.rollout_end_ordinal,s.rollout_ordinal)>coalesce(d.rollout_end_ordinal,d.rollout_ordinal) AND s.status<>d.status)".into());
    checks.join(" OR ")
}

pub fn migrate(profile: &Path, shared: &Path) -> Result<Report> {
    let mut report = Report::default();
    if profile.canonicalize()? == shared.canonicalize()? {
        return Ok(report);
    }
    let source_catalog = database(profile, "state_", "state_5.sqlite")?;
    if !source_catalog.exists() {
        return Ok(report);
    }
    report.profiles = 1;
    let catalog = database(shared, "state_", "state_5.sqlite")?;
    anyhow::ensure!(
        catalog.exists(),
        "Open the main Codex profile once to initialize its session catalog before migration"
    );
    let source_history = database(profile, "thread_history_", "thread_history_1.sqlite")?;
    let history = database(shared, "thread_history_", "thread_history_1.sqlite")?;
    if source_history.exists() {
        anyhow::ensure!(history.exists(), "Open the main profile with the current desktop to initialize paginated history before migration");
    }
    let shared = shared.canonicalize()?;
    let mut paths = BTreeMap::new();
    let known: std::collections::BTreeSet<String> = sql(&catalog, "SELECT id FROM threads;", true)?
        .into_iter()
        .filter_map(|row| row["id"].as_str().map(str::to_string))
        .collect();
    let mut unavailable_ids = Vec::new();
    for row in sql(
        &source_catalog,
        "SELECT id,rollout_path FROM threads;",
        true,
    )? {
        let id = row["id"].as_str().context("Session has no ID")?;
        if uuid::Uuid::parse_str(id).is_err() {
            continue;
        }
        let path = row["rollout_path"]
            .as_str()
            .and_then(|p| Path::new(p).canonicalize().ok());
        if let Some(path) = path.filter(|p| {
            p.starts_with(shared.join("sessions"))
                || p.starts_with(shared.join("archived_sessions"))
        }) {
            paths.insert(id.to_string(), path);
        } else if !known.contains(id) {
            report.unavailable_rollouts += 1;
            unavailable_ids.push(id.to_string());
        }
    }
    if paths.is_empty() {
        return Ok(report);
    }
    let eligible = format!("CREATE TEMP TABLE eligible(id TEXT PRIMARY KEY,path TEXT NOT NULL); INSERT INTO eligible VALUES {};\n", paths.iter().map(|(id, path)| format!("({},{})", literal(id), literal(&path.to_string_lossy()))).collect::<Vec<_>>().join(","));
    let catalog_count = sql(&catalog, &format!("{}{}SELECT count(*) AS n FROM eligible e WHERE NOT EXISTS(SELECT 1 FROM threads t WHERE t.id=e.id);", attach(&source_catalog, "old_catalog"), eligible), true)?;
    let missing_catalog = catalog_count[0]["n"].as_u64().unwrap_or(0);
    let mut missing_history = 0;
    if source_history.exists() {
        let count = sql(
            &history,
            &format!(
                "{}{}SELECT count(*) AS n FROM eligible e WHERE ({}) AND NOT ({});",
                attach(&source_history, "old_history"),
                eligible,
                history_exists("old_history", "e.id"),
                history_exists("main", "e.id")
            ),
            true,
        )?;
        missing_history = count[0]["n"].as_u64().unwrap_or(0);
    }
    if missing_catalog == 0 && missing_history == 0 {
        if source_history.exists() {
            report.conflicts = partial_conflicts(&source_history, &history, &eligible)?;
            report.pending_projection = pending_projection(&source_history, &history, &eligible)?;
        }
        return Ok(report);
    }
    let directory = shared.join("backups").join(format!(
        "session-catalog-migration-{}",
        uuid::Uuid::new_v4()
    ));
    private_directory(&directory)?;
    let source_snapshot = directory.join("source-state.sqlite");
    let catalog_before = directory.join("main-state-before.sqlite");
    backup(&source_catalog, &source_snapshot)?;
    backup(&catalog, &catalog_before)?;
    let history_snapshot = directory.join("source-history.sqlite");
    if source_history.exists() {
        backup(&source_history, &history_snapshot)?;
        backup(&history, &directory.join("main-history-before.sqlite"))?;
    }
    private_file(
        &directory.join("prepared.json"),
        &serde_json::to_vec_pretty(
            &json!({"version":1,"source":profile,"shared":shared,"status":"prepared","createdAt":chrono::Utc::now().to_rfc3339(),"eligibleThreadIds":paths.keys().collect::<Vec<_>>(),"unavailableThreadIds":unavailable_ids,"plannedCatalogThreads":missing_catalog,"plannedHistoryThreads":missing_history}),
        )?,
    )?;
    std::fs::File::open(&directory)?.sync_all()?;
    report.backups.push(directory.clone());

    let tables = [
        "threads",
        "thread_sections",
        "projects",
        "project_roots",
        "thread_dynamic_tools",
        "thread_attachments",
        "thread_spawn_edges",
    ];
    let mut fields = BTreeMap::new();
    for table in tables {
        fields.insert(table, copy_columns(&source_snapshot, &catalog, table)?);
    }
    let mut statement = format!("{}{}PRAGMA foreign_keys=ON; BEGIN IMMEDIATE; CREATE TEMP TABLE new_threads AS SELECT e.* FROM eligible e JOIN old_catalog.threads s ON s.id=e.id WHERE NOT EXISTS(SELECT 1 FROM threads d WHERE d.id=e.id); CREATE TEMP TABLE new_projects AS SELECT p.id FROM old_catalog.projects p WHERE p.id IN (SELECT t.project_id FROM old_catalog.threads t JOIN new_threads e ON e.id=t.id) AND NOT EXISTS(SELECT 1 FROM main.projects d WHERE d.id=p.id);\n", attach(&source_snapshot, "old_catalog"), eligible);
    // Copy referenced organization rows only if absent. Existing organization and creator fields
    // remain authoritative. Never import enrollments, tokens, migrations or daemon state.
    for (table, predicate) in [
        ("thread_sections", "s.id IN (SELECT t.thread_section_id FROM old_catalog.threads t JOIN new_threads e ON e.id=t.id) AND NOT EXISTS(SELECT 1 FROM main.thread_sections d WHERE d.id=s.id)"),
        ("projects", "s.id IN (SELECT id FROM new_projects)"),
        ("project_roots", "s.project_id IN (SELECT id FROM new_projects)"),
    ] { statement.push_str(&insert(table, &fields[table], "old_catalog", predicate)); }
    statement.push_str(&insert(
        "threads",
        &fields["threads"],
        "old_catalog",
        "s.id IN (SELECT id FROM new_threads)",
    ));
    statement.push_str("UPDATE threads SET rollout_path=(SELECT path FROM new_threads e WHERE e.id=threads.id) WHERE id IN (SELECT id FROM new_threads);\n");
    for table in ["thread_dynamic_tools", "thread_attachments"] {
        statement.push_str(&insert(
            table,
            &fields[table],
            "old_catalog",
            "s.thread_id IN (SELECT id FROM new_threads)",
        ));
    }
    statement.push_str(&insert("thread_spawn_edges", &fields["thread_spawn_edges"], "old_catalog", "s.child_thread_id IN (SELECT id FROM new_threads) AND s.parent_thread_id IN (SELECT id FROM main.threads) AND NOT EXISTS(SELECT 1 FROM main.thread_spawn_edges d WHERE d.child_thread_id=s.child_thread_id)"));
    statement
        .push_str("SELECT count(*) AS n,json_group_array(id) AS ids FROM new_threads; COMMIT;");
    let catalog_result = sql(&catalog, &statement, false)?;
    let catalog_ids: Value =
        serde_json::from_str(catalog_result[0]["ids"].as_str().unwrap_or("[]"))?;
    report.catalog_threads = catalog_result[0]["n"].as_u64().unwrap_or(0) as usize;

    let mut history_ids = json!([]);
    if source_history.exists() {
        let mut fields = BTreeMap::new();
        for table in [
            "thread_turns",
            "thread_items",
            "thread_realtime_items",
            "thread_history_projection_state",
        ] {
            fields.insert(table, copy_columns(&history_snapshot, &history, table)?);
        }
        let mut statement = format!("{}{}{}BEGIN IMMEDIATE; CREATE TEMP TABLE new_history AS SELECT e.id FROM eligible e WHERE EXISTS(SELECT 1 FROM catalog.threads t WHERE t.id=e.id) AND ({}) AND NOT ({});\n", attach(&history_snapshot, "old_history"), attach(&catalog, "catalog"), eligible, history_exists("old_history", "e.id"), history_exists("main", "e.id"));
        for table in [
            "thread_turns",
            "thread_items",
            "thread_realtime_items",
            "thread_history_projection_state",
        ] {
            statement.push_str(&insert(
                table,
                &fields[table],
                "old_history",
                "s.thread_id IN (SELECT id FROM new_history)",
            ));
        }
        statement
            .push_str("SELECT count(*) AS n,json_group_array(id) AS ids FROM new_history; COMMIT;");
        let result = sql(&history, &statement, false)?;
        history_ids = serde_json::from_str(result[0]["ids"].as_str().unwrap_or("[]"))?;
        report.history_threads = result[0]["n"].as_u64().unwrap_or(0) as usize;
        report.conflicts = partial_conflicts(&history_snapshot, &history, &eligible)?;
        report.pending_projection = pending_projection(&history_snapshot, &history, &eligible)?;
    }
    // The two SQLite transactions are independently durable in WAL mode. A crash between them
    // is repaired additively on retry; no existing rows or cursors are replaced.
    private_file(
        &directory.join("completed.json"),
        &serde_json::to_vec_pretty(
            &json!({"version":1,"status":"completed","report":report,"insertedCatalogIds":catalog_ids,"insertedHistoryIds":history_ids,"skippedCatalogIds":paths.keys().filter(|id| !catalog_ids.as_array().is_some_and(|ids| ids.iter().any(|value| value.as_str()==Some(id.as_str())))).collect::<Vec<_>>() }),
        )?,
    )?;
    std::fs::File::open(&directory)?.sync_all()?;
    Ok(report)
}
fn insert(table: &str, fields: &str, source: &str, predicate: &str) -> String {
    let selected = fields
        .split(',')
        .map(|name| format!("s.{name}"))
        .collect::<Vec<_>>()
        .join(",");
    format!("INSERT INTO main.{table} ({fields}) SELECT {selected} FROM {source}.{table} s WHERE {predicate};\n")
}
fn partial_conflicts(source: &Path, target: &Path, eligible: &str) -> Result<usize> {
    let result = sql(
        target,
        &format!(
            "{}{}SELECT count(*) AS n FROM eligible e WHERE ({}) AND ({});",
            attach(source, "old_history"),
            eligible,
            history_exists("main", "e.id"),
            history_missing()
        ),
        true,
    )?;
    Ok(result[0]["n"].as_u64().unwrap_or(0) as usize)
}

fn pending_projection(source: &Path, target: &Path, eligible: &str) -> Result<usize> {
    // A trailing settings/usage event can advance a source cursor without changing any message.
    // Preserve the destination cursor and let its backend replay that event when it opens history.
    let result = sql(target, &format!("{}{}SELECT count(*) AS n FROM eligible e JOIN old_history.thread_history_projection_state s ON s.thread_id=e.id JOIN main.thread_history_projection_state d ON d.thread_id=e.id WHERE (s.next_rollout_byte_offset>d.next_rollout_byte_offset OR s.next_rollout_ordinal>d.next_rollout_ordinal) AND NOT ({});", attach(source, "old_history"), eligible, history_missing()), true)?;
    Ok(result[0]["n"].as_u64().unwrap_or(0) as usize)
}
