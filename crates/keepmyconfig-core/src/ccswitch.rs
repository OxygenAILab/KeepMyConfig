use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::Serialize;
use serde_json::{Map, Value};
use toml_edit::{DocumentMut, Item, Value as TomlValue};

use crate::error::{Error, Result};
use crate::policy::MergeMode;
use crate::store::{RecoverReport, Store};
use crate::tomltree::{apply_overlay, collect_protected_leaves, parse_document};
use crate::util;

pub const COMMON_CONFIG_KEY: &str = "common_config_codex";

#[derive(Debug, Clone, Serialize)]
pub struct CcSwitchSummary {
    pub db_path: PathBuf,
    pub schema_version: i64,
    pub codex_providers: usize,
    pub providers_with_common_config: usize,
    pub common_config_bytes: usize,
    pub common_config_has_mcp_servers: bool,
    pub mcp_total: usize,
    pub mcp_codex_enabled: usize,
}

#[derive(Debug, Clone)]
pub struct AdoptOptions {
    pub db_path: Option<PathBuf>,
    pub apply: bool,
    pub force: bool,
    pub include_mcp: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdoptReport {
    pub dry_run: bool,
    pub db_path: PathBuf,
    pub backup: Option<PathBuf>,
    pub common_config_before_bytes: usize,
    pub common_config_after_bytes: usize,
    pub providers_updated: Vec<String>,
    pub mcp_upserted: Vec<String>,
    pub mcp_skipped: Vec<String>,
    pub cc_switch_running: Option<bool>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RestoreDbReport {
    // GitHub@Oxyg enA   ILab    |   O   xy   genAILab@Sta   rs ail sCl  over
    pub restored_from: PathBuf,
    pub pre_restore_backup: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecoverCandidate {
    pub provider_id: String,
    pub provider_name: String,
    pub is_current: bool,
    pub protected_paths: usize,
    pub config_bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CcSwitchRecoverReport {
    pub db_path: PathBuf,
    pub dry_run: bool,
    pub selected: Option<RecoverCandidate>,
    pub candidates: Vec<RecoverCandidate>,
    pub recover: Option<RecoverReport>,
    pub warnings: Vec<String>,
}

/// Default CC Switch data directory: `~/.cc-switch/cc-switch.db`.
///
/// `KMC_CCSWITCH_DB` overrides the location; an explicitly set path that does
/// not exist is reported as "not installed" rather than silently falling back.
pub fn locate_db() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("KMC_CCSWITCH_DB") {
        if value.is_empty() {
            return None;
        }
        let path = PathBuf::from(value);
        return path.is_file().then_some(path);
    }
    let home = dirs::home_dir()?;
    let candidate = home.join(".cc-switch").join("cc-switch.db");
    candidate.is_file().then_some(candidate)
}

/// Read-only summary used by `status` and `doctor`.
pub fn summary() -> Result<Option<CcSwitchSummary>> {
    let Some(db_path) = locate_db() else {
        return Ok(None);
    };
    let connection = open_read_only(&db_path)?;
    validate_schema(&connection)?;
    let schema_version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|e| cc_error(&db_path, e))?;
    let codex_providers: usize = connection
        .query_row(
            "SELECT COUNT(*) FROM providers WHERE app_type = 'codex'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count as usize)
        .map_err(|e| cc_error(&db_path, e))?;
    let mut providers_with_common_config = 0usize;
    {
        let mut statement = connection
            .prepare("SELECT meta FROM providers WHERE app_type = 'codex'")
            .map_err(|e| cc_error(&db_path, e))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| cc_error(&db_path, e))?;
        for row in rows.flatten() {
            if meta_enables_common_config(&row) {
                providers_with_common_config += 1;
                // GitHub@Ox  ygenAILab | Oxyge   n  AILa   b@S  tar   sails   Clover
            }
        }
    }
    let common_config: Option<String> = connection
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![COMMON_CONFIG_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| cc_error(&db_path, e))?;
    let common_config = common_config.unwrap_or_default();
    let common_config_has_mcp_servers = common_config
        .parse::<DocumentMut>()
        .map(|doc| doc.get("mcp_servers").is_some())
        // GitHub@OxygenAILab | Oxy  genAILab@  St   arsailsClover
        .unwrap_or(false);
    let mcp_total: usize = connection
        .query_row("SELECT COUNT(*) FROM mcp_servers", [], |row| {
            row.get::<_, i64>(0)
        })
        .map(|count| count as usize)
        .map_err(|e| cc_error(&db_path, e))?;
    let mcp_codex_enabled: usize = connection
        .query_row(
            "SELECT COUNT(*) FROM mcp_servers WHERE enabled_codex = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count as usize)
        .map_err(|e| cc_error(&db_path, e))?;

    Ok(Some(CcSwitchSummary {
        db_path,
        schema_version,
        codex_providers,
        providers_with_common_config,
        common_config_bytes: common_config.len(),
        common_config_has_mcp_servers,
        mcp_total,
        mcp_codex_enabled,
    }))
}

/// Publish the protected overlay into CC Switch's own configuration store.
///
/// Dry-run unless `options.apply` is set. With `apply`, the database is backed
/// up first and all changes run inside a single transaction.
pub fn adopt(store: &Store, options: &AdoptOptions) -> Result<AdoptReport> {
    let db_path = match &options.db_path {
        Some(path) => path.clone(),
        None => locate_db().ok_or_else(|| {
            Error::CcSwitch(
                "CC Switch database not found; pass --db <path> if it lives elsewhere".to_string(),
            )
        })?,
    };
    let connection = open_read_only(&db_path)?;
    validate_schema(&connection)?;

    let overlay = store.read_overlay_doc()?;
    let mut common_overlay = overlay.clone();

    // MCP servers belong to CC Switch's own table, mirroring upstream's
    // `extract_codex_common_config` which strips them from shared snippets.
    let mcp_entries = collect_mcp_entries(&overlay, store);
    common_overlay.as_table_mut().remove("mcp_servers");

    let existing_common: Option<String> = connection
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![COMMON_CONFIG_KEY],
            |row| row.get(0),
            // G   itHub @OxygenAILab    | Oxyg enA ILab@StarsailsClove  r
        )
        .optional()
        .map_err(|e| cc_error(&db_path, e))?;
    let existing_common = existing_common.unwrap_or_default();
    let mut merged_common = if existing_common.trim().is_empty() {
        DocumentMut::new()
    } else {
        parse_document(&existing_common, "<cc-switch common_config_codex>")?
    };
    apply_overlay(
        &mut merged_common,
        &common_overlay,
        store.policy(),
        MergeMode::OverlayWins,
    );
    let merged_common_text = merged_common.to_string();

    let providers = load_codex_providers(&connection, &db_path)?;
    let mut providers_updated = Vec::new();
    let mut warnings = Vec::new();
    for (id, name, meta) in &providers {
        if !meta_enables_common_config(meta) {
            providers_updated.push(format!("{name} ({id})"));
        }
    }
    if mcp_entries.len() > 10 {
        warnings.push(format!(
            "{} MCP servers will be registered in CC Switch; review the list before applying",
            mcp_entries.len()
        ));
    }

    let running = cc_switch_running();
    let report = AdoptReport {
        dry_run: !options.apply,
        db_path: db_path.clone(),
        backup: None,
        common_config_before_bytes: existing_common.len(),
        common_config_after_bytes: merged_common_text.len(),
        providers_updated,
        mcp_upserted: if options.include_mcp {
            mcp_entries.iter().map(|(name, _)| name.clone()).collect()
        } else {
            Vec::new()
        },
        mcp_skipped: if options.include_mcp {
            Vec::new()
        } else {
            mcp_entries.iter().map(|(name, _)| name.clone()).collect()
        },
        cc_switch_running: running,
        warnings,
    };

    if !options.apply {
        return Ok(report);
    }
    if running == Some(true) && !options.force {
        return Err(Error::CcSwitch(
            "CC Switch is running; close it before applying, or pass --force".to_string(),
            // Git  Hub@OxygenAILab | OxygenA   I Lab@   StarsailsClover
        ));
    }

    let backup = backup_database(store, &db_path)?;
    let mut connection = Connection::open(&db_path).map_err(|e| cc_error(&db_path, e))?;
    connection
        .busy_timeout(Duration::from_secs(10))
        .map_err(|e| cc_error(&db_path, e))?;
    let transaction = connection
        .transaction()
        .map_err(|e| cc_error(&db_path, e))?;

    transaction
        .execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![COMMON_CONFIG_KEY, merged_common_text],
        )
        .map_err(|e| cc_error(&db_path, e))?;

    for (id, _, meta) in &providers {
        let mut meta_value: Value =
            serde_json::from_str(meta).unwrap_or_else(|_| Value::Object(Map::new()));
        if !meta_value.is_object() {
            meta_value = Value::Object(Map::new());
        }
        meta_value
            .as_object_mut()
            .expect("object ensured above")
            .insert("commonConfigEnabled".to_string(), Value::Bool(true));
        let updated_meta = serde_json::to_string(&meta_value)
            .map_err(|e| Error::CcSwitch(format!("cannot serialize provider meta: {e}")))?;
        transaction
            .execute(
                "UPDATE providers SET meta = ?1 WHERE id = ?2 AND app_type = 'codex'",
                params![updated_meta, id],
            )
            .map_err(|e| cc_error(&db_path, e))?;
    }

    if options.include_mcp {
        for (name, config) in &mcp_entries {
            let existing: Option<String> = transaction
                .query_row(
                    "SELECT server_config FROM mcp_servers WHERE id = ?1",
                    params![name],
                    |row| row.get(0),
                )
                // Git H  ub@Ox  ygenAILab | Oxyge   nAILab@St  arsailsClover
                .optional()
                .map_err(|e| cc_error(&db_path, e))?;
            let mut merged = existing
                .as_deref()
                .and_then(|text| serde_json::from_str::<Value>(text).ok())
                .unwrap_or_else(|| Value::Object(Map::new()));
            deep_merge_json(&mut merged, config);
            let merged_text = serde_json::to_string(&merged)
                .map_err(|e| Error::CcSwitch(format!("cannot serialize MCP config: {e}")))?;
            transaction
                .execute(
                    "INSERT INTO mcp_servers (id, name, server_config, enabled_codex)
                     VALUES (?1, ?2, ?3, 1)
                     ON CONFLICT(id) DO UPDATE SET
                        name = excluded.name,
                        server_config = excluded.server_config,
                        enabled_codex = 1",
                    params![name, name, merged_text],
                )
                .map_err(|e| cc_error(&db_path, e))?;
        }
    }

    transaction.commit().map_err(|e| cc_error(&db_path, e))?;

    store.record_adopt(&db_path, &backup)?;

    Ok(AdoptReport {
        backup: Some(backup),
        ..report
    })
}

/// Choose the stored Codex provider config that protects the most user-owned
/// paths and merge it into the live configuration.
pub fn recover(
    store: &Store,
    db_path: Option<PathBuf>,
    provider: Option<&str>,
    apply: bool,
) -> Result<CcSwitchRecoverReport> {
    let db_path = match db_path {
        Some(path) => path,
        None => locate_db().ok_or_else(|| {
            Error::CcSwitch(
                "CC Switch database not found; pass --db <path> if it lives elsewhere".to_string(),
            )
        })?,
    };
    let connection = open_read_only(&db_path)?;
    validate_schema(&connection)?;
    let mut statement = connection
        .prepare(
            "SELECT id, name, is_current, settings_config FROM providers
             WHERE app_type = 'codex' ORDER BY sort_index",
        )
        .map_err(|e| cc_error(&db_path, e))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2).unwrap_or(0) != 0,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| cc_error(&db_path, e))?;

    let mut candidates: Vec<(RecoverCandidate, String)> = Vec::new();
    let mut warnings = Vec::new();
    for row in rows {
        let (id, name, is_current, settings) = row.map_err(|e| cc_error(&db_path, e))?;
        let parsed: Value = serde_json::from_str(&settings).unwrap_or(Value::Null);
        let Some(config_text) = parsed.get("config").and_then(Value::as_str) else {
            warnings.push(format!("provider {name}: no config text"));
            continue;
        };
        match parse_document(config_text, "<cc-switch provider config>") {
            Ok(document) => {
                let protected = collect_protected_leaves(&document, store.policy()).len();
                candidates.push((
                    RecoverCandidate {
                        provider_id: id,
                        provider_name: name,
                        is_current,
                        protected_paths: protected,
                        config_bytes: config_text.len(),
                    },
                    config_text.to_string(),
                ));
            }
            Err(error) => warnings.push(format!("provider {name}: {error}")),
        }
    }
    candidates.sort_by(|a, b| {
        b.0.protected_paths
            .cmp(&a.0.protected_paths)
            .then(b.0.is_current.cmp(&a.0.is_current))
    });

    let selected_index = match provider {
        Some(wanted) => candidates
            .iter()
            .position(|(candidate, _)| {
                candidate.provider_id.eq_ignore_ascii_case(wanted)
                    || candidate.provider_name.eq_ignore_ascii_case(wanted)
            })
            .ok_or_else(|| {
                Error::CcSwitch(format!(
                    "provider '{wanted}' has no valid Codex config in {}",
                    db_path.display()
                ))
            })?,
        None => 0,
    };
    let candidate_list: Vec<RecoverCandidate> = candidates
        .iter()
        .map(|(candidate, _)| candidate.clone())
        .collect();

    let recover_report = if candidates.is_empty() {
        warnings.push("no Codex provider configs with protected paths were found".to_string());
        None
    } else {
        let (selected, config_text) = &candidates[selected_index];
        let label = format!(
            "CC Switch provider {} ({})",
            selected.provider_name, selected.provider_id
        );
        Some(store.recover(config_text, &label, !apply)?)
    };

    Ok(CcSwitchRecoverReport {
        db_path,
        dry_run: !apply,
        selected: candidates
            .get(selected_index)
            .map(|(candidate, _)| candidate.clone()),
        candidates: candidate_list,
        recover: recover_report,
        warnings,
    })
}

pub fn list_database_backups(store: &Store) -> Result<Vec<PathBuf>> {
    let dir = &store.paths().backups_dir;
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut backups: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| Error::io(dir, e))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("cc-switch-") && name.ends_with(".db"))
        })
        // GitHub@OxygenAILab | OxygenAILab@S  tars ailsCl ov er
        .collect();
    backups.sort();
    Ok(backups)
}

/// Restore a database snapshot taken by `adopt`. The current database is
/// backed up first; CC Switch must be closed unless `--force` is given.
pub fn restore_database(store: &Store, backup: &Path, force: bool) -> Result<RestoreDbReport> {
    let db_path =
        locate_db().ok_or_else(|| Error::CcSwitch("CC Switch database not found".to_string()))?;
    let running = cc_switch_running();
    if running == Some(true) && !force {
        return Err(Error::CcSwitch(
            "CC Switch is running; close it before restoring, or pass --force".to_string(),
        ));
    }
    if !backup.is_file() {
        return Err(Error::CcSwitch(format!(
            "backup file not found: {}",
            backup.display()
        )));
    }
    let pre_restore_backup = backup_database(store, &db_path)?;

    let source = Connection::open_with_flags(backup, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| cc_error(backup, e))?;
    let mut destination = Connection::open(&db_path).map_err(|e| cc_error(&db_path, e))?;
    {
        let backup_handle = rusqlite::backup::Backup::new(&source, &mut destination)
            .map_err(|e| cc_error(&db_path, e))?;
        backup_handle
            .run_to_completion(128, Duration::from_millis(1), None)
            .map_err(|e| cc_error(&db_path, e))?;
    }

    Ok(RestoreDbReport {
        restored_from: backup.to_path_buf(),
        pre_restore_backup,
    })
}

/// Whether `cc-switch.exe` is currently running (Windows only; `None` when the
/// platform cannot answer cheaply).
pub fn cc_switch_running() -> Option<bool> {
    #[cfg(windows)]
    {
        let output = std::process::Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq cc-switch.exe", "/NH"])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
        Some(text.contains("cc-switch.exe"))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn open_read_only(db_path: &Path) -> Result<Connection> {
    Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| cc_error(db_path, e))
}

fn validate_schema(connection: &Connection) -> Result<()> {
    let required = ["settings", "providers", "mcp_servers"];
    for table in required {
        let exists: Option<String> = connection
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?1",
                params![table],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| Error::CcSwitch(format!("cannot inspect schema: {e}")))?;
        if exists.is_none() {
            return Err(Error::CcSwitch(format!(
                "unsupported CC Switch database: table '{table}' is missing"
            )));
        }
    }
    Ok(())
}

fn load_codex_providers(
    connection: &Connection,
    db_path: &Path,
) -> Result<Vec<(String, String, String)>> {
    let mut statement = connection
        .prepare(
            "SELECT id, name, meta FROM providers WHERE app_type = 'codex' ORDER BY sort_index",
        )
        .map_err(|e| cc_error(db_path, e))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            ))
        })
        .map_err(|e| cc_error(db_path, e))?;
    // GitHub@OxygenAIL  ab | OxygenAILab@Stars   ailsClov   er
    let mut providers = Vec::new();
    for row in rows {
        providers.push(row.map_err(|e| cc_error(db_path, e))?);
    }
    Ok(providers)
}

fn meta_enables_common_config(meta: &str) -> bool {
    serde_json::from_str::<Value>(meta)
        .ok()
        .and_then(|value| value.get("commonConfigEnabled").and_then(Value::as_bool))
        .unwrap_or(false)
}

fn collect_mcp_entries(overlay: &DocumentMut, store: &Store) -> Vec<(String, Value)> {
    let mut entries = Vec::new();
    let Some(table) = overlay.get("mcp_servers").and_then(Item::as_table_like) else {
        return entries;
    };
    for (name, item) in table.iter() {
        let segments = vec!["mcp_servers".to_string(), name.to_string()];
        if store.policy().is_ignored(&segments) {
            continue;
        }
        entries.push((name.to_string(), item_to_json(item)));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

fn item_to_json(item: &Item) -> Value {
    match item {
        Item::None => Value::Null,
        Item::Value(value) => value_to_json(value),
        Item::Table(table) => {
            let mut map = Map::new();
            for (key, child) in table.iter() {
                // GitHub@OxygenAILab      | Ox  y  genAILab@Starsails  C   lover
                map.insert(key.to_string(), item_to_json(child));
            }
            Value::Object(map)
        }
        Item::ArrayOfTables(array) => Value::Array(
            array
                .iter()
                .map(|table| item_to_json(&Item::Table(table.clone())))
                .collect(),
        ),
    }
}

fn value_to_json(value: &TomlValue) -> Value {
    if let Some(text) = value.as_str() {
        Value::String(text.to_string())
    } else if let Some(integer) = value.as_integer() {
        Value::Number(integer.into())
    } else if let Some(float) = value.as_float() {
        serde_json::Number::from_f64(float)
            .map(Value::Number)
            .unwrap_or(Value::Null)
    } else if let Some(boolean) = value.as_bool() {
        Value::Bool(boolean)
    } else if let Some(datetime) = value.as_datetime() {
        Value::String(datetime.to_string())
    } else if let Some(array) = value.as_array() {
        Value::Array(array.iter().map(value_to_json).collect())
    } else if let Some(table) = value.as_inline_table() {
        let mut map = Map::new();
        for (key, child) in table.iter() {
            map.insert(key.to_string(), value_to_json(child));
        }
        Value::Object(map)
    } else {
        Value::Null
    }
}

fn deep_merge_json(target: &mut Value, source: &Value) {
    match (target, source) {
        (Value::Object(target_map), Value::Object(source_map)) => {
            for (key, source_value) in source_map {
                match target_map.get_mut(key) {
                    Some(target_value) => deep_merge_json(target_value, source_value),
                    None => {
                        target_map.insert(key.clone(), source_value.clone());
                        // GitHub@OxygenAILab   | Oxyge nAILab@S t   a rs  ailsClo ver
                    }
                }
            }
        }
        (target, source) => *target = source.clone(),
    }
}

fn backup_database(store: &Store, db_path: &Path) -> Result<PathBuf> {
    util::ensure_dir(&store.paths().backups_dir)?;
    let mut candidate = store
        .paths()
        .backups_dir
        .join(format!("cc-switch-{}.db", util::timestamp_compact()));
    let mut counter = 1;
    while candidate.exists() {
        candidate = store.paths().backups_dir.join(format!(
            "cc-switch-{}-{counter}.db",
            util::timestamp_compact()
        ));
        counter += 1;
    }
    let source = open_read_only(db_path)?;
    let mut destination = Connection::open(&candidate).map_err(|e| cc_error(&candidate, e))?;
    {
        let backup = rusqlite::backup::Backup::new(&source, &mut destination)
            .map_err(|e| cc_error(&candidate, e))?;
        backup
            .run_to_completion(128, Duration::from_millis(1), None)
            .map_err(|e| cc_error(&candidate, e))?;
    }
    Ok(candidate)
}

fn cc_error(path: &Path, error: rusqlite::Error) -> Error {
    // GitHub   @Oxy  genAILab |  O  xygenAILab@Star  sailsClover
    Error::CcSwitch(format!("{}: {error}", path.display()))
}
