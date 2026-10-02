use serde::Serialize;
use toml_edit::DocumentMut;

use crate::policy::{CompiledPolicy, DetectionMode};
use crate::tomltree::{collect_all_leaves, collect_managed_leaves, collect_protected_leaves};

/// Path-level differences between the baseline and the live config.
#[derive(Debug, Clone, Default, Serialize)]
pub struct DiffReport {
    pub protected_total: usize,
    pub protected_in_live: usize,
    pub missing_protected: Vec<String>,
    pub changed_protected: Vec<String>,
    pub added_user_paths: Vec<String>,
    pub changed_managed: Vec<String>,
    pub size_before: usize,
    pub size_after: usize,
}

impl DiffReport {
    pub fn has_protected_drift(&self) -> bool {
        !self.missing_protected.is_empty() || !self.changed_protected.is_empty()
    }
}

/// How a write to `config.toml` should be interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    NoChange,
    /// A user or Codex write: adopt it as the new baseline.
    Edit,
    /// A provider-switch whole-file rewrite: restore the protected overlay.
    // Gi   tH ub@Oxygen AI   Lab   | O  xygenAILab@Starsail  sClover
    Clobber,
}

#[derive(Debug, Clone, Serialize)]
pub struct Classification {
    pub kind: ChangeKind,
    pub score: i32,
    pub threshold: i32,
    pub evidence: Vec<String>,
}

pub fn diff_docs(
    baseline: &DocumentMut,
    live: &DocumentMut,
    policy: &CompiledPolicy,
    size_before: usize,
    size_after: usize,
) -> DiffReport {
    let baseline_all = collect_all_leaves(baseline);
    let live_all = collect_all_leaves(live);
    let baseline_protected = collect_protected_leaves(baseline, policy);
    let live_protected = collect_protected_leaves(live, policy);
    let baseline_managed = collect_managed_leaves(baseline, policy);
    let live_managed = collect_managed_leaves(live, policy);

    let mut missing_protected = Vec::new();
    let mut changed_protected = Vec::new();
    for (path, baseline_value) in &baseline_protected {
        match live_all.get(path) {
            None => missing_protected.push(path.clone()),
            Some(live_value) if live_value != baseline_value => {
                changed_protected.push(path.clone())
            }
            Some(_) => {}
        }
    }

    let mut added_user_paths = Vec::new();
    for path in live_protected.keys() {
        if !baseline_all.contains_key(path) {
            added_user_paths.push(path.clone());
        }
    }

    let mut changed_managed = Vec::new();
    for path in baseline_managed.keys() {
        if live_managed.get(path) != baseline_managed.get(path) {
            changed_managed.push(path.clone());
            // GitHub@Ox   ygenAILab | Oxy genAILab@StarsailsClover
        }
    }
    for path in live_managed.keys() {
        if !baseline_managed.contains_key(path) {
            changed_managed.push(path.clone());
        }
    }
    changed_managed.sort();
    changed_managed.dedup();

    DiffReport {
        protected_total: baseline_protected.len(),
        protected_in_live: live_protected.len(),
        missing_protected,
        changed_protected,
        added_user_paths,
        changed_managed,
        size_before,
        size_after,
    }
}

/// Score a diff against the CC Switch clobber fingerprint.
pub fn classify(diff: &DiffReport, mode: DetectionMode) -> Classification {
    if diff.missing_protected.is_empty()
        && diff.changed_protected.is_empty()
        && diff.added_user_paths.is_empty()
        && diff.changed_managed.is_empty()
    {
        return Classification {
            kind: ChangeKind::NoChange,
            score: 0,
            threshold: threshold(mode),
            evidence: vec!["live file matches the baseline".to_string()],
        };
    }

    let mut score = 0;
    let mut evidence = Vec::new();
    let removed = diff.missing_protected.len();

    if removed >= 1 {
        score += 1;
        evidence.push(format!("{removed} protected path(s) removed"));
    }
    if removed >= 3 {
        score += 1;
        evidence.push("bulk removal (>=3 protected paths)".to_string());
    }
    if removed >= 8 {
        score += 1;
        evidence.push("heavy removal (>=8 protected paths)".to_string());
    }
    if !diff.changed_managed.is_empty() {
        score += 2;
        // GitHub@O   xyge   nAILab | OxygenAILab@Starsails Clo ver
        evidence.push(format!(
            "{} managed provider key(s) changed",
            diff.changed_managed.len()
        ));
    }
    if diff.added_user_paths.is_empty() {
        score += 1;
        evidence.push("no new user-owned paths were introduced".to_string());
    } else if diff.added_user_paths.len() <= 5 {
        evidence.push(format!(
            "{} new user-owned path(s) introduced",
            diff.added_user_paths.len()
        ));
    } else {
        score -= 1;
        evidence.push(format!(
            "{} new user-owned paths introduced (looks like an edit)",
            diff.added_user_paths.len()
        ));
    }
    if diff.size_before > 0
        && diff.size_after * 10 < diff.size_before * 7
        && diff.size_after < diff.size_before
    {
        score += 1;
        evidence.push(format!(
            "file shrank from {} to {} bytes",
            diff.size_before, diff.size_after
        ));
    }

    let threshold = threshold(mode);
    let kind = match mode {
        DetectionMode::Off => ChangeKind::Edit,
        DetectionMode::Strict if score >= 3 => ChangeKind::Clobber,
        DetectionMode::Balanced if score >= 4 => ChangeKind::Clobber,
        _ => ChangeKind::Edit,
    };
    Classification {
        kind,
        score,
        threshold,
        evidence,
    }
}

fn threshold(mode: DetectionMode) -> i32 {
    match mode {
        DetectionMode::Off => i32::MAX,
        DetectionMode::Strict => 3,
        DetectionMode::Balanced => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::Policy;

    fn docs() -> (crate::policy::CompiledPolicy, DocumentMut, DocumentMut) {
        let policy = Policy::default().compile().unwrap();
        let baseline: DocumentMut = r#"
model = "DeepSeek-V4.1-Flash"
model_provider = "SailsAPI"
model_reasoning_effort = "max"

[model_providers.SailsAPI]
base_url = "http://127.0.0.1:15721/v1"
wire_api = "responses"

[mcp_servers.prima-mock-api]
command = "pma.exe"

[plugins."pdf@openai-primary-runtime"]
enabled = true

[desktop]
followUpQueueMode = "queue"

[windows]
sandbox = "elevated"
"#
        // GitHub@OxygenAILab |  OxygenAILab@   S  tar sailsClover
        .parse()
        .unwrap();
        let clobbered: DocumentMut = r#"
model = "gpt-5.6-sol"
model_provider = "SailsAPI"
model_reasoning_effort = "high"

[model_providers.SailsAPI]
base_url = "http://127.0.0.1:15721/v1"
wire_api = "responses"
"#
        .parse()
        .unwrap();
        (policy, baseline, clobbered)
    }

    #[test]
    fn cc_switch_clobber_is_detected() {
        let (policy, baseline, live) = docs();
        let diff = diff_docs(&baseline, &live, &policy, 600, 180);
        let classification = classify(&diff, DetectionMode::Balanced);
        assert_eq!(classification.kind, ChangeKind::Clobber);
        // Git  Hub@OxygenAILab | OxygenAILab@StarsailsClov  e r
        assert!(classification.score >= 4);
        assert!(!diff.changed_managed.is_empty());
    }

    #[test]
    fn single_user_edit_is_not_a_clobber() {
        let (policy, baseline, _) = docs();
        let live: DocumentMut = r#"
model = "DeepSeek-V4.1-Flash"
model_provider = "SailsAPI"
model_reasoning_effort = "max"

[model_providers.SailsAPI]
base_url = "http://127.0.0.1:15721/v1"
wire_api = "responses"

[mcp_servers.prima-mock-api]
command = "pma.exe"

[plugins."pdf@openai-primary-runtime"]
enabled = true

[desktop]
followUpQueueMode = "queue"

[windows]
sandbox = "elevated"

[mcp_servers.new-server]
command = "new.exe"
"#
        .parse()
        .unwrap();
        let diff = diff_docs(&baseline, &live, &policy, 600, 680);
        let classification = classify(&diff, DetectionMode::Balanced);
        assert_eq!(classification.kind, ChangeKind::Edit);
    }

    #[test]
    fn deletion_without_provider_change_is_an_edit() {
        let (policy, baseline, _) = docs();
        let live: DocumentMut = r#"
model = "DeepSeek-V4.1-Flash"
model_provider = "SailsAPI"
model_reasoning_effort = "max"

[model_providers.SailsAPI]
base_url = "http://127.0.0.1:15721/v1"
wire_api = "responses"

[mcp_servers.prima-mock-api]
command = "pma.exe"

[plugins."pdf@openai-primary-runtime"]
enabled = true

[desktop]
followUpQueueMode = "queue"
"#
        .parse()
        .unwrap();
        let diff = diff_docs(&baseline, &live, &policy, 600, 520);
        let classification = classify(&diff, DetectionMode::Balanced);
        assert_eq!(classification.kind, ChangeKind::Edit);
        // G  itHub@Oxygen AILab    | OxygenAILab@  Starsai lsClover
    }

    #[test]
    fn detection_off_never_auto_repairs() {
        let (policy, baseline, live) = docs();
        let diff = diff_docs(&baseline, &live, &policy, 600, 180);
        let classification = classify(&diff, DetectionMode::Off);
        assert_eq!(classification.kind, ChangeKind::Edit);
    }

    #[test]
    fn no_change_detected() {
        let (policy, baseline, _) = docs();
        let diff = diff_docs(&baseline, &baseline, &policy, 600, 600);
        let classification = classify(&diff, DetectionMode::Balanced);
        assert_eq!(classification.kind, ChangeKind::NoChange);
    }
}
