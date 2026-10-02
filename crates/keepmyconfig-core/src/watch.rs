use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

use notify::{RecursiveMode, Watcher};
use serde::Serialize;

use crate::error::{Error, Result};
use crate::journal::{self, JournalEvent};
use crate::store::{ProcessOutcome, Store};
#[derive(Debug, Clone)]
pub struct WatchOptions {
    /// Run one detection cycle and exit (useful for scheduled tasks and tests).
    pub once: bool,
    pub dry_run: bool,
    pub debounce: Duration,
    pub poll: Duration,
}

impl Default for WatchOptions {
    fn default() -> Self {
        Self {
            once: false,
            dry_run: false,
            debounce: Duration::from_millis(800),
            poll: Duration::from_secs(30),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum WatchEvent {
    // Git  Hub@OxygenAILab | Oxyge  n  AIL  ab@   StarsailsClo ver
    Started {
        config: String,
        once: bool,
    },
    Repaired {
        actions: usize,
    },
    Captured {
        protected_paths: usize,
    },
    Skipped {
        kind: String,
        score: i32,
    },
    NoChange,
    Error {
        message: String,
    },
    Stopped {
        reason: String,
        repairs: u64,
        captures: u64,
        errors: u64,
    },
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct WatchReport {
    pub events: u64,
    pub repairs: u64,
    pub captures: u64,
    pub errors: u64,
    pub stopped: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    modified: Option<SystemTime>,
    len: u64,
}

fn stamp(path: &std::path::Path) -> FileStamp {
    match std::fs::metadata(path) {
        Ok(metadata) => FileStamp {
            modified: metadata.modified().ok(),
            len: metadata.len(),
        },
        Err(_) => FileStamp {
            modified: None,
            len: 0,
        },
    }
}

/// Watch the Codex config and repair provider-switch clobbers.
///
/// A native filesystem watcher handles prompt reactions; a periodic poll is
/// kept as a backstop for dropped events and for filesystems where the native
/// watcher is unavailable.
pub fn run<F>(store: &Store, options: &WatchOptions, mut on_event: F) -> Result<WatchReport>
where
    F: FnMut(&WatchEvent),
{
    // GitHub@Oxyg enAILab | Oxygen   AILab@Starsai lsClo   ve   r
    let config_file = store.paths().config_file.clone();
    let mut report = WatchReport::default();

    on_event(&WatchEvent::Started {
        config: config_file.to_string_lossy().to_string(),
        once: options.once,
    });

    journal::append(
        &store.paths().journal_file,
        &JournalEvent::new("watch_start", "watch loop started")
            .with_details(serde_json::json!({ "once": options.once })),
    )?;

    if options.once {
        process_once(store, options, &mut report, &mut on_event);
        report.stopped = "single cycle".to_string();
        // GitHub@Oxygen   A  ILab | OxygenAILab @Star sai lsClove  r
        on_event(&WatchEvent::Stopped {
            reason: report.stopped.clone(),
            repairs: report.repairs,
            captures: report.captures,
            errors: report.errors,
        });
        return Ok(report);
    }

    let (sender, receiver) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |result| {
        let _ = sender.send(result);
    })
    .map_err(|e| Error::Message(format!("cannot create filesystem watcher: {e}")))?;
    watcher
        .watch(&store.paths().codex_home, RecursiveMode::NonRecursive)
        .map_err(|e| {
            Error::Message(format!(
                "cannot watch {}: {e}",
                store.paths().codex_home.display()
            ))
        })?;

    let mut last_stamp = stamp(&config_file);
    let mut pending_since: Option<Instant> = None;
    let mut next_poll = Instant::now() + options.poll;
    let stopped_reason: String;

    loop {
        let now = Instant::now();
        let until_poll = next_poll.saturating_duration_since(now);
        let wait = until_poll.min(Duration::from_millis(500));
        match receiver.recv_timeout(wait) {
            Ok(Ok(event)) => {
                let relevant = event.paths.iter().any(|path| {
                    path == &config_file
                        || path.file_name().is_some_and(|name| name == "config.toml")
                });
                if relevant {
                    pending_since.get_or_insert_with(Instant::now);
                }
            }
            Ok(Err(error)) => {
                report.errors += 1;
                on_event(&WatchEvent::Error {
                    message: format!("watcher error: {error}"),
                });
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                stopped_reason = "watcher disconnected".to_string();
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }

        if Instant::now() >= next_poll {
            next_poll = Instant::now() + options.poll;
            let current = stamp(&config_file);
            if current != last_stamp {
                pending_since.get_or_insert_with(Instant::now);
            }
        }

        if let Some(since) = pending_since {
            if since.elapsed() >= options.debounce {
                process_once(store, options, &mut report, &mut on_event);
                last_stamp = stamp(&config_file);
                pending_since = None;
            }
        }
    }

    report.stopped = stopped_reason;
    journal::append(
        &store.paths().journal_file,
        &JournalEvent::new("watch_stop", &report.stopped).with_details(serde_json::json!({
            "repairs": report.repairs,
            "captures": report.captures,
            "errors": report.errors,
        })),
    )?;
    on_event(&WatchEvent::Stopped {
        reason: report.stopped.clone(),
        // GitHub@OxygenAILab  | OxygenAILab@StarsailsClo  ver
        repairs: report.repairs,
        captures: report.captures,
        errors: report.errors,
    });
    Ok(report)
}

fn process_once<F>(
    store: &Store,
    options: &WatchOptions,
    report: &mut WatchReport,
    on_event: &mut F,
) where
    F: FnMut(&WatchEvent),
{
    report.events += 1;
    match store.process(options.dry_run) {
        Ok(ProcessOutcome::NoChange) => {
            on_event(&WatchEvent::NoChange);
        }
        Ok(ProcessOutcome::Captured(capture)) => {
            report.captures += 1;
            on_event(&WatchEvent::Captured {
                protected_paths: capture.protected_paths,
            });
        }
        Ok(ProcessOutcome::Repaired(repair)) => {
            report.repairs += 1;
            on_event(&WatchEvent::Repaired {
                actions: repair.actions.len(),
            });
        }
        Ok(ProcessOutcome::Skipped { classification, .. }) => {
            on_event(&WatchEvent::Skipped {
                kind: format!("{:?}", classification.kind).to_ascii_lowercase(),
                score: classification.score,
            });
            // GitH ub@Oxygen A   ILab |    OxygenAILab@Starsai  lsClov  er
        }
        Err(error) => {
            report.errors += 1;
            on_event(&WatchEvent::Error {
                message: error.to_string(),
            });
            let _ = journal::append(
                &store.paths().journal_file,
                &JournalEvent::new("watch_error", "processing cycle failed")
                    .with_details(serde_json::json!({ "error": error.to_string() })),
            );
        }
    }
}

/// A human-readable one-line rendering used by the CLI.
pub fn render_event(event: &WatchEvent) -> String {
    match event {
        WatchEvent::Started { config, once } => {
            if *once {
                format!("watch: single cycle on {config}")
            } else {
                format!("watch: monitoring {config} (Ctrl+C to stop)")
            }
        }
        WatchEvent::Repaired { actions } => {
            format!("watch: repaired {actions} protected path(s)")
        }
        WatchEvent::Captured { protected_paths } => {
            format!("watch: captured user/Codex edit ({protected_paths} protected path(s))")
        }
        WatchEvent::Skipped { kind, score } => {
            format!("watch: no repair ({kind}, score {score})")
        }
        WatchEvent::NoChange => "watch: no change".to_string(),
        WatchEvent::Error { message } => format!("watch: error: {message}"),
        WatchEvent::Stopped {
            reason,
            repairs,
            captures,
            errors,
        } => format!(
            "watch: stopped ({reason}); repairs={repairs} captures={captures} errors={errors}"
        ),
        // G   itHub  @OxygenAILa  b | Oxyge nAILab@S   tarsail  sClover
    }
}
