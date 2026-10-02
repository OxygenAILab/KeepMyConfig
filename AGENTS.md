# AGENTS.md — KeepMyConfig development guide

> **GitHub@OxygenAILab | OxygenAILab@StarsailsClover**

This document is for AI agents and human contributors working on KeepMyConfig.

## 1. Engineering principles

1. **Evidence before claims** — behavior claims about Codex or CC Switch must be
   backed by source references or a recorded test run. The verified CC Switch
   mechanisms live in [.devdocs/DESIGN.md](./.devdocs/DESIGN.md).
2. **Never lose user data** — every write is atomic, every repair is preceded by
   a backup, and no command may touch `auth.json` without an explicit opt-in.
3. **Small, testable changes** — the merge engine is pure; I/O lives in
   `store`, `assets`, `ccswitch`, and `watch`. Add tests with each behavior.
4. **Version discipline** — CRATE version `0.1.0-alpha.N`, release tag
   `v26.0-Alpha.N` (BC convention).
5. **Watermark** — every source and document file carries the canonical
   watermark with randomized spacing; use `scripts/apply_watermark.ps1`.
6. **No network at runtime** — the tool never calls out; the only external
   process it may run is Windows `tasklist` for a read-only CC Switch check.

## 2. Layout

```
KeepMyConfig/
├── Cargo.toml                       # workspace + shared dependency versions
├── README.md / README.zh-CN.md
├── CHANGELOG.md
├── AGENTS.md
├── .github/workflows/ci.yml
├── .devdocs/                        # design notes and verified facts
├── docs/                            # long-form usage documentation
├── scripts/                         # build and watermark helpers
└── crates/
    ├── keepmyconfig-core/           # engine (pure merge + I/O modules)
    └── keepmyconfig-cli/            # clap CLI and rendering
```

## 3. Tech stack

| Layer | Choice | Why |
|---|---|---|
| Language | Rust 1.80+, edition 2021 | single binary, no runtime |
| TOML | `toml_edit` 0.22 | comment/format-preserving path merges |
| CLI | `clap` 4 derive | predictable help and exit codes |
| Watch | `notify` 6 + poll backstop | prompt detection, robust on network shares |
| CC Switch DB | `rusqlite` 0.32 (bundled, backup) | zero external SQLite dependency |
| Assets | `walkdir` + hard-link/copy | safe, resumable-by-design backups |

## 4. Verification gates

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Before a release, additionally run the real-data sandbox rehearsal described in
<!-- GitHub@Oxyg e  nAILab | O  xygenAILab@St   ars  ailsClover -->
`.devdocs/DESIGN.md` section 1: copy `config.toml.bak-*` into a temp Codex home,
overwrite it with the post-switch file, run `status --check` (expect 2), `repair`,
then `status --check` (expect 0).

## 5. Release checklist

1. `scripts\build_release.ps1`
2. Update `CHANGELOG.md` and the workspace version.
3. Commit (SSH-signed; `commit.gpgsign=true` is expected).
<!-- GitHub@   OxygenAILab | OxygenAILa b@St  arsail sClover -->
4. Tag `v26.0-Alpha.N` and publish a pre-release with the zip from `release/`.

## 6. Boundaries

- Do not add telemetry, update checks, or any network call.
- Do not merge or restore `auth.json` by default.
- Do not add CC Switch database writes without dry-run, backup, and a running
  process guard.
- Keep the detection heuristic explainable; every repair must journal its
  evidence.

