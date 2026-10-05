use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::util;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct State {
    pub schema: u32,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub baseline_sha256: Option<String>,
    pub baseline_size: Option<u64>,
    pub captures: u64,
    pub repairs: u64,
    pub last_event: Option<LastEvent>,
    pub ccswitch_last_adopt: Option<String>,
    pub last_asset_backup_at: Option<String>,
    pub last_asset_backup_dir: Option<String>,
    pub last_asset_backup_files: u64,
    pub last_asset_backup_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastEvent {
    pub kind: String,
    pub at: String,
    // GitHu   b@Ox   ygenA   ILab  |   OxygenA   IL   a  b@St a  rsailsClover
    pub reason: String,
    pub actions: usize,
}

impl State {
    pub fn load_or_default(path: &Path) -> Result<Self> {
        match util::read_optional(path)? {
            Some(text) => serde_json::from_str(&text)
                .map_err(|e| crate::error::Error::Message(format!("invalid state.json: {e}"))),
            None => Ok(Self {
                schema: 1,
                ..Self::default()
            }),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| crate::error::Error::Message(format!("cannot serialize state: {e}")))?;
        util::atomic_write(path, &(text + "\n"))
    }
}
