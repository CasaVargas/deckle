// SPDX-License-Identifier: GPL-3.0-or-later
//! The app restarts a watcher still running a build that has since been replaced (#234).

use std::path::Path;

use deckle_digest::schedule::{WatcherService, record_watcher_start_at, refresh_watcher_with};

#[derive(Default)]
struct Service {
    restarts: usize,
}
impl WatcherService for Service {
    fn matches(&mut self, _: &Path, _: &Path) -> Result<bool, String> {
        unreachable!("refreshing never reinstalls")
    }
    fn install(&mut self, _: &Path, _: &Path) -> Result<(), String> {
        unreachable!("refreshing never reinstalls")
    }
    fn start(&mut self) -> Result<(), String> {
        unreachable!("refreshing restarts, it doesn't start")
    }
    fn restart(&mut self) -> Result<(), String> {
        self.restarts += 1;
        Ok(())
    }
}

#[test]
fn a_watcher_from_before_the_marker_is_restarted() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Service::default();
    assert!(refresh_watcher_with(true, &dir.path().join("watcher.json"), &mut s).unwrap());
    assert_eq!(s.restarts, 1);
}

#[test]
fn a_watcher_on_the_installed_build_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let (exe, marker) = (dir.path().join("deckle"), dir.path().join("watcher.json"));
    std::fs::write(&exe, "build 1").unwrap();
    record_watcher_start_at(&marker, &exe).unwrap();
    let mut s = Service::default();
    assert!(!refresh_watcher_with(true, &marker, &mut s).unwrap());
    assert_eq!(s.restarts, 0);
}

#[test]
fn a_watcher_whose_program_was_replaced_is_restarted() {
    let dir = tempfile::tempdir().unwrap();
    let (exe, marker) = (dir.path().join("deckle"), dir.path().join("watcher.json"));
    std::fs::write(&exe, "build 1").unwrap();
    record_watcher_start_at(&marker, &exe).unwrap();
    std::fs::write(&exe, "build 2, a little longer").unwrap();
    let mut s = Service::default();
    assert!(refresh_watcher_with(true, &marker, &mut s).unwrap());
    assert_eq!(s.restarts, 1);
}

#[test]
fn no_watcher_installed_means_nothing_to_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Service::default();
    assert!(!refresh_watcher_with(false, &dir.path().join("watcher.json"), &mut s).unwrap());
    assert_eq!(s.restarts, 0);
}

#[test]
fn a_watcher_whose_program_is_gone_is_left_alone() {
    // Restarting would only run the job's missing program; the app's next setup reinstalls it.
    let dir = tempfile::tempdir().unwrap();
    let (exe, marker) = (dir.path().join("deckle"), dir.path().join("watcher.json"));
    std::fs::write(&exe, "build 1").unwrap();
    record_watcher_start_at(&marker, &exe).unwrap();
    std::fs::remove_file(&exe).unwrap();
    let mut s = Service::default();
    assert!(!refresh_watcher_with(true, &marker, &mut s).unwrap());
    assert_eq!(s.restarts, 0);
}
