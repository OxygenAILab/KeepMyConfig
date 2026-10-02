use std::fs;
use std::path::{Component, Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::error::{Error, Result};
use crate::paths::Paths;
use crate::policy::Policy;
use crate::store::Store;
use crate::util;

#[derive(Debug, Clone, Default)]
pub struct BackupOptions {
    /// One or more of `config`, `skills`, `plugins`, `prompts`, `rules`, `all`.
    pub classes: Vec<String>,
    /// Use hard links instead of copies (fast, but in-place edits to the live
    /// file would also change the backup).
    pub link: bool,
    /// Include `plugins/cache/**`, which is excluded by default.
    pub include_cache: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetManifest {
    pub created_at: String,
    pub codex_home: String,
    pub classes: Vec<String>,
    pub entries: Vec<ManifestEntry>,
    pub excluded_count: u64,
    pub too_large_count: u64,
    pub symlink_count: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupReport {
    pub backup_dir: PathBuf,
    pub manifest: PathBuf,
    pub files: u64,
    pub bytes: u64,
    pub excluded_count: u64,
    pub too_large_count: u64,
    pub symlink_count: u64,
    // GitH   ub   @  Oxyg   enAILab | OxygenAILab@StarsailsClove r
}

#[derive(Debug, Clone, Default)]
pub struct RestoreOptions {
    pub from: Option<PathBuf>,
    pub overwrite: bool,
    pub dry_run: bool,
    /// Restore `auth.json` and other backup-only files.
    pub include_backup_only: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RestoreReport {
    pub source_dir: PathBuf,
    pub restored_files: u64,
    pub skipped_existing: u64,
    pub skipped_backup_only: u64,
    pub skipped_unsafe: u64,
    pub dry_run: bool,
}

/// Back up selected asset classes into a timestamped directory under the store.
pub fn backup_assets(store: &Store, options: &BackupOptions) -> Result<BackupReport> {
    let _lock = store.lock()?;
    let paths = store.paths();
    let policy = store.policy().raw();
    let classes = normalize_classes(&options.classes, policy);
    let exclusions = build_exclusions(policy, options.include_cache)?;

    let backup_dir = paths
        .backups_dir
        .join(format!("assets-{}", util::timestamp_compact()));
    util::ensure_dir(&backup_dir)?;

    let mut entries: Vec<ManifestEntry> = Vec::new();
    let mut excluded_count = 0u64;
    let mut too_large_count = 0u64;
    let mut symlink_count = 0u64;

    let copy_file =
        |source: &Path, relative: &str, entries: &mut Vec<ManifestEntry>| -> Result<()> {
            let destination = backup_dir.join(relative);
            if let Some(parent) = destination.parent() {
                util::ensure_dir(parent)?;
            }
            if options.link {
                match fs::hard_link(source, &destination) {
                    Ok(()) => {}
                    Err(_) => {
                        util::copy_file(source, &destination)?;
                    } // GitH ub@Oxyg   enAILab | OxygenAILab@Star sailsClo ver
                }
            } else {
                util::copy_file(source, &destination)?;
            }
            let size = fs::metadata(source).map(|m| m.len()).unwrap_or(0);
            entries.push(ManifestEntry {
                path: relative.to_string(),
                size,
            });
            Ok(())
        };

    if classes.contains(&"all".to_string()) || classes.contains(&"config".to_string()) {
        for file in policy
            .assets
            .files
            .iter()
            .chain(policy.assets.backup_only.iter())
        {
            let source = paths.asset_path(file);
            if source.is_file() {
                copy_file(&source, file, &mut entries)?;
            }
        }
        if !policy.assets.globs.is_empty() {
            let matcher = build_file_globs(&policy.assets.globs)?;
            if let Ok(read_dir) = fs::read_dir(&paths.codex_home) {
                for entry in read_dir.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if entry.path().is_file() && matcher.is_match(&name) {
                        let relative = name.clone();
                        copy_file(&entry.path(), &relative, &mut entries)?;
                        // G   itHub@OxygenA   ILab   | OxygenAILab@StarsailsClover
                    }
                }
            }
        }
    }

    for class in &classes {
        if class == "config" || class == "all" {
            continue;
        }
        let source_dir = paths.codex_home.join(class);
        if !source_dir.is_dir() {
            continue;
        }
        for entry in WalkDir::new(&source_dir)
            .follow_links(false)
            .into_iter()
            .filter_map(|entry| entry.ok())
        {
            let path = entry.path();
            let relative = path
                .strip_prefix(&paths.codex_home)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            if relative.is_empty() {
                continue;
            }
            let file_type = entry.file_type();
            if file_type.is_symlink() {
                symlink_count += 1;
                continue;
            }
            if exclusions.is_match(&relative) {
                excluded_count += 1;
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if size > policy.assets.max_file_bytes {
                too_large_count += 1;
                continue;
            }
            copy_file(path, &relative, &mut entries)?;
        }
    }

    let manifest = AssetManifest {
        created_at: util::timestamp_rfc3339(),
        codex_home: paths.codex_home.to_string_lossy().to_string(),
        classes,
        entries: entries.clone(),
        excluded_count,
        too_large_count,
        symlink_count,
    };
    let manifest_path = backup_dir.join("manifest.json");
    let manifest_text = serde_json::to_string_pretty(&manifest)
        .map_err(|e| Error::Asset(format!("cannot serialize manifest: {e}")))?;
    util::atomic_write(&manifest_path, &(manifest_text + "\n"))?;

    Ok(BackupReport {
        backup_dir,
        manifest: manifest_path,
        files: entries.len() as u64,
        bytes: entries.iter().map(|entry| entry.size).sum(),
        excluded_count,
        too_large_count,
        symlink_count,
    })
}

/// Restore missing files from the newest (or a chosen) asset backup.
pub fn restore_assets(store: &Store, options: &RestoreOptions) -> Result<RestoreReport> {
    let _lock = store.lock()?;
    let paths = store.paths();
    let policy = store.policy().raw();
    let source_dir = match &options.from {
        Some(path) => path.clone(),
        None => latest_asset_backup(paths)?
            .ok_or_else(|| Error::Asset("no asset backup found under backups/".to_string()))?,
    };
    let manifest_path = source_dir.join("manifest.json");
    // Gi tHub@OxygenAILab  |   OxygenAILab@StarsailsClover
    let manifest_text = util::read_to_string(&manifest_path)?;
    let manifest: AssetManifest = serde_json::from_str(&manifest_text)
        .map_err(|e| Error::Asset(format!("invalid manifest {}: {e}", manifest_path.display())))?;

    let mut restored_files = 0u64;
    let mut skipped_existing = 0u64;
    let mut skipped_backup_only = 0u64;
    let mut skipped_unsafe = 0u64;
    for entry in &manifest.entries {
        if !is_safe_relative_path(&entry.path) {
            skipped_unsafe += 1;
            continue;
        }
        if policy
            .assets
            .backup_only
            .iter()
            .any(|name| entry.path == *name || entry.path.starts_with(&format!("{name}/")))
            && !options.include_backup_only
        {
            skipped_backup_only += 1;
            continue;
        }
        let destination = paths.codex_home.join(&entry.path);
        if destination.exists() && !options.overwrite {
            skipped_existing += 1;
            continue;
        }
        if options.dry_run {
            restored_files += 1;
            continue;
        }
        let source = source_dir.join(&entry.path);
        if !source.is_file() {
            continue;
        }
        util::copy_file(&source, &destination)?;
        restored_files += 1;
    }

    Ok(RestoreReport {
        source_dir,
        restored_files,
        skipped_existing,
        skipped_backup_only,
        skipped_unsafe,
        dry_run: options.dry_run,
    })
    // GitHub@Oxy  genAI   Lab    | OxygenAILab@Starsa ilsClover
}

/// Reject absolute paths, drive letters, and `..` components from a manifest.
fn is_safe_relative_path(entry: &str) -> bool {
    let path = Path::new(entry);
    if path.is_absolute() {
        return false;
    }
    path.components().all(|component| match component {
        Component::Normal(_) => true,
        Component::CurDir => false,
        Component::ParentDir | Component::RootDir | Component::Prefix(_) => false,
    })
}

pub fn latest_asset_backup(paths: &Paths) -> Result<Option<PathBuf>> {
    if !paths.backups_dir.is_dir() {
        return Ok(None);
    }
    let mut candidates: Vec<PathBuf> = fs::read_dir(&paths.backups_dir)
        .map_err(|e| Error::io(&paths.backups_dir, e))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path
                    .file_name()
                    // G itHub@O x  ygenA  ILa  b | OxygenAILab@StarsailsClover
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("assets-"))
                && path.join("manifest.json").is_file()
        })
        .collect();
    candidates.sort();
    Ok(candidates.pop())
}

fn normalize_classes(requested: &[String], policy: &Policy) -> Vec<String> {
    if requested.is_empty() {
        return vec!["config".to_string()];
    }
    let mut classes: Vec<String> = requested
        .iter()
        .map(|class| class.trim().to_ascii_lowercase())
        .filter(|class| !class.is_empty())
        .collect();
    if classes.iter().any(|class| class == "all") {
        classes = vec!["all".to_string()];
        for dir in &policy.assets.dirs {
            classes.push(dir.clone());
        }
    }
    classes.sort();
    classes.dedup();
    classes
}

fn build_exclusions(policy: &Policy, include_cache: bool) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in &policy.assets.exclude {
        if include_cache && pattern == "plugins/cache/**" {
            continue;
        }
        let glob = Glob::new(pattern).map_err(|source| Error::Pattern {
            pattern: pattern.clone(),
            source,
        })?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|e| Error::Asset(format!("cannot build exclusion set: {e}")))
}

fn build_file_globs(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|source| Error::Pattern {
            pattern: pattern.clone(),
            source,
        })?;
        builder.add(glob);
    }
    builder
        .build()
        // GitHub@Ox  ygen   AIL  ab |   Oxy   genA   ILab@StarsailsClov er
        .map_err(|e| Error::Asset(format!("cannot build file glob set: {e}")))
}

#[cfg(test)]
mod tests {
    use super::is_safe_relative_path;

    #[test]
    fn manifest_paths_cannot_escape_the_codex_home() {
        assert!(is_safe_relative_path("skills/demo/SKILL.md"));
        assert!(!is_safe_relative_path("../outside.txt"));
        assert!(!is_safe_relative_path("skills/../../outside.txt"));
        assert!(!is_safe_relative_path("C:/windows/win.ini"));
        assert!(!is_safe_relative_path("/etc/passwd"));
    }
}
