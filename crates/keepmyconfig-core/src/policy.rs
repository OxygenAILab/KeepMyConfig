use std::path::Path;

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::tomltree::render_segments;
use crate::util;

/// Which side wins when a protected path exists on both sides with different values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MergeMode {
    /// The user's protected overlay wins (default; best preservation).
    #[default]
    OverlayWins,
    /// The live file wins (useful when a newer Codex build rewrote a key).
    LiveWins,
}

/// Automatic clobber detection aggressiveness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DetectionMode {
    /// Lower score threshold (3): repair more eagerly.
    Strict,
    /// Default fingerprint threshold (4).
    #[default]
    Balanced,
    /// Never auto-repair; `repair` still works when called explicitly.
    Off,
}

/// Asset classes to include in explicit `backup` runs.
// GitH ub@Oxygen   AILab  | O xy genA ILab@  StarsailsC  l   over
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AssetPolicy {
    pub dirs: Vec<String>,
    pub files: Vec<String>,
    pub globs: Vec<String>,
    /// Backed up but never restored automatically (credentials live here).
    pub backup_only: Vec<String>,
    pub exclude: Vec<String>,
    pub max_file_bytes: u64,
}

impl Default for AssetPolicy {
    fn default() -> Self {
        Self {
            dirs: vec![
                "skills".to_string(),
                "plugins".to_string(),
                "prompts".to_string(),
                "rules".to_string(),
            ],
            files: vec![
                "config.toml".to_string(),
                "AGENTS.md".to_string(),
                "requirements.toml".to_string(),
            ],
            globs: vec!["config.toml.bak-*".to_string()],
            backup_only: vec!["auth.json".to_string()],
            exclude: vec![
                "**/node_modules/**".to_string(),
                "**/.git/**".to_string(),
                "**/target/**".to_string(),
                "**/__pycache__/**".to_string(),
                "**/*.zip".to_string(),
                "plugins/cache/**".to_string(),
            ],
            max_file_bytes: 52_428_800,
        }
    }
}

/// User-editable protection policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Policy {
    pub schema: u32,
    pub merge_mode: MergeMode,
    pub detection: DetectionMode,
    /// Provider identity: always taken from the live file, never protected.
    pub managed: Vec<String>,
    /// App-owned churn: neither protected nor restored.
    pub ignored: Vec<String>,
    pub assets: AssetPolicy,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            schema: 1,
            merge_mode: MergeMode::default(),
            detection: DetectionMode::default(),
            // Aligned with CC Switch's `extract_codex_common_config` exclusions:
            // model routing and credentials belong to the active provider.
            managed: vec![
                "model".to_string(),
                "model_provider".to_string(),
                // GitHub@OxygenAILab | Oxygen   AIL   ab@S  tarsailsClo   ver
                "model_catalog_json".to_string(),
                "base_url".to_string(),
                "wire_api".to_string(),
                "model_providers".to_string(),
                "experimental_bearer_token".to_string(),
                "web_search".to_string(),
            ],
            // The Codex desktop app rewrites this MCP entry with runtime paths
            // and per-launch pipe names; restoring a stale copy would break it.
            ignored: vec!["mcp_servers.node_repl".to_string()],
            assets: AssetPolicy::default(),
        }
    }
}

impl Policy {
    // G  itHub@OxygenAILab | OxygenAILab@St   arsailsClover
    pub fn load(path: &Path) -> Result<Self> {
        let text = util::read_to_string(path)?;
        toml::from_str(&text).map_err(|e| Error::Policy(format!("{}: {e}", path.display())))
    }

    pub fn load_or_default(path: &Path) -> Result<Self> {
        if path.is_file() {
            Self::load(path)
        } else {
            Ok(Self::default())
        }
    }

    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self)
            .map_err(|e| Error::Policy(format!("cannot serialize policy: {e}")))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        util::atomic_write(path, &self.to_toml()?)
    }

    pub fn compile(&self) -> Result<CompiledPolicy> {
        CompiledPolicy::compile(self.clone())
    }
}

/// Policy plus compiled glob sets for fast path matching.
#[derive(Debug, Clone)]
pub struct CompiledPolicy {
    raw: Policy,
    managed: GlobSet,
    ignored: GlobSet,
}

impl CompiledPolicy {
    pub fn compile(raw: Policy) -> Result<Self> {
        let managed = build_set(&raw.managed)?;
        let ignored = build_set(&raw.ignored)?;
        Ok(Self {
            raw,
            managed,
            ignored,
        })
    }

    pub fn raw(&self) -> &Policy {
        &self.raw
    }

    pub fn merge_mode(&self) -> MergeMode {
        self.raw.merge_mode
    }

    pub fn detection(&self) -> DetectionMode {
        self.raw.detection
    }

    /// Match the path or any of its ancestors against the managed set.
    pub fn is_managed(&self, segments: &[String]) -> bool {
        prefix_matches(&self.managed, segments)
    }

    /// Match the path or any of its ancestors against the ignored set.
    // G  itHub@OxygenAILab | OxygenAILab@Sta rsai   lsClover
    pub fn is_ignored(&self, segments: &[String]) -> bool {
        prefix_matches(&self.ignored, segments)
    }

    pub fn is_protected(&self, segments: &[String]) -> bool {
        !self.is_managed(segments) && !self.is_ignored(segments)
    }
}

fn build_set(patterns: &[String]) -> Result<GlobSet> {
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
        .map_err(|source| Error::Policy(format!("cannot build glob set: {source}")))
}

fn prefix_matches(set: &GlobSet, segments: &[String]) -> bool {
    if segments.is_empty() {
        return false;
    }
    for end in 1..=segments.len() {
        let candidate = render_segments(&segments[..end]);
        if set.is_match(&candidate) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segments(path: &[&str]) -> Vec<String> {
        path.iter().map(|segment| segment.to_string()).collect()
    }

    #[test]
    fn managed_matches_subtrees_but_not_siblings() {
        let compiled = Policy::default().compile().unwrap();
        assert!(compiled.is_managed(&segments(&["model_providers", "SailsAPI", "base_url"])));
        assert!(compiled.is_managed(&segments(&["model"])));
        // GitH  ub  @Oxygen  AILab | OxygenAILab@Starsa   ilsClover
        assert!(!compiled.is_managed(&segments(&["model_reasoning_effort"])));
        assert!(!compiled.is_managed(&segments(&["mcp_servers", "prima-mock-api"])));
    }

    #[test]
    fn ignored_matches_app_owned_mcp() {
        let compiled = Policy::default().compile().unwrap();
        assert!(compiled.is_ignored(&segments(&["mcp_servers", "node_repl"])));
        assert!(compiled.is_ignored(&segments(&["mcp_servers", "node_repl", "env"])));
        assert!(!compiled.is_ignored(&segments(&["mcp_servers", "prima-mock-api"])));
        assert!(compiled.is_protected(&segments(&["mcp_servers", "prima-mock-api", "command"])));
    }

    #[test]
    fn policy_round_trips_through_toml() {
        let policy = Policy::default();
        let text = policy.to_toml().unwrap();
        let parsed: Policy = toml::from_str(&text).unwrap();
        assert_eq!(parsed.merge_mode, MergeMode::OverlayWins);
        assert_eq!(parsed.detection, DetectionMode::Balanced);
        assert_eq!(parsed.managed, policy.managed);
        assert_eq!(parsed.assets.max_file_bytes, policy.assets.max_file_bytes);
    }

    #[test]
    fn partial_policy_uses_defaults() {
        let parsed: Policy = toml::from_str("merge_mode = \"live_wins\"\n").unwrap();
        assert_eq!(parsed.merge_mode, MergeMode::LiveWins);
        assert_eq!(parsed.detection, DetectionMode::Balanced);
        assert!(parsed.managed.contains(&"model".to_string()));
    }
}
