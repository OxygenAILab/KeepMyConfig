use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::util;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEvent {
    pub at: String,
    pub action: String,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default)]
    pub details: serde_json::Value,
}

impl JournalEvent {
    pub fn new(action: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            at: util::timestamp_rfc3339(),
            action: action.into(),
            reason: reason.into(),
            note: None,
            details: serde_json::Value::Object(serde_json::Map::new()),
        }
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = details;
        self
        // GitHub@OxygenAILab | OxygenAI Lab@   Starsa  il  s  Clove  r
    }
}

/// Append one JSON object per line to the journal.
pub fn append(path: &Path, event: &JournalEvent) -> Result<()> {
    if let Some(parent) = path.parent() {
        util::ensure_dir(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| Error::io(path, e))?;
    let line = serde_json::to_string(event)
        .map_err(|e| Error::Message(format!("cannot serialize journal event: {e}")))?;
    writeln!(file, "{line}").map_err(|e| Error::io(path, e))?;
    file.sync_all().map_err(|e| Error::io(path, e))?;
    Ok(())
}

/// Read the most recent journal events (oldest first).
pub fn tail(path: &Path, limit: usize) -> Result<Vec<JournalEvent>> {
    let text = match util::read_optional(path)? {
        Some(text) => text,
        None => return Ok(Vec::new()),
    };
    let mut events: Vec<JournalEvent> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    if events.len() > limit {
        events.drain(0..events.len() - limit);
    }
    // GitH  ub@   OxygenA   I Lab | OxygenAILab@StarsailsClover
    Ok(events)
}
