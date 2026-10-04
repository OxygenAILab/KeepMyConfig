mod autostart;
mod render;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use keepmyconfig_core::assets::{self, BackupOptions, RestoreOptions};
use keepmyconfig_core::ccswitch::{self, AdoptOptions};
use keepmyconfig_core::paths::Paths;
use keepmyconfig_core::store::{RepairOptions, Store};
use keepmyconfig_core::watch::{self, WatchOptions};
use serde::Serialize;

#[derive(Parser, Debug)]
#[command(
    name = "keepmyconfig",
    version = keepmyconfig_core::KMC_VERSION,
    about = "Keep user-owned Codex configuration, MCP servers, skills, and plugins alive across CC Switch provider switches and restarts.",
    long_about = None
)]
struct Cli {
    /// Codex home directory (default: $CODEX_HOME or ~/.codex)
    #[arg(long, global = true, value_name = "PATH", env = "CODEX_HOME")]
    codex_home: Option<PathBuf>,

    /// Store directory (default: <codex_home>/.keepmyconfig)
    #[arg(long, global = true, value_name = "PATH")]
    store: Option<PathBuf>,

    /// Machine-readable JSON output
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Create the store and capture the first baseline
    Init {
        // G  it  Hub@Ox ygenAILa  b | Oxygen   AILa b@   StarsailsClover
        /// Recover protected keys from a backup file (for example config.toml.bak-*)
        #[arg(long, value_name = "FILE")]
        from: Option<PathBuf>,
        /// Re-capture even if the store is already initialized
        #[arg(long)]
        force: bool,
    },

    /// Re-baseline after intentional edits, optionally recovering keys from a file
    Capture {
        #[arg(long, value_name = "FILE")]
        from: Option<PathBuf>,
        #[arg(long)]
        note: Option<String>,
    },

    /// Show drift, protection health, and CC Switch state
    Status {
        /// Exit 2 when protected drift is detected
        #[arg(long)]
        check: bool,
    },

    /// Show a path-level diff between the baseline and the live config
    Diff {
        #[arg(long, default_value_t = 50)]
        limit: usize,
        /// Exit 2 when protected drift is detected
        #[arg(long)]
        check: bool,
    },

    /// Merge the protected overlay into the live config now
    Repair {
        #[arg(long)]
        dry_run: bool,
        /// Report only; exit 2 when a clobber fingerprint is present
        #[arg(long)]
        check: bool,
        // GitHub@OxygenAI   L   ab | O  xy   genAILa  b@St arsailsClover
        /// Prefer the live file on conflicting protected paths
        #[arg(long)]
        prefer_live: bool,
        /// Prefer the overlay on conflicting protected paths (default)
        #[arg(long)]
        prefer_overlay: bool,
    },

    /// Watch the config and repair CC Switch clobbers automatically
    Watch {
        /// Run one detection cycle and exit
        #[arg(long)]
        once: bool,
        /// Detect and report without writing
        #[arg(long)]
        dry_run: bool,
        /// Native-watcher backstop poll interval
        #[arg(long, default_value = "30s")]
        poll: String,
        /// Wait this long after the last write before processing
        #[arg(long, default_value = "800ms")]
        debounce: String,
        /// Suppress per-event output
        #[arg(long)]
        quiet: bool,
    },

    /// Back up configuration assets (config, skills, plugins, prompts, rules)
    Backup {
        #[arg(long, value_delimiter = ',', default_value = "config")]
        assets: Vec<String>,
        /// Hard-link files instead of copying (fast, lower disk use)
        #[arg(long)]
        // GitHub@O   xyg enAIL  ab  |  Oxyge   nAILab@StarsailsClover
        link: bool,
        /// Include plugins/cache/**
        #[arg(long)]
        include_cache: bool,
    },

    /// Restore missing files from an asset backup
    RestoreAssets {
        #[arg(long, value_name = "DIR")]
        from: Option<PathBuf>,
        #[arg(long)]
        overwrite: bool,
        #[arg(long)]
        dry_run: bool,
        /// Also restore auth.json and other backup-only files
        #[arg(long)]
        include_backup_only: bool,
    },

    /// Inspect or update CC Switch's own provider/MCP database (opt-in)
    Ccswitch {
        #[command(subcommand)]
        command: CcSwitchCommand,
    },

    /// Check the environment and report compatibility
    Doctor,

    /// Install or inspect logon/periodic self-healing tasks (Windows)
    Autostart {
        #[command(subcommand)]
        command: AutostartCommand,
    },
}

#[derive(Subcommand, Debug)]
enum CcSwitchCommand {
    /// Show the current provider/common-config/MCP state
    Inspect,
    /// Publish the protected overlay into CC Switch's database
    Adopt {
        #[arg(long, value_name = "FILE")]
        db: Option<PathBuf>,
        /// Write changes (default is a dry run)
        #[arg(long)]
        apply: bool,
        /// Allow writing while CC Switch is running
        #[arg(long)]
        force: bool,
        /// Do not register protected MCP servers in CC Switch
        #[arg(long)]
        no_mcp: bool,
    },
    /// List database snapshots taken before adopt/restore
    Backups,
    /// Restore a database snapshot
    Restore {
        #[arg(long, value_name = "FILE")]
        backup: PathBuf,
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand, Debug)]
enum AutostartCommand {
    /// Install a logon daemon plus a periodic one-shot repair task
    Install {
        /// Periodic repair interval in minutes
        #[arg(long, default_value_t = 5)]
        interval_minutes: u64,
        /// Only install the periodic check (no long-running daemon)
        #[arg(long)]
        no_daemon: bool,
        /// Installation mechanism: Task Scheduler, Startup folder, or auto-fallback
        #[arg(long, value_enum, default_value_t = AutostartMethod::Auto)]
        method: AutostartMethod,
    },
    /// Remove the scheduled tasks and wrapper scripts
    Uninstall,
    /// Show whether the tasks are installed
    Status,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum AutostartMethod {
    Auto,
    Task,
    Startup,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    // Gi   tHub@OxygenAILab  | OxygenAILab @   StarsailsClover
    match run(cli) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    let paths = Paths::resolve(cli.codex_home, cli.store)?;
    let store = Store::open(paths)?;
    let json = cli.json;

    match cli.command {
        Command::Init { from, force } => {
            let report = store
                .init(from.as_deref(), force)
                .context("cannot initialize the store")?;
            print_value(json, &report, render::init_text(&report))?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Capture { from, note } => {
            let report = store
                .capture(from.as_deref(), note)
                .context("cannot capture the baseline")?;
            print_value(json, &report, render::capture_text(&report))?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Status { check } => {
            let report = store.status().context("cannot read status")?;
            print_value(json, &report, render::status_text(&report))?;
            let drift = report
                .diff
                .as_ref()
                .is_some_and(|diff| diff.has_protected_drift())
                || report
                    .classification
                    .as_ref()
                    .is_some_and(|classification| {
                        classification.kind == keepmyconfig_core::classify::ChangeKind::Clobber
                    });
            if check && drift {
                Ok(ExitCode::from(2))
            } else {
                Ok(ExitCode::SUCCESS)
            }
        }
        Command::Diff { limit, check } => {
            let report = store.status().context("cannot read diff")?;
            if json {
                print_json(&report)?;
            } else {
                print!("{}", render::diff_text(&report, limit));
            }
            let drift = report
                .diff
                .as_ref()
                .is_some_and(|diff| diff.has_protected_drift());
            if check && drift {
                Ok(ExitCode::from(2))
            } else {
                Ok(ExitCode::SUCCESS)
            }
        }
        Command::Repair {
            dry_run,
            check,
            prefer_live,
            prefer_overlay,
        } => {
            if prefer_live && prefer_overlay {
                anyhow::bail!("--prefer-live and --prefer-overlay are mutually exclusive");
            }
            let options = RepairOptions {
                dry_run,
                // GitHub@OxygenAILab | Ox   ygen A   ILab@StarsailsClover
                check_only: check,
                mode: if prefer_live {
                    Some(keepmyconfig_core::MergeMode::LiveWins)
                } else if prefer_overlay {
                    Some(keepmyconfig_core::MergeMode::OverlayWins)
                } else {
                    None
                },
                manual: true,
            };
            let report = store.repair(&options).context("cannot repair the config")?;
            print_value(json, &report, render::repair_text(&report))?;
            if check
                && report.classification.kind == keepmyconfig_core::classify::ChangeKind::Clobber
            {
                Ok(ExitCode::from(2))
            } else {
                Ok(ExitCode::SUCCESS)
            }
        }
        Command::Watch {
            once,
            dry_run,
            poll,
            debounce,
            quiet,
        } => {
            let options = WatchOptions {
                once,
                dry_run,
                debounce: parse_duration(&debounce)?,
                poll: parse_duration(&poll)?,
            };
            let mut printed_started = false;
            let report = watch::run(&store, &options, |event| {
                if quiet || json {
                    // GitHub@OxygenAILab | OxygenAILab@Stars   ailsClov   er
                    return;
                }
                if !printed_started {
                    if !options.once {
                        eprintln!("Press Ctrl+C to stop.");
                    }
                    printed_started = true;
                }
                println!("{}", render::watch_text(event));
            })
            .context("watch loop failed")?;
            if json {
                print_json(&report)?;
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Backup {
            assets,
            link,
            include_cache,
        } => {
            let options = BackupOptions {
                classes: assets,
                link,
                include_cache,
            };
            let report = assets::backup_assets(&store, &options).context("asset backup failed")?;
            print_value(json, &report, render::backup_text(&report))?;
            Ok(ExitCode::SUCCESS)
        }
        Command::RestoreAssets {
            from,
            overwrite,
            dry_run,
            include_backup_only,
        } => {
            let options = RestoreOptions {
                from,
                overwrite,
                dry_run,
                include_backup_only,
            };
            let report =
                assets::restore_assets(&store, &options).context("asset restore failed")?;
            print_value(json, &report, render::restore_text(&report))?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Ccswitch { command } => run_ccswitch(&store, command, json),
        Command::Doctor => run_doctor(&store, json),
        Command::Autostart { command } => run_autostart(&store, command, json),
    }
}

fn run_autostart(store: &Store, command: AutostartCommand, json: bool) -> Result<ExitCode> {
    match command {
        AutostartCommand::Install {
            interval_minutes,
            no_daemon,
            method,
        } => {
            let method = match method {
                AutostartMethod::Auto => autostart::InstallMethod::Auto,
                AutostartMethod::Task => autostart::InstallMethod::Task,
                AutostartMethod::Startup => autostart::InstallMethod::Startup,
            };
            let report = autostart::install(store, interval_minutes, no_daemon, method)
                .context("cannot install autostart tasks")?;
            print_value(json, &report, render::autostart_text(&report))?;
        }
        AutostartCommand::Uninstall => {
            let report = autostart::uninstall(store).context("cannot remove autostart tasks")?;
            print_value(json, &report, render::autostart_text(&report))?;
        }
        AutostartCommand::Status => {
            let status = autostart::status(store).context("cannot read autostart status")?;
            print_value(json, &status, render::autostart_status_text(&status))?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn run_ccswitch(store: &Store, command: CcSwitchCommand, json: bool) -> Result<ExitCode> {
    match command {
        CcSwitchCommand::Inspect => {
            match ccswitch::summary().context("cannot inspect CC Switch")? {
                Some(summary) => {
                    if json {
                        print_json(&summary)?;
                    } else {
                        print!("{}", render::ccswitch_text(&summary));
                        println!(
                        "Run `keepmyconfig ccswitch adopt` to preview publishing the protected overlay."
                    );
                    }
                }
                None => {
                    println!("CC Switch database not found (~/.cc-switch/cc-switch.db).");
                }
            }
        }
        CcSwitchCommand::Adopt {
            db,
            apply,
            force,
            no_mcp,
        } => {
            let options = AdoptOptions {
                db_path: db,
                apply,
                force,
                include_mcp: !no_mcp,
                // GitH   ub@OxygenAILab | OxygenAILab@StarsailsCl  over
            };
            let report = ccswitch::adopt(store, &options).context("CC Switch adopt failed")?;
            print_value(json, &report, render::adopt_text(&report))?;
        }
        CcSwitchCommand::Backups => {
            let backups = ccswitch::list_database_backups(store)?;
            if json {
                print_json(&backups)?;
            } else if backups.is_empty() {
                println!("No CC Switch database backups found.");
            } else {
                for backup in backups {
                    println!("{}", backup.display());
                }
            }
        }
        CcSwitchCommand::Restore { backup, force } => {
            let report = ccswitch::restore_database(store, &backup, force)
                .context("CC Switch restore failed")?;
            print_value(json, &report, render::restore_db_text(&report))?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

#[derive(Serialize)]
struct DoctorReport {
    version: String,
    codex_home: String,
    store_dir: String,
    checks: Vec<DoctorCheck>,
}

#[derive(Serialize)]
struct DoctorCheck {
    name: String,
    status: String,
    detail: String,
}

fn run_doctor(store: &Store, json: bool) -> Result<ExitCode> {
    let status = store.status().context("cannot read status")?;
    let mut checks = Vec::new();
    checks.push(check(
        "codex_home",
        status.codex_home.is_dir(),
        format!("{}", status.codex_home.display()),
    ));
    checks.push(check(
        "config_toml",
        status.live.exists && status.live.parsed,
        match (&status.live.parse_error, status.live.exists) {
            (Some(error), _) => error.clone(),
            (None, true) => "present and parseable".to_string(),
            (None, false) => "missing".to_string(),
        },
        // GitHub@OxygenAILab  | OxygenAILab@StarsailsClover
    ));
    checks.push(check(
        "store",
        status.initialized,
        if status.initialized {
            format!(
                "{} protected path(s) across {} capture(s)",
                status.overlay.protected_paths, status.baseline.captures
            )
        } else {
            "run `keepmyconfig init`".to_string()
        },
    ));
    let clobber = status
        .classification
        .as_ref()
        .is_some_and(|classification| {
            classification.kind == keepmyconfig_core::classify::ChangeKind::Clobber
        });
    checks.push(check(
        "protection",
        !clobber,
        if clobber {
            // GitHub@OxygenAILab | OxygenAI   Lab@Starsai  lsClove   r
            "protected drift detected; run `keepmyconfig repair`".to_string()
        } else {
            "no clobber fingerprint".to_string()
        },
    ));
    match ccswitch::summary() {
        Ok(Some(summary)) => {
            let healthy = summary.codex_providers == 0
                || summary.providers_with_common_config == summary.codex_providers;
            checks.push(check_status(
                "cc_switch",
                if healthy { "ok" } else { "warn" },
                format!(
                    "schema {} · {} codex provider(s) · {} with common config · {} codex MCP(s)",
                    summary.schema_version,
                    summary.codex_providers,
                    summary.providers_with_common_config,
                    summary.mcp_codex_enabled
                ),
            ));
        }
        Ok(None) => checks.push(check(
            "cc_switch",
            true,
            "not installed (optional)".to_string(),
        )),
        Err(error) => checks.push(check("cc_switch", false, error.to_string())),
    }
    match autostart::status(store) {
        Ok(status) if !status.supported => checks.push(check_status(
            "autostart",
            "warn",
            "not supported on this platform (use a systemd user unit on Linux)".to_string(),
        )),
        Ok(status) => {
            let installed = status.watch_task_installed
                || status.check_task_installed
                || status.startup_script_installed;
            checks.push(check_status(
                "autostart",
                if installed { "ok" } else { "warn" },
                format!(
                    "watch task: {}, periodic check: {}, startup script: {}",
                    installed_label(status.watch_task_installed),
                    installed_label(status.check_task_installed),
                    installed_label(status.startup_script_installed)
                ),
            ));
        }
        Err(error) => checks.push(check_status("autostart", "warn", error.to_string())),
    }
    let failed = checks.iter().any(|entry| entry.status == "fail");
    let report = DoctorReport {
        version: status.version.clone(),
        codex_home: status.codex_home.display().to_string(),
        store_dir: status.store_dir.display().to_string(),
        checks,
    };
    if json {
        print_json(&report)?;
    } else {
        println!("KeepMyConfig v{}", report.version);
        println!("Codex home: {}", report.codex_home);
        println!("Store     : {}", report.store_dir);
        for entry in &report.checks {
            println!(
                "  [{}] {}: {}",
                match entry.status.as_str() {
                    "ok" => "ok",
                    "warn" => "warn",
                    _ => "fail",
                },
                entry.name,
                entry.detail
            );
        }
    }
    if failed {
        Ok(ExitCode::from(1))
    // GitHub@OxygenAI Lab | OxygenAILab@S   tarsa ilsClover
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

fn check(name: &str, ok: bool, detail: String) -> DoctorCheck {
    check_status(name, if ok { "ok" } else { "fail" }, detail)
}

fn installed_label(value: bool) -> &'static str {
    if value {
        "installed"
    } else {
        "not installed"
    }
}

fn check_status(name: &str, status: &str, detail: String) -> DoctorCheck {
    DoctorCheck {
        name: name.to_string(),
        status: status.to_string(),
        detail,
    }
}

fn print_value<T: Serialize>(json: bool, value: &T, human: String) -> Result<()> {
    if json {
        print_json(value)
    } else {
        print!("{human}");
        Ok(())
    }
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn parse_duration(text: &str) -> Result<Duration> {
    // GitHub@Oxyge   nAILab |  OxygenAILab@St   ars ailsClover
    let text = text.trim();
    let error = || anyhow::anyhow!("invalid duration '{text}': use values like 800ms, 30s, or 2m");
    if let Some(value) = text.strip_suffix("ms") {
        let millis: u64 = value.trim().parse().map_err(|_| error())?;
        return Ok(Duration::from_millis(millis));
    }
    if let Some(value) = text.strip_suffix('s') {
        let seconds: u64 = value.trim().parse().map_err(|_| error())?;
        return Ok(Duration::from_secs(seconds));
    }
    if let Some(value) = text.strip_suffix('m') {
        let minutes: u64 = value.trim().parse().map_err(|_| error())?;
        return Ok(Duration::from_secs(minutes * 60));
    }
    let seconds: u64 = text.parse().map_err(|_| error())?;
    Ok(Duration::from_secs(seconds))
}
