// SPDX-License-Identifier: GPL-3.0-or-later
//! Fresh-paper requests in a daemon install, where the daemon already runs `digest watch`.

use std::path::PathBuf;

use deckle_digest::Config;
use deckle_digest::config::{ExportConfig, ExportFormat};

#[test]
fn press_setup_in_daemon_install_skips_watcher_install() {
    let tmp = tempfile::tempdir().unwrap();
    // nextest runs each test in its own process, so the environment is this test's alone.
    // Dry-run makes any attempt to install the watcher an error.
    unsafe {
        std::env::set_var("HOME", tmp.path());
        std::env::set_var("APPDATA", tmp.path());
        std::env::set_var("XDG_CONFIG_HOME", tmp.path());
        std::env::set_var("DECKLE_DAEMON", "1");
        std::env::set_var("DECKLE_SCHEDULE_DRY_RUN", "1");
    }
    let main = deckle_digest::config::default_path();
    std::fs::create_dir_all(main.parent().unwrap()).unwrap();
    let folder = tmp.path().join("shared");
    std::fs::create_dir(&folder).unwrap();
    let cfg = Config {
        export: vec![ExportConfig {
            name: "Paper".into(),
            dir: folder.display().to_string(),
            format: ExportFormat::Bundle,
            ..Default::default()
        }],
        ..Default::default()
    };
    deckle_digest::config::save(&main, &cfg).unwrap();

    let status =
        deckle_app::press::press_setup(None, 0, true, None, &PathBuf::from("/opt/deckle/deckle"))
            .unwrap();
    assert!(status.enabled && status.watcher_ready, "{:?}", status.error);
    assert!(status.error.is_none());
}
