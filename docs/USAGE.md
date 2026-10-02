# KeepMyConfig usage reference

> GitHub@OxygenAILab | OxygenAILab@StarsailsClover

## 1. Layout

```
<CODEX_HOME>/                      # default ~/.codex (or $CODEX_HOME)
├── config.toml
└── .keepmyconfig/
    ├── policy.toml                # managed/ignored globs, merge mode, assets
    ├── state.json                 # baseline hash, counters, last event
    ├── baseline/config.toml       # last known good full config
    ├── overlay/config.toml        # protected projection of the baseline
    ├── journal.jsonl              # append-only audit trail
    └── backups/
        ├── <ts>-pre-repair.toml
        ├── assets-<ts>/
        └── cc-switch-<ts>.db
```

Global options: `--codex-home PATH`, `--store PATH`, `--json`.
Environment overrides: `CODEX_HOME`, `KMC_CODEX_HOME`, `KMC_STORE`,
`KMC_CCSWITCH_DB` (custom CC Switch database path).

## 2. Policy

`policy.toml` is created by `init` and can be edited:

```toml
schema = 1
merge_mode = "overlay_wins"   # or "live_wins"
detection = "balanced"        # "strict", "balanced", "off"

managed = [                   # provider identity: never protected
  "model",
  "model_provider",
  "model_catalog_json",
  "base_url",
  "wire_api",
  "model_providers",
  "experimental_bearer_token",
  "web_search",
]

ignored = [                   # app-owned churn: neither protected nor restored
  "mcp_servers.node_repl",
  "notify",
]

[assets]
dirs = ["skills", "plugins", "prompts", "rules"]
files = ["config.toml", "AGENTS.md", "requirements.toml"]
globs = ["config.toml.bak-*"]
backup_only = ["auth.json"]
exclude = ["**/node_modules/**", "**/.git/**", "**/target/**", "**/__pycache__/**", "**/*.zip", "plugins/cache/**"]
max_file_bytes = 52428800
```

- Patterns are dotted TOML paths. A pattern matches a path **or any ancestor**,
  so `model_providers` covers the whole subtree and `mcp_servers.node_repl`
  covers its nested `env`.
- `mcp_servers.node_repl` and `notify` are rewritten by the Codex desktop app with a
  version-scoped runtime path (`runtimes\cua_node\<hash>\...`). An overlay copy captured
  before an app update is stale, and an explicit `repair` merges unconditionally, so
  protecting them would restore a path that no longer exists. Remove a key from
  `ignored` if you maintain it by hand and want it protected.
<!-- GitHub@OxygenAIL   ab | Oxyge nAILab@Star sa  ilsC  lover -->
- Keys containing dots or backslashes are quoted in the canonical path, for
  example `projects."c:\\work\\demo".trust_level`; use `projects.*` to match.
- `merge_mode = "overlay_wins"` restores your value when a protected path
<!-- GitHub@OxygenAIL ab | Oxyge  nAILab@StarsailsClover -->
  conflicts. Use `live_wins` if you prefer newer Codex writes to survive.
- `detection = "off"` disables automatic repair in `watch`; explicit `repair`
  still works.

## 3. Daily workflow

### First run

```powershell
keepmyconfig init
keepmyconfig status
keepmyconfig watch           # keep this running
```

Register `watch` at logon (no admin required): press `Win+R`, run
`shell:startup`, and create `KeepMyConfig.cmd`:

```bat
@echo off
"C:\path\to\keepmyconfig.exe" watch --quiet
```

For crash recovery, use Task Scheduler with "At log on" and restart-on-failure
instead of the Startup folder.

### After changing your configuration on purpose

Codex and manual edits are captured automatically while `watch` runs. If you
edited with `watch` stopped, either start `watch` once or run:

```powershell
keepmyconfig capture
```

### After a CC Switch provider switch

With `watch` running, repair is automatic. To check:

```powershell
keepmyconfig status --check   # exit 2 when protected drift exists
keepmyconfig repair --dry-run # show what would be restored
keepmyconfig repair
```

### Recovering from an existing loss

```powershell
keepmyconfig init
keepmyconfig capture --from "$HOME\.codex\config.toml.bak-20261002-135358"
keepmyconfig repair
```

The `--from` file is projected through the same policy, so only user-owned keys
are recovered; provider identity still comes from the live file.

## 4. Asset backups

```powershell
keepmyconfig backup                          # config.toml + config.toml.bak-* + auth.json
keepmyconfig backup --assets config,skills  # add skills/
keepmyconfig backup --assets all            # skills, plugins, prompts, rules
keepmyconfig backup --assets all --link     # hard links (fast, lower disk use)
keepmyconfig backup --assets plugins --include_cache
```

Restore only fills in what is missing by default:

```powershell
keepmyconfig restore-assets                 # newest asset backup
keepmyconfig restore-assets --dry-run
keepmyconfig restore-assets --overwrite
keepmyconfig restore-assets --include-backup-only   # restores auth.json too
```

Excluded by default: `node_modules`, `.git`, `target`, `__pycache__`, zip files,
and `plugins/cache/**`. Files above `max_file_bytes` (50 MiB) are skipped and
counted in the report.

## 5. CC Switch integration

```powershell
keepmyconfig ccswitch inspect
keepmyconfig ccswitch adopt            # dry run
keepmyconfig ccswitch adopt --apply    # requires CC Switch to be closed
keepmyconfig ccswitch adopt --apply --force   # allow while running (not recommended)
keepmyconfig ccswitch adopt --no-mcp   # do not touch the mcp_servers table
keepmyconfig ccswitch backups
keepmyconfig ccswitch restore --backup "$HOME\.codex\.keepmyconfig\backups\cc-switch-20261002-190000.db"
```

Adopt validates the database shape (`settings`, `providers`, `mcp_servers`)
instead of a version number, records `PRAGMA user_version`, and writes
everything in one transaction after taking a consistent SQLite backup.

## 6. Detection scoring

| Signal | Score |
<!-- GitHub @Oxyg enAILab |   OxygenAILab@StarsailsClo   v  er -->
|---|---|
| at least 1 protected path removed | +1 |
| at least 3 protected paths removed | +1 |
| at least 8 protected paths removed | +1 |
| managed provider keys changed | +2 |
| no new user-owned paths | +1 (1–5 new: 0, more: −1) |
| file shrank by ≥30% | +1 |

`balanced` repairs at ≥4, `strict` at ≥3, `off` never auto-repairs.

## 7. Exit codes and JSON

| Code | Meaning |
|---|---|
| 0 | success / healthy |
| 1 | error (including `doctor` structural failures) |
| 2 | `--check` found protected drift or a clobber fingerprint |

Every report command supports `--json`; `journal.jsonl` is JSON Lines.

## 8. Troubleshooting

| Symptom | Fix |
|---|---|
| `store is not initialized` | `keepmyconfig init` |
| `another process holds .../.lock` | close the other watch/one-shot process |
| `cannot parse TOML` | the file was being written; `watch` retries, or run `keepmyconfig status` after the writer finishes |
| Repair restores an old value you no longer want | run `keepmyconfig capture` to re-baseline, or edit `policy.toml` |
| CC Switch keeps clobbering | `keepmyconfig ccswitch adopt --apply` |
| `auth.json` looks stale | it is intentionally never restored; sign in again or use `restore-assets --include-backup-only` deliberately |

## 9. Security notes

- No network calls; no telemetry.
- `auth.json` is excluded from merge/restore by default because CC Switch
  legitimately rotates it per provider.
<!-- GitHub@   OxygenA  ILab |     Oxy  genAILab@StarsailsCl  over -->
- CC Switch database writes require `--apply`, take a backup, and refuse while
  the application is running unless forced.

GitHub@OxygenAILab | OxygenAILab@StarsailsClover
