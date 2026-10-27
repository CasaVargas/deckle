// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_cli::press_worker::{PressSupervisor, WorkerOptions};
use deckle_digest::{Config, chrono::Utc};
use std::{
    path::Path,
    process::{Child, Command},
    sync::Arc,
    time::{Duration, Instant},
};
fn launch(job: &Path) -> std::io::Result<Child> {
    Command::new(std::env::current_exe()?)
        .args(["--ignored", "--exact", "press_worker::worker_fixture"])
        .env("DECKLE_TEST_JOB", job)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
}
#[test]
#[ignore]
fn worker_fixture() {
    let Some(job) = std::env::var_os("DECKLE_TEST_JOB") else {
        return;
    };
    let data: serde_json::Value = serde_json::from_slice(&std::fs::read(&job).unwrap()).unwrap();
    match data["config"]["title"].as_str() {
        Some("TEST HANG") => std::thread::sleep(Duration::from_secs(120)),
        Some("TEST CRASH") => std::process::exit(9),
        Some("TEST DELAY") => std::thread::sleep(Duration::from_millis(400)),
        _ => (),
    }
    let count = std::sync::atomic::AtomicUsize::new(0);
    deckle_cli::press_worker::run_job_with_net(Path::new(&job), &|url: &str| -> Result<Vec<u8>,String> {
        count.fetch_add(1,std::sync::atomic::Ordering::SeqCst);
        if url=="https://example.invalid/feed" {Ok(br#"<rss version="2.0"><channel><title>Fake wire</title><item><title>Library reopened</title><link>https://example.invalid/story</link><description>Repairs are complete.</description></item></channel></rss>"#.to_vec())}else{Err("fake source unavailable".into())}
    }).unwrap();
    std::fs::write(
        Path::new(&job).with_file_name("source-count"),
        count.load(std::sync::atomic::Ordering::SeqCst).to_string(),
    )
    .unwrap();
}
#[test]
fn press_worker_legacy_tick_creates_no_state_and_does_not_block() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("press");
    let mut worker = PressSupervisor::with_options(WorkerOptions {
        root: root.clone(),
        run_lock: dir.path().join("run.lock"),
        config_path: dir.path().join("digest.toml"),
        launch: Arc::new(launch),
        clock: Arc::new(Utc::now),
    });
    let start = Instant::now();
    for _ in 0..100 {
        worker.tick(&[Config::default()], Utc::now()).unwrap();
    }
    assert!(start.elapsed() < Duration::from_secs(1));
    drop(worker);
    assert!(!root.exists());
}
#[test]
fn press_worker_hidden_dispatch_rejects_missing_descriptor_without_loading_config() {
    assert_cmd::cargo::cargo_bin_cmd!("deckle")
        .args([
            "digest",
            "press-worker",
            "--job",
            "/does-not-exist/job.json",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("worker"));
    let output = assert_cmd::cargo::cargo_bin_cmd!("deckle")
        .args(["digest", "--help"])
        .output()
        .unwrap();
    assert!(
        !String::from_utf8(output.stdout)
            .unwrap()
            .contains("press-worker")
    );
}

use deckle_digest::config::{ExportConfig, ExportFormat};
use deckle_digest::press::{
    self, PressBinding, PressOperation, PressRequest, PressState, ProducerManifest, Uuid,
};
struct Fixture {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    cfg: Config,
    binding: PressBinding,
    request: PressRequest,
    options: WorkerOptions,
}
fn fixture(title: &str) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().canonicalize().unwrap();
    let root = base.join("private");
    std::fs::create_dir(&root).unwrap();
    let id = press::setup::producer_identity(&root.join("producer.json")).unwrap();
    let export = base.join("shared");
    std::fs::create_dir(&export).unwrap();
    let binding = PressBinding {
        root: export.clone(),
        producer_id: id,
        paper_id: Uuid::new_v4(),
    };
    let mut cfg = Config {
        title: title.into(),
        paper_id: Some(binding.paper_id),
        sections: vec![deckle_digest::SectionKind::Feeds],
        feeds: vec![deckle_digest::config::FeedConfig {
            name: "Fake wire".into(),
            url: "https://example.invalid/feed".into(),
            max_items: 1,
            ..Default::default()
        }],
        full_text: false,
        photos: false,
        export: vec![ExportConfig {
            name: "Shared".into(),
            dir: export.display().to_string(),
            format: ExportFormat::Bundle,
            press_enabled: true,
            ..Default::default()
        }],
        ..Default::default()
    };
    cfg.puzzle_path = Some(base.join("puzzles.json"));
    cfg.shelf_path = Some(base.join("shelf.json"));
    cfg.almanac_path = Some(base.join("almanac.json"));
    cfg.press_path = Some(base.join("stop.json"));
    cfg.reading.queue_path = Some(base.join("reading.jsonl"));
    let options = WorkerOptions {
        root: root.clone(),
        run_lock: base.join("run.lock"),
        config_path: base.join("digest.toml"),
        launch: Arc::new(launch),
        clock: Arc::new(Utc::now),
    };
    deckle_digest::config::save(&options.config_path, &cfg).unwrap();
    press::setup::write_manifest(
        &binding,
        &ProducerManifest {
            format: "deckle-press".into(),
            version: 1,
            producer_id: id,
            paper_id: binding.paper_id,
            paper_name: title.into(),
            enabled: true,
            operations: vec![PressOperation::FreshFull],
            updated_at: Utc::now(),
        },
    )
    .unwrap();
    let now = Utc::now();
    let request = PressRequest {
        format: "deckle-press-request".into(),
        version: 1,
        request_id: Uuid::new_v4(),
        reader_id: Uuid::new_v4(),
        producer_id: id,
        paper_id: binding.paper_id,
        operation: PressOperation::FreshFull,
        created_at: now,
        expires_at: now + deckle_digest::chrono::Duration::hours(24),
    };
    let path = export.join(format!(
        ".deckle-press/requests/{}/{}.json",
        request.reader_id, request.request_id
    ));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_vec(&request).unwrap()).unwrap();
    Fixture {
        _dir: dir,
        root,
        cfg,
        binding,
        request,
        options,
    }
}
fn status(f: &Fixture) -> Option<press::PressStatus> {
    serde_json::from_slice(
        &std::fs::read(f.binding.root.join(format!(
            ".deckle-press/status/{}.json",
            f.request.request_id
        )))
        .ok()?,
    )
    .ok()
}
fn until(f: &Fixture, w: &mut PressSupervisor, want: PressState) {
    let end = Instant::now() + Duration::from_secs(30);
    while Instant::now() < end {
        let _ = w.tick(std::slice::from_ref(&f.cfg), Utc::now());
        if status(f).is_some_and(|s| s.state == want) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("status did not reach {want:?}: {:?}", status(f));
}
#[test]
fn press_worker_real_child_publishes_and_restart_does_not_refetch() {
    let f = fixture("Process paper");
    let mut w = PressSupervisor::with_options(f.options.clone());
    until(&f, &mut w, PressState::Published);
    drop(w);
    let first = status(&f).unwrap();
    let attempt = std::fs::read_dir(f.root.join("jobs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(
        std::fs::read_to_string(attempt.join("source-count")).unwrap(),
        "1"
    );
    let attempts = std::fs::read_dir(f.root.join("jobs")).unwrap().count();
    let mut w = PressSupervisor::with_options(f.options.clone());
    for _ in 0..10 {
        let _ = w.tick(std::slice::from_ref(&f.cfg), Utc::now());
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(w);
    assert_eq!(status(&f).unwrap(), first);
    assert_eq!(
        std::fs::read_dir(f.root.join("jobs")).unwrap().count(),
        attempts
    );
}
#[test]
fn press_worker_scheduled_lock_does_not_start_deadline_and_opt_out_rejects_queued() {
    let mut f = fixture("Queued paper");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&f.options.run_lock)
        .unwrap();
    lock.lock().unwrap();
    let mut w = PressSupervisor::with_options(f.options.clone());
    until(&f, &mut w, PressState::Queued);
    assert!(status(&f).unwrap().started_at.is_none());
    f.cfg.export[0].press_enabled = false;
    drop(lock);
    until(&f, &mut w, PressState::Failed);
    assert_eq!(
        status(&f).unwrap().error_code,
        Some(press::PressErrorCode::Disabled)
    );
    assert!(!f.root.join("jobs").exists());
}
#[test]
fn press_worker_hang_keeps_ticks_fast_and_timeout_kills_and_fences() {
    let mut f = fixture("TEST HANG");
    let clock = Arc::new(std::sync::Mutex::new(Utc::now()));
    let clock2 = clock.clone();
    f.options.clock = Arc::new(move || *clock2.lock().unwrap());
    let mut w = PressSupervisor::with_options(f.options.clone());
    until(&f, &mut w, PressState::Building);
    let start = Instant::now();
    for _ in 0..1000 {
        let _ = w.tick(std::slice::from_ref(&f.cfg), Utc::now());
    }
    assert!(start.elapsed() < Duration::from_secs(1));
    *clock.lock().unwrap() += deckle_digest::chrono::Duration::minutes(16);
    until(&f, &mut w, PressState::Failed);
    assert_eq!(
        status(&f).unwrap().error_code,
        Some(press::PressErrorCode::TimedOut)
    );
    drop(w);
    let j = press::PressJournal::open(&f.root.join("journal")).unwrap();
    assert!(j.active().is_none());
    assert!(!f.binding.root.join(".deckle-press/editions").exists());
}
#[test]
fn press_worker_crashed_child_stops_after_three_attempts() {
    let f = fixture("TEST CRASH");
    let mut w = PressSupervisor::with_options(f.options.clone());
    until(&f, &mut w, PressState::Failed);
    assert_eq!(std::fs::read_dir(f.root.join("jobs")).unwrap().count(), 3);
    assert_eq!(
        status(&f).unwrap().error_code,
        Some(press::PressErrorCode::BuildFailed)
    );
}
#[test]
fn press_worker_other_bundle_export_is_untouched() {
    let mut f = fixture("Multiple exports");
    let other = f.root.parent().unwrap().join("other");
    std::fs::create_dir(&other).unwrap();
    f.cfg.export.push(ExportConfig {
        name: "Other".into(),
        dir: other.display().to_string(),
        format: ExportFormat::Bundle,
        ..Default::default()
    });
    let mut w = PressSupervisor::with_options(f.options.clone());
    until(&f, &mut w, PressState::Published);
    assert!(std::fs::read_dir(other).unwrap().next().is_none());
}
#[test]
fn press_worker_stopped_paper_rejects_without_child() {
    let f = fixture("Stopped paper");
    press::write_stop(&f.cfg.press_state(), Some(Utc::now().date_naive())).unwrap();
    let mut w = PressSupervisor::with_options(f.options.clone());
    until(&f, &mut w, PressState::Failed);
    assert_eq!(
        status(&f).unwrap().error_code,
        Some(press::PressErrorCode::PressesStopped)
    );
    assert!(!f.root.join("jobs").exists());
}
#[test]
fn press_worker_recovers_committed_output_after_deadline_without_a_child() {
    let mut f = fixture("Recovery");
    let mut j = press::PressJournal::open(&f.root.join("journal")).unwrap();
    j.register_binding(f.binding.clone(), true, false).unwrap();
    j.accept(f.request.clone(), "fixture", Utc::now()).unwrap();
    let lease = j.admit(Utc::now()).unwrap().unwrap();
    let artifact = press::build_fresh(
        &f.cfg,
        &press::FreshBuildContext {
            identity: press::FreshIdentity {
                paper_id: lease.paper_id,
                edition_id: lease.edition_id,
            },
            started_at: lease.started_at,
            work_dir: f.root.join("prepared"),
            books: false,
        },
        &|_: &str| Err("fake source".into()),
        &deckle_digest::GatherOptions::default(),
    )
    .unwrap();
    let blocked = f.root.join("journal/journal.backup.json.tmp");
    let mut calls = 0;
    assert!(
        press::publish_result(&mut j, &lease, &artifact, || {
            calls += 1;
            if calls == 3 {
                std::fs::create_dir(&blocked).unwrap();
            }
            Utc::now()
        })
        .is_err()
    );
    drop(j);
    std::fs::remove_dir(blocked).unwrap();
    f.options.clock =
        Arc::new(move || lease.deadline + deckle_digest::chrono::Duration::minutes(1));
    f.options.launch = Arc::new(|_| panic!("recovery must not fetch"));
    let mut w = PressSupervisor::with_options(f.options.clone());
    until(&f, &mut w, PressState::Published);
    assert!(!f.root.join("jobs").exists());
}
#[test]
fn press_worker_descriptor_symlink_cannot_read_or_write_outside_job() {
    let f = fixture("TEST HANG");
    let mut w = PressSupervisor::with_options(f.options.clone());
    until(&f, &mut w, PressState::Building);
    drop(w);
    let job = std::fs::read_dir(f.root.join("jobs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
        .join("job.json");
    let mut data: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&job).unwrap()).unwrap();
    data["work"] = f.binding.root.display().to_string().into();
    std::fs::write(&job, serde_json::to_vec(&data).unwrap()).unwrap();
    assert!(
        deckle_cli::press_worker::run_job_with_net(&job, &|_: &str| panic!("must not fetch"))
            .is_err()
    );
    assert!(!f.binding.root.join("inputs").exists());
    #[cfg(unix)]
    {
        let original = job.with_file_name("original.json");
        std::fs::rename(&job, &original).unwrap();
        std::os::unix::fs::symlink(&original, &job).unwrap();
        assert!(
            deckle_cli::press_worker::run_job_with_net(&job, &|_: &str| panic!(
                "linked descriptor must not fetch"
            ))
            .is_err()
        );
    }
}
#[test]
fn press_worker_status_obstruction_does_not_restart_a_running_child() {
    let mut f = fixture("TEST HANG");
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = count.clone();
    f.options.launch = Arc::new(move |p| {
        seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        launch(p)
    });
    let mut w = PressSupervisor::with_options(f.options.clone());
    until(&f, &mut w, PressState::Building);
    let status_path = f.binding.root.join(format!(
        ".deckle-press/status/{}.json",
        f.request.request_id
    ));
    std::fs::remove_file(&status_path).unwrap();
    std::fs::create_dir(&status_path).unwrap();
    for _ in 0..20 {
        let _ = w.tick(std::slice::from_ref(&f.cfg), Utc::now());
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        count.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "shared status outage must not consume build attempts"
    );
}
#[test]
fn press_worker_archive_is_verified_and_published_without_a_child() {
    let mut f = fixture("Archive recovery");
    let mut j = press::PressJournal::open(&f.root.join("journal")).unwrap();
    j.register_binding(f.binding.clone(), true, false).unwrap();
    j.accept(f.request.clone(), "fixture", Utc::now()).unwrap();
    let lease = j.admit(Utc::now()).unwrap().unwrap();
    let artifact = press::build_fresh(
        &f.cfg,
        &press::FreshBuildContext {
            identity: press::FreshIdentity {
                paper_id: lease.paper_id,
                edition_id: lease.edition_id,
            },
            started_at: lease.started_at,
            work_dir: f.root.join("prepared"),
            books: false,
        },
        &|_: &str| Err("fake source".into()),
        &deckle_digest::GatherOptions::default(),
    )
    .unwrap();
    let mut calls = 0;
    assert!(
        press::publish_result(&mut j, &lease, &artifact, || {
            calls += 1;
            if calls == 3 {
                lease.deadline
            } else {
                Utc::now()
            }
        })
        .is_err()
    );
    assert!(press::archived_artifact(&j, &lease).unwrap().is_some());
    drop(j);
    std::fs::remove_dir_all(artifact.bundle).unwrap();
    f.options.launch = Arc::new(|_| panic!("archive recovery must not fetch"));
    let mut w = PressSupervisor::with_options(f.options.clone());
    until(&f, &mut w, PressState::Published);
    assert!(!f.root.join("jobs").exists());
}
#[test]
fn press_worker_changed_binding_allows_running_snapshot_to_finish() {
    let mut f = fixture("TEST DELAY");
    let original = f.binding.root.clone();
    let mut w = PressSupervisor::with_options(f.options.clone());
    until(&f, &mut w, PressState::Building);
    let other = f.root.parent().unwrap().join("replacement");
    std::fs::create_dir(&other).unwrap();
    f.cfg.export[0].dir = other.display().to_string();
    until(&f, &mut w, PressState::Published);
    assert!(std::fs::read_dir(other).unwrap().next().is_none());
    assert!(original.join(".deckle-press/editions").exists());
}
#[test]
fn press_worker_same_paper_two_enabled_folders_and_offline_old_folder() {
    let mut f = fixture("Two folders");
    let other = f.root.parent().unwrap().join("second");
    std::fs::create_dir(&other).unwrap();
    let second = PressBinding {
        root: other.canonicalize().unwrap(),
        ..f.binding.clone()
    };
    f.cfg.export.push(ExportConfig {
        name: "Second".into(),
        dir: other.display().to_string(),
        format: ExportFormat::Bundle,
        press_enabled: true,
        ..Default::default()
    });
    press::setup::write_manifest(
        &second,
        &ProducerManifest {
            format: "deckle-press".into(),
            version: 1,
            producer_id: second.producer_id,
            paper_id: second.paper_id,
            paper_name: "Two folders".into(),
            enabled: true,
            operations: vec![PressOperation::FreshFull],
            updated_at: Utc::now(),
        },
    )
    .unwrap();
    let clock = Arc::new(std::sync::Mutex::new(Utc::now()));
    let c = clock.clone();
    f.options.clock = Arc::new(move || *c.lock().unwrap());
    let mut w = PressSupervisor::with_options(f.options.clone());
    until(&f, &mut w, PressState::Published);
    let first = status(&f).unwrap();
    let mut request = f.request.clone();
    request.request_id = Uuid::new_v4();
    let path = other.join(format!(
        ".deckle-press/requests/{}/{}.json",
        request.reader_id, request.request_id
    ));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_vec(&request).unwrap()).unwrap();
    f.cfg.export[0].press_enabled = false;
    std::fs::rename(&f.binding.root, f.root.parent().unwrap().join("offline")).unwrap();
    *clock.lock().unwrap() += deckle_digest::chrono::Duration::minutes(6);
    let path = other.join(format!(".deckle-press/status/{}.json", request.request_id));
    let end = Instant::now() + Duration::from_secs(30);
    let mut published = None;
    while Instant::now() < end {
        let _ = w.tick(std::slice::from_ref(&f.cfg), Utc::now());
        if let Ok(bytes) = std::fs::read(&path) {
            let s: press::PressStatus = serde_json::from_slice(&bytes).unwrap();
            if s.state == PressState::Published {
                published = Some(s);
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let second_status = published.expect("second root must publish with first offline");
    assert_eq!(std::fs::read_dir(f.root.join("jobs")).unwrap().count(), 2);
    assert_ne!(first.job_id, second_status.job_id);
    assert_ne!(first.result, second_status.result);
    assert!(
        !other
            .join(format!(".deckle-press/status/{}.json", first.request_id))
            .exists()
    );
    drop(w);
    let j = press::PressJournal::open(&f.root.join("journal")).unwrap();
    assert_eq!(j.binding_for_request(first.request_id).unwrap(), f.binding);
    assert_eq!(
        j.binding_for_request(second_status.request_id).unwrap(),
        second
    );
}
#[test]
fn press_worker_old_orphan_attempt_cannot_publish_after_restart() {
    let f = fixture("TEST DELAY");
    let mut w = PressSupervisor::with_options(f.options.clone());
    until(&f, &mut w, PressState::Building);
    drop(w);
    let old = std::fs::read_dir(f.root.join("jobs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
        .join("job.json");
    let mut orphan = launch(&old).unwrap();
    let mut w = PressSupervisor::with_options(f.options.clone());
    until(&f, &mut w, PressState::Published);
    assert!(orphan.wait().unwrap().success());
    drop(w);
    let j = press::PressJournal::open(&f.root.join("journal")).unwrap();
    assert!(j.active().is_none());
    let published = status(&f).unwrap();
    assert_eq!(published.state, PressState::Published);
    let dirs = std::fs::read_dir(f.binding.root.join(".deckle-press/editions"))
        .unwrap()
        .filter(|e| {
            !e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with('.')
        })
        .count();
    assert_eq!(dirs, 1);
    assert!(old.with_file_name("result.json").exists());
    assert_eq!(std::fs::read_dir(f.root.join("jobs")).unwrap().count(), 2);
}
#[test]
#[ignore = "requires an explicitly built desktop executable"]
fn press_worker_desktop_binary_dispatch() {
    let exe = std::env::var_os("DECKLE_TEST_DESKTOP").expect("set built desktop binary");
    let bad = Command::new(&exe)
        .args(["digest", "press-worker", "--job", "/missing/job.json"])
        .output()
        .unwrap();
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("worker"));
    let mut f = fixture("Desktop dispatch");
    // This actual production child exercises dispatch/build/publication with all source
    // sections disabled, so the smoke test cannot contact a live service.
    f.cfg.sections.clear();
    f.cfg.feeds.clear();
    f.options.launch = Arc::new(move |job| {
        Command::new(&exe)
            .args(["digest", "press-worker", "--job"])
            .arg(job)
            .spawn()
    });
    let mut w = PressSupervisor::with_options(f.options.clone());
    until(&f, &mut w, PressState::Published);
}
#[test]
fn press_worker_completed_attempt_releases_duplicate_build_bytes() {
    let f = fixture("Cleanup");
    let mut w = PressSupervisor::with_options(f.options.clone());
    until(&f, &mut w, PressState::Published);
    drop(w);
    for entry in std::fs::read_dir(f.root.join("jobs")).unwrap() {
        assert!(
            !entry.unwrap().path().join("attempt").exists(),
            "only retained archive/shared output should keep build bytes"
        );
    }
}
#[test]
fn press_worker_poisoned_snapshot_reopens_before_recovering() {
    let mut f = fixture("Poison recovery");
    let blocked = f.root.join("journal/journal.backup.json.tmp");
    let obstruction = blocked.clone();
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let calls = count.clone();
    f.options.launch = Arc::new(move |job| {
        let child = launch(job)?;
        if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            std::fs::create_dir(&obstruction)?;
        }
        Ok(child)
    });
    let mut w = PressSupervisor::with_options(f.options.clone());
    let end = Instant::now() + Duration::from_secs(15);
    let mut observed = false;
    while Instant::now() < end {
        if w.tick(std::slice::from_ref(&f.cfg), Utc::now()).is_err() {
            observed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(observed, "real snapshot write failure should surface");
    std::fs::remove_dir(blocked).unwrap();
    until(&f, &mut w, PressState::Published);
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 2);
}
#[test]
fn press_worker_private_state_cannot_be_placed_in_shared_export() {
    let mut f = fixture("Bad local root");
    let invalid = f.binding.root.join("private-state");
    f.options.root = invalid.clone();
    let mut w = PressSupervisor::with_options(f.options.clone());
    let end = Instant::now() + Duration::from_secs(5);
    let mut refused = false;
    while Instant::now() < end {
        if w.tick(std::slice::from_ref(&f.cfg), Utc::now()).is_err() {
            refused = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(refused);
    drop(w);
    assert!(!invalid.exists());
    assert!(!f.binding.root.join(".deckle-press/editions").exists());
}
