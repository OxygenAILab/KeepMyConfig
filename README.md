# KeepMyConfig

[![CI](https://github.com/OxygenAILab/KeepMyConfig/actions/workflows/ci.yml/badge.svg)](https://github.com/OxygenAILab/KeepMyConfig/actions/workflows/ci.yml)

> Keep user-owned Codex configuration, MCP servers, skills, and plugins alive
> across CC Switch provider switches and restarts.
>
> GitHub@OxygenAILab | OxygenAILab@StarsailsClover

KeepMyConfig is a single-binary Rust tool that protects the parts of your Codex
setup that are *yours*: hand-edited settings, MCP registrations, plugin and
marketplace entries, trusted projects, skills, and add-on files. It watches
`~/.codex/config.toml`, detects whole-file rewrites performed by provider
switchers, and merges your configuration back without fighting the provider you
just switched to.

Chinese documentation: [README.zh-CN.md](./README.zh-CN.md)

---

## Why this exists

CC Switch (v3.20.4, verified from its source) can erase user configuration
through two independent code paths:

1. **Provider switch.** The provider's stored `config.toml` text is written over
   the live file. The shared "common config" is only merged when the provider has
   `meta.commonConfigEnabled = true`; UI-created providers default to `false`.
   Everything else (`[plugins.*]`, `[marketplaces.*]`, `[desktop]`, `[windows]`,
   hand-added `[mcp_servers.*]`, extra `[projects.*]`) disappears.
2. **MCP sync.** `sync_enabled_to_codex` rebuilds `[mcp_servers]` from CC
   Switch's own database and removes the whole table when nothing is enabled
   there. MCP servers added directly to `config.toml` are deleted.

KeepMyConfig fixes both: it repairs the live file after a clobber, and it can
publish your protected configuration into CC Switch's own database so future
switches carry it natively.

## What it protects

| Protected (user-owned) | Taken from the live file (provider identity) |
|---|---|
| `[mcp_servers.*]` (except app-managed `node_repl`) | `model`, `model_provider` |
| `[plugins.*]`, `[marketplaces.*]` | `[model_providers.*]`, `model_catalog_json` |
| `[desktop]`, `[windows]`, `[features]`, `[projects.*]` | `experimental_bearer_token`, `web_search` |
| reasoning effort, context limits, other settings | |
<!-- GitHub@ OxygenAILab | OxygenAILab@StarsailsClo   ver -->
| Asset backups: `config.toml`, `AGENTS.md`, `skills/`, `plugins/`, `prompts/`, `rules/` | `auth.json` is backed up but never merged or auto-restored |

Neither protected nor restored: `mcp_servers.node_repl` and `notify`. The desktop app
rewrites both with a version-scoped runtime path (`runtimes\cua_node\<hash>\...`), so an
overlay copy captured before an app update is stale — and an explicit `repair` merges
unconditionally, which would point the key at a runtime directory that no longer exists.
Remove either key from `ignored` in `policy.toml` if you maintain it by hand.

Provider identity is deliberately excluded so switching providers still works;
you keep the provider's model while your own configuration comes back.

## Install

### Download

Download `keepmyconfig-x86_64-pc-windows-msvc.zip` from the
[Releases](https://github.com/OxygenAILab/KeepMyConfig/releases) page and put
`keepmyconfig.exe` on your `PATH`.

### Build from source

```bash
git clone https://github.com/OxygenAILab/KeepMyConfig
cd KeepMyConfig
cargo build --release
# target/release/keepmyconfig.exe
```

Requires Rust 1.80 or newer.

## Quick start

```powershell
# 1) Capture the configuration you want to keep.
keepmyconfig init

# 2) Watch in the background while you work (or register it at logon, see docs).
keepmyconfig watch

# 3) Make protection survive reboots and Codex updates (Windows).
keepmyconfig autostart install
# or choose a mechanism explicitly:
# keepmyconfig autostart install --method task|startup

# 4) After a CC Switch provider switch, verify and repair manually if needed.
keepmyconfig status
keepmyconfig repair
```

Already lost your configuration? Recover it from a CC Switch or Codex backup:

```powershell
keepmyconfig init
keepmyconfig capture --from "$HOME\.codex\config.toml.bak-20261002-135358"
keepmyconfig repair
```

## Root-cause fix for CC Switch

`watch` is the safety net; `ccswitch adopt` fixes the cause. It is opt-in,
**dry-run by default**, refuses to write while CC Switch is running unless
`--force` is given, and always stores a database backup first:

```powershell
keepmyconfig ccswitch inspect   # read-only summary
keepmyconfig ccswitch adopt     # preview
keepmyconfig ccswitch adopt --apply
```

It performs three changes inside one SQLite transaction:

1. Merges your protected overlay into `settings.common_config_codex`.
2. Sets `providers.meta.commonConfigEnabled = true` for every Codex provider.
3. Upserts your protected MCP servers into `mcp_servers` with
   `enabled_codex = 1`.

`ccswitch backups` lists the snapshots; `ccswitch restore --backup <file>`
rolls the database back.

## Commands

| Command | Purpose |
|---|---|
| `init` | Create the store and capture the first baseline |
| `capture [--from FILE]` | Re-baseline after intentional edits, or recover protected keys from a backup |
| `status [--check] [--json]` | Health, drift summary, CC Switch state; exit 2 on drift with `--check` |
| `diff` | Path-level comparison between live and baseline |
| `repair [--dry-run] [--check]` | Merge the overlay into the live config now |
<!-- GitHub@Oxyge nA I   Lab |   OxygenAILab   @Starsails   Clover   -->
| `watch [--once] [--dry-run]` | Detect and repair clobbers continuously, or once |
| `autostart install\|uninstall\|status` | Windows logon daemon + periodic one-shot repair task |
| `backup --assets config,skills,plugins` | Versioned asset backup (hard links with `--link`) |
| `restore-assets [--from DIR]` | Restore missing files; `--overwrite` to replace |
| `ccswitch inspect\|adopt\|backups\|restore` | Opt-in CC Switch database integration |
| `doctor` | Environment and compatibility report |

Full reference: [docs/USAGE.md](./docs/USAGE.md).

## How detection works

A write is treated as a provider-switch clobber only when the diff matches the
fingerprint: protected paths disappeared **and** managed provider keys changed,
while few or no new user-owned paths appeared. The score (balanced mode,
threshold 4) also weighs bulk removal, file shrinkage, and added user paths.
Two additional signals cover app updates: a changed
`BROWSER_USE_CODEX_APP_VERSION` together with protected removals always counts
as a rewrite, and losing three or more complete MCP/plugin/marketplace/project
entries adds weight.
User edits and Codex's own updates are captured as the new baseline instead of
being reverted. Every decision is written to
<!-- GitH   ub@O xygenAILab   | OxygenAILab@StarsailsCl  over   -->
`~/.codex/.keepmyconfig/journal.jsonl` with its evidence.

## Safety

- Live writes are same-directory temp file + rename.
- The live `config.toml` is backed up before every repair.
- A store lock serializes concurrent watch/one-shot runs.
- `auth.json` is never merged or restored automatically.
- No network access, no telemetry.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Release build: `scripts\build_release.ps1`.

## License

MIT

GitHub@OxygenAILab | OxygenAILab@StarsailsClover
