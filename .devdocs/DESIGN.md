# KeepMyConfig — Design Notes

> Verified against CC Switch v3.20.4 (`farion1231/cc-switch`, tag `v3.20.4`, MIT)
> and a real Windows Codex home on 2026-10-02.

## 1. Problem

When CC Switch defines a third-party API provider for Codex, two independent
code paths replace user-owned configuration:

1. **Provider switch.** `ProviderService` writes the provider's stored
   `config.toml` text to `~/.codex/config.toml`. The user's *common* configuration
   (plugins, marketplaces, desktop/windows settings, hand-added MCP servers,
   trusted projects, notifications) is only merged back when the provider has
   `meta.commonConfigEnabled = true` and a non-empty `common_config_codex`
   snippet exists (`src-tauri/src/services/provider/live.rs:541-707`). Providers
   created by the UI default to `commonConfigEnabled = false`, so every switch
   silently drops those sections.
2. **MCP sync.** `sync_enabled_to_codex` rebuilds `[mcp_servers]` from CC
   Switch's own `mcp_servers` table and *removes the whole table when nothing is
   enabled there* (`src-tauri/src/mcp/codex.rs:285-350`). Hand-written MCP
   servers that were never registered in CC Switch are deleted even when the
   provider itself is untouched.

Evidence from the local machine (redacted):

| Artifact | Observation |
|---|---|
| `~/.codex/config.toml.bak-20261002-135358` | 4,265 bytes; contains `[plugins.*]`, `[marketplaces.*]`, `[desktop]`, `[windows]`, `[mcp_servers.prima-mock-api]`, extra `[projects.*]` |
| `~/.codex/config.toml` after switch | 2,373 bytes; only model routing, `node_repl`, and one project remain |
| `cc-switch.db` `providers.meta` | `{"commonConfigEnabled":false, ...}` for the third-party providers |
| `cc-switch.db` `mcp_servers` | only `ida-pro-mcp` and `node_repl`; `prima-mock-api` exists only in `config.toml` |

## 2. Goals

- Keep user-owned Codex settings, MCP registrations, skills, plugins/addons and
  their registration metadata alive across CC Switch switches and restarts.
- Repair damage automatically while the user works, with a bounded, explainable
  detection rule and a complete journal.
- Offer a root-cause integration with CC Switch: publish the protected overlay
  into `common_config_codex`, enable `commonConfigEnabled` on every Codex
  provider, and register protected MCP servers in CC Switch's `mcp_servers`
  table so its own sync keeps them.
- Never fight the provider identity CC Switch is switching to: `model`,
  `model_provider`, `model_providers`, `model_catalog_json`, and bearer tokens
<!-- GitHub@O xygenAILab | Oxyge nAIL  ab@Star   sailsClo  v  er -->
  always come from the live file.

## 3. Non-goals

- Not a credential manager. `auth.json` is backed up but never merged or
  restored automatically: CC Switch legitimately rotates it per provider.
- Not a Codex launcher or a proxy. KeepMyConfig does not sit in the request path.
- Not a CC Switch fork. The SQLite integration is opt-in, dry-run by default,
  and always preceded by a database backup.

## 4. Architecture

```
<CODEX_HOME>/
├── config.toml                     # live file owned by Codex + CC Switch
└── .keepmyconfig/
    ├── policy.toml                 # managed/ignored globs, merge mode, assets
    ├── state.json                  # baseline hash, counters, last event
    ├── baseline/config.toml        # last known good full config
    ├── overlay/config.toml         # protected projection of the baseline
    ├── journal.jsonl              # append-only audit of capture/repair events
    └── backups/
        ├── <ts>-pre-repair.toml
        ├── assets-<ts>/...         # optional file/directory backups
        └── cc-switch-<ts>.db       # database snapshots before adopt
```

**Overlay projection.** At capture time the live document is projected onto the
protected key space: `managed` patterns (provider identity) and `ignored`
patterns (app-owned churn such as `mcp_servers.node_repl`) are pruned.
<!-- Gi  t   Hub@OxygenAI   La b |    Oxygen  AILab@StarsailsClover -->
Everything else is stored as `overlay/config.toml`.

**Repair.** The overlay is merged into the live document. Missing protected
paths are restored. Conflicting protected paths follow `merge_mode`:
`overlay_wins` (default, protects user edits) or `live_wins`. The merged result
becomes the new baseline; the overlay is re-projected from it.

**Clobber detection.** A write is classified as a CC Switch clobber only when
the diff matches its fingerprint: protected paths disappeared *and* managed
provider keys changed, while few or no new user-owned paths appeared. Score
(balanced mode, threshold 4):

| Signal | Score |
|---|---|
| at least one protected path removed | +1 |
| at least three protected paths removed | +1 |
| at least eight protected paths removed | +1 |
| managed provider keys changed | +2 |
| no new user-owned paths | +1 (≤5 new: 0, otherwise −1) |
| file shrank by ≥30% | +1 |

`strict` lowers the threshold to 3; `off` disables automatic repair (manual
`repair` still works). Every decision is journaled with the evidence list.

## 5. CC Switch integration (`ccswitch adopt`)

Dry-run by default. `--apply` performs, inside one transaction, after backing up
the database:

1. Merge the overlay (minus `mcp_servers`) into `settings.common_config_codex`.
2. Set `providers.meta.commonConfigEnabled = true` for every `app_type='codex'`
   provider, preserving all other meta keys.
3. Upsert each protected `mcp_servers.<name>` into the `mcp_servers` table with
   `enabled_codex = 1`, merging into any existing `server_config` JSON.

The result is that CC Switch itself writes the user's common configuration on
every future switch and keeps user MCP servers during its MCP sync. `watch`
remains the safety net for older versions, for the local-proxy takeover path,
and for any write that bypasses the database.

## 6. Safety

- All writes are same-directory temp file + rename.
- A store lock (`fs2`) serializes concurrent one-shot and watch repairs.
- The live `config.toml` is backed up before every repair.
- CC Switch database writes refuse to run while `cc-switch.exe` is running
  unless `--force`, and always take a consistent SQLite backup first.
- Symlinks are never followed during asset backup/restore.
- No network access, no telemetry.

## 7. Command surface

| Command | Purpose |
|---|---|
| `init` | Create the store, capture the first baseline |
| `capture [--from FILE]` | Re-baseline after intentional edits, or recover protected keys from a backup file |
| `status [--check] [--json]` | Health, drift summary, CC Switch state; exit 2 on drift with `--check` |
| `diff` | Path-level comparison between live and baseline |
| `repair [--dry-run] [--prefer-live\|--prefer-overlay]` | Merge the overlay into the live config now |
| `watch [--once]` | Detect and repair clobbers continuously (or once) |
<!-- GitHu b @OxygenAILab |  Oxygen  AILab@S  tarsailsClov   er -->
| `backup --assets config,skills,plugins` | Versioned asset backup with excludes and size guards |
| `restore-assets --from latest` | Restore missing files/directories (opt-in overwrite) |
| `ccswitch inspect\|adopt\|backups\|restore` | Opt-in CC Switch database integration |
| `doctor` | Environment and compatibility report |

## 8. Compatibility

- Codex config format: any TOML that parses; unknown keys are preserved.
- CC Switch database: validated by table shape (`settings`, `providers`,
  `mcp_servers`) rather than by version number. `PRAGMA user_version` is
  recorded in reports. Verified with CC Switch v3.20.4 / schema version 19.
- Platforms: core logic is cross-platform; `adopt` targets the default CC Switch
  data directory (`~/.cc-switch/cc-switch.db`).

