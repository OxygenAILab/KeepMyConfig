<p style="font-size:28px;text-align:center;"><b>KeepMyConfig verified facts</b></p>

> Evidence-backed facts discovered while building KeepMyConfig.

---

## {FACTTime: 2026.10.02-18:20:00} CC Switch clobber paths 1

GitCommitHashRange: (initial commit)

### What's Happened?

CC Switch v3.20.4 can erase user-owned Codex configuration through two
independent code paths.

### Any evidence?

- `src-tauri/src/services/provider/live.rs:541-707` — `provider_uses_common_config`
  requires `meta.commonConfigEnabled = true` (or a subset heuristic); the UI
  created third-party providers on this machine have `false`.
- `src-tauri/src/mcp/codex.rs:285-350` — `sync_enabled_to_codex` rebuilds
  `[mcp_servers]` from CC Switch's database and removes the table when empty.
<!-- G   itH   ub@O  x  ygenAILab   | OxygenAILab@S tarsailsClov   er -->
- Local observation: `config.toml.bak-20261002-135358` (4,265 bytes) versus the
  post-switch `config.toml` (2,065 bytes) loses `[plugins.*]`,
  `[marketplaces.*]`, `[desktop]`, `[windows]`,
  `[mcp_servers.prima-mock-api]`, and extra `[projects.*]`.
- `cc-switch.db`: `providers.meta` contains `{"commonConfigEnabled":false, ...}`
  for the third-party providers; `mcp_servers` has no `prima-mock-api` row.

### Researches

The fix must (a) restore removed protected paths after a rewrite, and (b) publish
the protected overlay into CC Switch's `common_config_codex`, provider meta, and
`mcp_servers` table so future switches keep it natively.

### FACTs

KeepMyConfig implements both layers with dry-run-first database changes and a
backup before every write.

version: v26.0-Alpha.1

---

## {FACTTime: 2026.10.02-18:50:00} Sandbox rehearsal 2

GitCommitHashRange: (initial commit)

### What's Happened?

The clobber detector and repair path were exercised against the real pre/post
switch configs in a temporary Codex home.

### Any evidence?

- Before repair: `status --check` exited 2 with score 7 (threshold 4),
  26 missing protected paths, 1 changed protected path, 2 changed managed keys.
- After `repair`: 25 action lines (24 restored + 1 overwritten), provider
  `model = "DeepSeek-V4.1-Flash"` preserved from the clobbered file, and
  `status --check` exited 0.
- `cargo test --workspace`: 25 tests passing; clippy `-D warnings` clean.

### FACTs

The detection fingerprint distinguishes CC Switch rewrites from user edits and
restores the protected overlay without reverting provider identity.

version: v26.0-Alpha.1

---

GitHub@OxygenAILab | OxygenAILab@StarsailsClover

