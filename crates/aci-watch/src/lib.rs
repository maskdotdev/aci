//! Filesystem watch helpers for incremental indexing.
//!
//! The watcher collects file-system events until the stream is quiet for a
//! debounce interval, then returns a stable set of changed paths for the CLI to
//! pass into incremental reindex planning.

use aci_core::Result;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

/// Root path and debounce interval for a watch pass.
#[derive(Clone, Debug)]
pub struct WatchOptions {
    pub root: PathBuf,
    pub debounce: Duration,
}

impl WatchOptions {
    /// Creates watch options with the default debounce interval.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            debounce: Duration::from_millis(150),
        }
    }
}

/// Deduplicated changed paths observed during a watch pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoalescedChanges {
    pub paths: Vec<PathBuf>,
}

/// Deduplicates paths from notify events.
pub fn coalesce_events(events: &[Event]) -> CoalescedChanges {
    let paths = events
        .iter()
        .flat_map(|event| event.paths.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    CoalescedChanges { paths }
}

/// Watches until the debounce interval is quiet or `max_wait` is reached.
pub fn watch_until_quiet(options: WatchOptions, max_wait: Duration) -> Result<CoalescedChanges> {
    let (tx, rx) = mpsc::channel();
    let mut watcher: RecommendedWatcher = notify::recommended_watcher(move |event| {
        let _ = tx.send(event);
    })
    .map_err(|error| aci_core::AciError::Message(error.to_string()))?;
    watcher
        .watch(Path::new(&options.root), RecursiveMode::Recursive)
        .map_err(|error| aci_core::AciError::Message(error.to_string()))?;

    let deadline = Instant::now() + max_wait;
    let mut last_event_at = None;
    let mut events = Vec::new();
    loop {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        if last_event_at.is_some_and(|last| now.duration_since(last) >= options.debounce) {
            break;
        }
        let timeout = options
            .debounce
            .min(deadline.saturating_duration_since(now));
        match rx.recv_timeout(timeout) {
            Ok(Ok(event)) => {
                last_event_at = Some(Instant::now());
                events.push(event);
            }
            Ok(Err(error)) => return Err(aci_core::AciError::Message(error.to_string())),
            Err(RecvTimeoutError::Timeout) if !events.is_empty() => break,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    Ok(coalesce_events(&events))
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::{EventKind, event::ModifyKind};
    use std::fs;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::thread;

    #[test]
    fn coalesces_duplicate_event_paths() {
        let path = PathBuf::from("src/lib.rs");
        let event = Event {
            kind: EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Content)),
            paths: vec![path.clone(), path.clone()],
            attrs: Default::default(),
        };
        assert_eq!(coalesce_events(&[event]).paths, vec![path]);
    }

    #[test]
    fn watch_until_quiet_observes_file_change() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("changed.py");
        let writer_target = target.clone();
        let done = Arc::new(AtomicBool::new(false));
        let writer_done = Arc::clone(&done);
        let writer = thread::spawn(move || {
            thread::sleep(Duration::from_millis(250));
            for attempt in 0..20 {
                if writer_done.load(Ordering::Relaxed) {
                    return;
                }
                fs::write(
                    &writer_target,
                    format!("def changed():\n    return {attempt}\n"),
                )
                .expect("write changed file");
                thread::sleep(Duration::from_millis(100));
            }
        });

        let changes = watch_until_quiet(WatchOptions::new(dir.path()), Duration::from_secs(3))
            .expect("watch changes");
        done.store(true, Ordering::Relaxed);
        writer.join().expect("writer thread");
        assert!(
            changes
                .paths
                .iter()
                .any(|path| path.ends_with("changed.py") || path == dir.path())
        );
    }
}
