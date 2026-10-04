# Changelog

All notable changes to KeepMyConfig are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to the BC version scheme (`v{Year}.{Major}-Alpha N`).

## [Unreleased]

## [v26.0-Alpha.5] — 2026-10-05

### Fixed

- `autostart install` no longer fails when the host denies Task Scheduler task
  creation (observed on a non-elevated session: `schtasks /Create /SC ONLOGON`
  returned `Access is denied`). `--method auto` now falls back to a Startup
  folder loop script that runs `watch --once --quiet` every N minutes, so
  protection survives logons and app updates without administrator rights.

### Added

- `autostart install --method auto|task|startup`, plus Startup-script state in
  `autostart status` and `doctor`.

### Verified

- On the affected machine the fallback script was installed under the user's
  Startup folder, a watch helper was started immediately, and the store journal
  recorded the periodic one-shot run.

## [v26.0-Alpha.4] — 2026-10-05

### Added

- `autostart install|uninstall|status` (Windows): installs a logon daemon
  (`KeepMyConfig-Watch`) plus a periodic one-shot repair task
  (`KeepMyConfig-Check`, five-minute default) through `schtasks`, using small
  wrapper scripts next to the executable. `doctor` now reports autostart state.
  This closes the gap that let an update kill the manually started watch and
  leave the configuration unprotected.
- Codex-update-aware rewrite detection: a change of
  `mcp_servers.node_repl.env.BROWSER_USE_CODEX_APP_VERSION` combined with any
  protected-path removal is treated as an app-rewrite clobber, and the removal
  of three or more complete `mcp_servers` / `plugins` / `marketplaces` /
  `projects` entries adds clobber weight.

### Fixed

- A Codex app update that rewrites `config.toml` while preserving provider
  identity no longer scores below the clobber threshold. The observed 0.147
  rewrite (app build `26.930.31428` -> `26.930.31730`) dropped three MCP
  servers, four curated plugin entries, and reset reasoning effort while
  introducing `desktop.conversationDetailMode`; the old score was 3 against a
  threshold of 4, so `watch` would have captured the damage as a user edit.

### Verified

- Real incident recovery on 2026-10-05: `status --check` exited 2 with 12
  missing protected paths and one changed value; `repair` restored all 12 and
  rewrote `model_reasoning_effort` to the user value, leaving a pre-repair
  backup. `status --check` returned 0 afterwards.
- New regression tests: an app-update rewrite is a clobber; an app update with
  no losses is an edit; three complete entries removed is a clobber; a single
  entry removal remains an edit; the end-to-end `process()` test repairs the
  0.147-style rewrite instead of capturing it.

## [v26.0-Alpha.3] — 2026-10-03

### Fixed

- `notify` is no longer captured into the protected overlay. The Codex desktop app
  rewrites it with a version-scoped runtime path (`runtimes\cua_node\<hash>\...`), so
  any copy captured before an app update is stale. The explicit `repair` command is
  `manual = true` and therefore merges without requiring a clobber fingerprint, which
  meant `status` reporting a benign `Edit` (score 1, threshold 4) still led the user to
  a `repair` that silently rewrote `notify` back to a runtime directory that no longer
  exists — breaking the `turn-ended` Computer Use hook. `notify` now sits in the
  default `ignored` set alongside `mcp_servers.node_repl`, which shares the same
  lifecycle. Remove it from `ignored` in `policy.toml` to protect a hand-written value.

### Verified

- Recorded reproduction of the defect: baseline `notify` -> runtime
  `2134bcb1950af07e`, live `notify` -> runtime `81ea4d5168ddd0a3` (app update),
  `repair` -> `overwritten notify`, runtime `2134bcb1950af07e` restored.
- Regression test `manual_repair_keeps_an_app_updated_notify`, plus the existing
  `watch_repairs_a_cc_switch_style_clobber` now asserts app-owned churn is left alone.

## [v26.0-Alpha.2] — 2026-10-02

### Fixed

- Manifest path validation is now platform-independent: Windows drive-letter
  paths, UNC paths, POSIX absolute paths, empty segments, `.` and `..` are all
  rejected before any restore write, on every host OS. This also fixes the
  Linux CI leg of v26.0-Alpha.1 (the check previously relied on
  `std::path::Component`, which does not recognize `C:/...` on Unix).

### Verified

- `cargo test --workspace`: 26 tests passing on Windows and Linux CI.

## [v26.0-Alpha.1] — 2026-10-02

> Superseded by v26.0-Alpha.2 (Linux CI path-validation fix). The Windows
> artifact remains functionally identical for the primary platform.

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
