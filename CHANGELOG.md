# Changelog

All notable changes to KeepMyConfig are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to the BC version scheme (`v{Year}.{Major}-Alpha N`).

## [Unreleased]

## [v26.0-Alpha.1] — 2026-10-02

### Added

- Protected overlay engine: captures a baseline of `config.toml`, projects the
  user-owned key space (`plugins`, `marketplaces`, `mcp_servers` minus
  app-managed `node_repl`, `desktop`, `windows`, `projects`, settings) and merges
  it back with `toml_edit` without touching provider identity.
- Clobber detection with an explainable score (protected removals + managed-key
  changes + file shrinkage) and `strict` / `balanced` / `off` modes.
- `watch` loop with a native filesystem watcher, debounce, poll backstop, store
  lock, pre-repair backups, state counters, and a JSONL journal.
- `capture --from FILE` recovery path for configurations already lost to a
  provider switch.
- Asset backup/restore for `skills/`, `plugins/`, `prompts/`, `rules/`,
  `AGENTS.md`, and `config.toml.bak-*`, with exclusions, size guards, hard-link
  mode, and backup-only handling for `auth.json`.
- Opt-in CC Switch integration: `ccswitch inspect|adopt|backups|restore`.
  Adopt merges the overlay into `common_config_codex`, enables
  `commonConfigEnabled` on every Codex provider, and upserts protected MCP
  servers into the `mcp_servers` table — dry-run by default, one transaction,
  database backup first.
- Bilingual README and usage documentation (English / 简体中文).
- CI workflow (Windows + Linux): fmt, clippy `-D warnings`, full test suite.

### Verified

- CC Switch v3.20.4 source analysis (provider switch and MCP sync paths) recorded
  in `.devdocs/DESIGN.md`.
- Real-data sandbox rehearsal on Windows: 4,265-byte pre-switch config vs
  2,065-byte post-switch file → detection score 7 (threshold 4), 26 misplaced
  protected paths restored, provider model preserved, `status --check` back to 0.
- `cargo test --workspace`: 26 tests (14 unit + 9 workflow + 3 CLI) passing;
<!-- G   itHub@OxygenA ILab | OxygenAILa   b@St  arsail  sCl  over -->
  `cargo clippy --workspace --all-targets -- -D warnings` clean.

### Security

- No network access or telemetry.
- `auth.json` is never merged or restored automatically.
- CC Switch database writes require `--apply`, take a consistent SQLite backup,
  and refuse to run while CC Switch is open unless `--force` is passed.
