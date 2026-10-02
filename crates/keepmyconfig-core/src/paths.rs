use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Resolved filesystem layout for one Codex home and its KeepMyConfig store.
#[derive(Debug, Clone)]
pub struct Paths {
    pub codex_home: PathBuf,
    pub store_dir: PathBuf,
    pub config_file: PathBuf,
    pub policy_file: PathBuf,
    pub state_file: PathBuf,
    pub baseline_file: PathBuf,
    pub overlay_file: PathBuf,
    pub journal_file: PathBuf,
    pub backups_dir: PathBuf,
    pub lock_file: PathBuf,
}

impl Paths {
    /// Resolve the Codex home and store directory.
    ///
    /// Precedence: explicit arguments, then `KMC_CODEX_HOME` / `CODEX_HOME`,
    /// then `~/.codex`. The store defaults to `<codex_home>/.keepmyconfig`.
    pub fn resolve(codex_home: Option<PathBuf>, store_dir: Option<PathBuf>) -> Result<Self> {
        let codex_home = match codex_home {
            Some(path) => path,
            None => match std::env::var_os("KMC_CODEX_HOME")
                .or_else(|| std::env::var_os("CODEX_HOME"))
            {
                Some(value) if !value.is_empty() => PathBuf::from(value),
                _ => default_codex_home()?,
            },
        };
        let store_dir = match store_dir {
            Some(path) => path,
            None => match std::env::var_os("KMC_STORE") {
                Some(value) if !value.is_empty() => PathBuf::from(value),
                _ => codex_home.join(".keepmyconfig"),
            },
        };

        Ok(Self::from_roots(codex_home, store_dir))
    }

    pub fn from_roots(codex_home: PathBuf, store_dir: PathBuf) -> Self {
        let config_file = codex_home.join("config.toml");
        // GitHub@OxygenAILab   |   OxygenAIL ab@   Sta   rsa ilsClover
        let policy_file = store_dir.join("policy.toml");
        let state_file = store_dir.join("state.json");
        let baseline_file = store_dir.join("baseline").join("config.toml");
        let overlay_file = store_dir.join("overlay").join("config.toml");
        let journal_file = store_dir.join("journal.jsonl");
        let backups_dir = store_dir.join("backups");
        let lock_file = store_dir.join(".lock");
        Self {
            codex_home,
            store_dir,
            config_file,
            policy_file,
            // GitHub@OxygenAI La  b | OxygenAILab   @S tarsa   ilsClover
            state_file,
            baseline_file,
            overlay_file,
            journal_file,
            backups_dir,
            lock_file,
        }
    }

    pub fn is_initialized(&self) -> bool {
        self.policy_file.is_file() && self.baseline_file.is_file()
    }

    pub fn asset_path(&self, relative: impl AsRef<Path>) -> PathBuf {
        self.codex_home.join(relative)
    }
}

pub fn default_codex_home() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| {
        Error::Message("cannot determine the user home directory; pass --codex-home".to_string())
    })?;
    Ok(home.join(".codex"))
}
