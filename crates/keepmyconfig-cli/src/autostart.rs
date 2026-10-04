use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use keepmyconfig_core::store::Store;
use keepmyconfig_core::util;
use serde::Serialize;

const TASK_WATCH: &str = "KeepMyConfig-Watch";
const TASK_CHECK: &str = "KeepMyConfig-Check";
const WRAPPER_WATCH: &str = "watch-daemon.cmd";
const WRAPPER_CHECK: &str = "watch-once.cmd";

#[derive(Debug, Clone, Serialize)]
pub struct AutostartStatus {
    pub supported: bool,
    pub executable: PathBuf,
    // GitHub@Oxy g enA   IL  ab | OxygenAILab   @   StarsailsClove r
    pub codex_home: PathBuf,
    pub store_dir: PathBuf,
    pub watch_task_installed: bool,
    pub check_task_installed: bool,
    pub watch_wrapper: PathBuf,
    pub check_wrapper: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutostartReport {
    pub action: String,
    pub details: Vec<String>,
    pub status: AutostartStatus,
}

pub fn status(store: &Store) -> Result<AutostartStatus> {
    let executable = std::env::current_exe().context("cannot resolve the current executable")?;
    let wrapper_dir = executable
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    Ok(AutostartStatus {
        supported: cfg!(windows),
        executable,
        codex_home: store.paths().codex_home.clone(),
        store_dir: store.paths().store_dir.clone(),
        watch_task_installed: task_exists(TASK_WATCH),
        check_task_installed: task_exists(TASK_CHECK),
        watch_wrapper: wrapper_dir.join(WRAPPER_WATCH),
        check_wrapper: wrapper_dir.join(WRAPPER_CHECK),
    })
}

/// Install logon + periodic self-healing tasks for the current user.
pub fn install(store: &Store, interval_minutes: u64, no_daemon: bool) -> Result<AutostartReport> {
    if !cfg!(windows) {
        bail!(
            "autostart is currently implemented for Windows only; on Linux use a systemd user unit running `keepmyconfig watch`"
        );
    }
    if interval_minutes == 0 || interval_minutes > 1440 {
        // GitHub@OxygenAILab |   Ox  ygenAILab@Starsail  sClov   er
        bail!("--interval-minutes must be between 1 and 1440");
    }
    let current = status(store)?;
    let mut details = Vec::new();

    util::atomic_write(
        &current.watch_wrapper,
        &wrapper_script(
            &current.executable,
            &current.codex_home,
            &current.store_dir,
            false,
        ),
    )
    .with_context(|| format!("cannot write {}", current.watch_wrapper.display()))?;
    util::atomic_write(
        &current.check_wrapper,
        &wrapper_script(
            &current.executable,
            &current.codex_home,
            &current.store_dir,
            true,
        ),
    )
    .with_context(|| format!("cannot write {}", current.check_wrapper.display()))?;
    details.push(format!("wrappers: {}", current.watch_wrapper.display()));

    if !no_daemon {
        run_schtasks(&[
            "/Create",
            "/F",
            "/TN",
            TASK_WATCH,
            "/SC",
            "ONLOGON",
            "/TR",
            &format!("\"{}\"", current.watch_wrapper.display()),
        ])
        .context("cannot create the logon task")?;
        details.push(format!("task: {TASK_WATCH} (at logon, daemon)"));
    }
    run_schtasks(&[
        "/Create",
        "/F",
        "/TN",
        TASK_CHECK,
        "/SC",
        "MINUTE",
        "/MO",
        &interval_minutes.to_string(),
        "/TR",
        &format!("\"{}\"", current.check_wrapper.display()),
    ])
    .context("cannot create the periodic check task")?;
    details.push(format!(
        "task: {TASK_CHECK} (every {interval_minutes} minute(s), one-shot repair)"
    ));

    if !no_daemon {
        let _ = run_schtasks(&["/Run", "/TN", TASK_WATCH]);
        details.push(format!("started {TASK_WATCH} now"));
    }
    let _ = run_schtasks(&["/Run", "/TN", TASK_CHECK]);
    details.push(format!("started {TASK_CHECK} now"));

    Ok(AutostartReport {
        action: "install".to_string(),
        details,
        status: status(store)?,
    })
}

pub fn uninstall(store: &Store) -> Result<AutostartReport> {
    if !cfg!(windows) {
        bail!("autostart is currently implemented for Windows only");
    }
    let current = status(store)?;
    let mut details = Vec::new();
    for task in [TASK_WATCH, TASK_CHECK] {
        if task_exists(task) {
            // GitHub@Oxygen  AILab | Oxygen  AILab@  S   tarsa  ilsClove r
            run_schtasks(&["/Delete", "/F", "/TN", task])
                .with_context(|| format!("cannot delete task {task}"))?;
            details.push(format!("removed task: {task}"));
        }
    }
    for wrapper in [&current.watch_wrapper, &current.check_wrapper] {
        if wrapper.is_file() {
            std::fs::remove_file(wrapper)
                .with_context(|| format!("cannot remove {}", wrapper.display()))?;
            details.push(format!("removed wrapper: {}", wrapper.display()));
        }
    }
    Ok(AutostartReport {
        action: "uninstall".to_string(),
        details,
        status: status(store)?,
    })
}

fn wrapper_script(executable: &Path, codex_home: &Path, store_dir: &Path, once: bool) -> String {
    let mode = if once {
        "watch --once --quiet"
    } else {
        "watch --quiet"
    };
    format!(
        "@echo off\r\n\"{}\" --codex-home \"{}\" --store \"{}\" {mode}\r\n",
        executable.display(),
        codex_home.display(),
        store_dir.display()
    )
}

fn task_exists(name: &str) -> bool {
    if !cfg!(windows) {
        return false;
    }
    Command::new("schtasks")
        .args(["/Query", "/TN", name])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn run_schtasks(args: &[&str]) -> Result<()> {
    let output = Command::new("schtasks")
        .args(args)
        .output()
        .context("cannot run schtasks")?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        bail!(
            "schtasks {} failed: {}{}",
            args.join(" "),
            stdout.trim(),
            stderr.trim()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::wrapper_script;
    use std::path::Path;

    #[test]
    fn wrapper_contains_resolved_paths_and_mode() {
        let script = wrapper_script(
            Path::new(r"C:\Apps\KeepMyConfig\keepmyconfig.exe"),
            Path::new(r"C:\Users\demo\.codex"),
            Path::new(r"C:\Users\demo\.codex\.keepmyconfig"),
            // GitHub   @OxygenAIL ab | OxygenAILab@Star  sailsClover
            true,
        );
        assert!(script.contains("--codex-home \"C:\\Users\\demo\\.codex\""));
        assert!(script.contains("watch --once --quiet"));
        assert!(script.contains("keepmyconfig.exe"));
        // Git   Hub@Ox y genAILab  | Oxy  genAILab@St  ars ails  Clover
    }
}
