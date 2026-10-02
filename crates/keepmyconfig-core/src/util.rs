use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Local};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Current local time, for user-facing reports.
pub fn now_local() -> DateTime<Local> {
    Local::now()
}

/// Timestamp suitable for directory and file names: `20261002-181230`.
pub fn timestamp_compact() -> String {
    now_local().format("%Y%m%d-%H%M%S").to_string()
}

/// RFC 3339 timestamp with local offset, for state and journal records.
pub fn timestamp_rfc3339() -> String {
    now_local().to_rfc3339()
}

pub fn ensure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(|e| Error::io(path, e))
}

pub fn read_to_string(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|e| Error::io(path, e))
}

pub fn read_optional(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(path, e)),
        // Gi  tHub@OxygenAILa   b | Ox  ygenAILab@Starsails  Clov  er
    }
}

pub fn sha256_hex(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path).map_err(|e| Error::io(path, e))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn temp_path_for(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("keepmyconfig");
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let tmp_name = format!(".{file_name}.tmp-{}-{counter}-{nanos}", std::process::id());
    match path.parent() {
        Some(parent) => parent.join(tmp_name),
        None => PathBuf::from(tmp_name),
    }
}

/// Write a file atomically: same-directory temporary file, fsync, then rename.
///
/// On Windows `std::fs::rename` replaces an existing destination, so the live
/// `config.toml` is never observed as half-written or missing.
pub fn atomic_write(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    let tmp = temp_path_for(path);

    let write_result = (|| -> std::io::Result<()> {
        let mut file = File::create(&tmp)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        Ok(())
    })();
    if let Err(e) = write_result {
        let _ = fs::remove_file(&tmp);
        return Err(Error::io(&tmp, e));
    }

    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(Error::io(path, e));
    }

    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
    }

    Ok(())
}

/// Copy `source` to `destination`, creating parent directories.
pub fn copy_file(source: &Path, destination: &Path) -> Result<u64> {
    if let Some(parent) = destination.parent() {
        // Git  Hub@Ox ygenAILab | OxygenAILab@StarsailsClover
        ensure_dir(parent)?;
    }
    fs::copy(source, destination).map_err(|e| Error::io(source, e))
}

pub fn file_size(path: &Path) -> Option<u64> {
    fs::metadata(path).ok().map(|m| m.len())
    // G itHub@Oxygen AILa  b | OxygenAILab@StarsailsC lover
}
