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
    if data["config"]["title"] == "TEST PUBLIC WEB" {
        deckle_cli::press_worker::run_job(Path::new(&job)).unwrap();
        return;
    }
    let count = std::sync::atomic::AtomicUsize::new(0);
    deckle_cli::press_worker::run_job_with_net(Path::new(&job), &|url: &str| -> Result<Vec<u8>,String> {
        count.fetch_add(1,std::sync::atomic::Ordering::SeqCst);
        if url.starts_with("https://example.invalid/selected-") { Ok(format!("<html><head><title>{url}</title></head><body><article><h1>{url}</h1>{}</article></body></html>", "<p>Fictional selected article about a town library and its repaired roof.</p>".repeat(100)).into_bytes()) } else if url=="https://example.invalid/feed" {Ok(br#"<rss version="2.0"><channel><title>Fake wire</title><item><title>Library reopened</title><link>https://example.invalid/story</link><description>Repairs are complete.</description></item></channel></rss>"#.to_vec())}else{Err("fake source unavailable".into())}
    }).unwrap();
    // Outside the job folder, which the supervisor removes once the result is recorded:
    // <base>/private/jobs/<id>/job.json -> <base>/source-count.
    let base = Path::new(&job).ancestors().nth(4).unwrap();
    std::fs::write(
        base.join("source-count"),
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
            reading_editions: true,
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
        articles: None,
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
// Published status can become visible before the publisher drops its directory handles.
// Windows deliberately denies rename during that interval; wait for that operation only.
fn rename_after_shared_operations(from: &Path, to: &Path) -> std::io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error)
                if cfg!(windows)
                    && error.raw_os_error() == Some(32)
                    && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => return Err(error),
        }
    }
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
    let counted = f.root.parent().unwrap().join("source-count");
    assert_eq!(std::fs::read_to_string(&counted).unwrap(), "1");
    // A finished job leaves no folder under jobs/.
    assert_eq!(std::fs::read_dir(f.root.join("jobs")).unwrap().count(), 0);
    std::fs::remove_file(&counted).unwrap();
    let mut w = PressSupervisor::with_options(f.options.clone());
    for _ in 0..10 {
        let _ = w.tick(std::slice::from_ref(&f.cfg), Utc::now());
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(w);
    assert_eq!(status(&f).unwrap(), first);
    assert!(!counted.exists(), "restart must not build again");
    assert_eq!(std::fs::read_dir(f.root.join("jobs")).unwrap().count(), 0);
}
/// A folder opened so its times can be set: Windows opens a folder only with backup semantics,
/// and setting a time needs write-attributes access.
fn open_dir_for_times(dir: &std::path::Path) -> std::fs::File {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_WRITE_ATTRIBUTES: u32 = 0x100;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        std::fs::OpenOptions::new()
            .access_mode(FILE_WRITE_ATTRIBUTES)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(dir)
            .unwrap()
    }
    #[cfg(not(windows))]
    std::fs::File::open(dir).unwrap()
}
#[test]
fn press_worker_removes_stale_job_folders_at_start() {
    let f = fixture("TEST HANG");
    let mut w = PressSupervisor::with_options(f.options.clone());
    until(&f, &mut w, PressState::Building);
    drop(w);
    let jobs = f.root.join("jobs");
    // The interrupted job is still held by the journal; keep its folder even when old.
    let held = std::fs::read_dir(&jobs)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let damaged = jobs.join(Uuid::new_v4().to_string());
    std::fs::create_dir(&damaged).unwrap();
    std::fs::write(damaged.join("job.json"), b"{").unwrap();
    let recent = jobs.join(Uuid::new_v4().to_string());
    std::fs::create_dir(&recent).unwrap();
    std::fs::write(recent.join("job.json"), b"{").unwrap();
    let two_days_ago = std::time::SystemTime::now() - Duration::from_secs(2 * 24 * 3600);
    for dir in [&held, &damaged] {
        open_dir_for_times(dir).set_modified(two_days_ago).unwrap();
    }
    let mut w = PressSupervisor::with_options(f.options.clone());
    // The status still says building from the first run; wait for the sweep itself.
    let end = Instant::now() + Duration::from_secs(30);
    while damaged.exists() && Instant::now() < end {
        let _ = w.tick(std::slice::from_ref(&f.cfg), Utc::now());
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(w);
    assert!(!damaged.exists(), "an old unreadable job folder is removed");
    assert!(held.exists(), "a job the journal holds keeps its folder");
    assert!(recent.exists(), "a folder under a day old is kept");
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
    deckle_digest::config::save(&f.options.config_path, &f.cfg).unwrap();
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
    deckle_digest::config::save(&f.options.config_path, &f.cfg).unwrap();
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
            reading_editions: true,
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
    deckle_digest::config::save(&f.options.config_path, &f.cfg).unwrap();
    rename_after_shared_operations(&f.binding.root, &f.root.parent().unwrap().join("offline"))
        .unwrap();
    *clock.lock().unwrap() += deckle_digest::chrono::Duration::minutes(6);
    let path = other.join(format!(".deckle-press/status/{}.json", request.request_id));
    let end = Instant::now() + Duration::from_secs(30);
    // Submit the changed config once. Repeated ticks increment the policy revision;
    // on slower filesystems they can continually supersede admission's policy check.
    // The supervisor continues processing its current config on its own timer.
    let _ = w.tick(std::slice::from_ref(&f.cfg), Utc::now());
    let mut published = None;
    let mut last_status = None;
    while Instant::now() < end {
        if let Ok(bytes) = std::fs::read(&path) {
            let s: press::PressStatus = serde_json::from_slice(&bytes).unwrap();
            if s.state == PressState::Published {
                published = Some(s);
                break;
            }
            last_status = Some(s);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let second_status = published.unwrap_or_else(|| {
        panic!("second root must publish with first offline; last status: {last_status:?}")
    });
    assert_ne!(first.job_id, second_status.job_id);
    assert_ne!(first.result, second_status.result);
    assert!(
        !other
            .join(format!(".deckle-press/status/{}.json", first.request_id))
            .exists()
    );
    drop(w);
    // The status is written before the finished job's folder is removed.
    assert_eq!(std::fs::read_dir(f.root.join("jobs")).unwrap().count(), 0);
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
    // The interrupted attempt's folder stays: its result was never recorded. The
    // finished attempt's folder is removed.
    assert_eq!(std::fs::read_dir(f.root.join("jobs")).unwrap().count(), 1);
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
fn press_worker_failed_journal_write_while_registering_is_retried_at_once() {
    // A journal write that fails while a folder is registered is the journal's, not the
    // folder's: once the write can go, the folder works on the next tick, not after the
    // minute a refused folder move waits.
    let f = fixture("Registration write");
    let blocked = f.root.join("journal/journal.backup.json.tmp");
    std::fs::create_dir_all(&blocked).unwrap();
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
    assert!(observed, "the failed journal write should surface");
    std::fs::remove_dir(blocked).unwrap();
    until(&f, &mut w, PressState::Published);
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

fn admission_boundary_change(change: &str) {
    use std::sync::{
        Barrier, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    };
    let mut f = fixture("Admission boundary");
    let calls = Arc::new(AtomicUsize::new(0));
    let clock_calls = calls.clone();
    let (entered, at_boundary) = mpsc::sync_channel(1);
    let release = Arc::new(Barrier::new(2));
    let clock_release = release.clone();
    let sent = Mutex::new(Some(entered));
    f.options.clock = Arc::new(move || {
        // First sample drives scanning; the second occurs while holding run.lock,
        // immediately before admission. Pause real supervisor execution at that boundary.
        if clock_calls.fetch_add(1, Ordering::SeqCst) == 1 {
            sent.lock().unwrap().take().unwrap().send(()).unwrap();
            clock_release.wait();
        }
        Utc::now()
    });
    let launches = Arc::new(AtomicUsize::new(0));
    let spawned = launches.clone();
    f.options.launch = Arc::new(move |job| {
        spawned.fetch_add(1, Ordering::SeqCst);
        launch(job)
    });
    let mut w = PressSupervisor::with_options(f.options.clone());
    w.tick(std::slice::from_ref(&f.cfg), Utc::now()).unwrap();
    at_boundary.recv_timeout(Duration::from_secs(5)).unwrap();
    let held = std::fs::OpenOptions::new()
        .write(true)
        .open(&f.options.run_lock)
        .unwrap();
    assert!(matches!(
        held.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    let error = match change {
        "snapshot" => {
            f.cfg.export[0].press_enabled = false;
            deckle_digest::config::save(&f.options.config_path, &f.cfg).unwrap();
            w.tick(std::slice::from_ref(&f.cfg), Utc::now()).unwrap();
            press::PressErrorCode::Disabled
        }
        "stop" => {
            press::write_stop(
                &f.cfg.press_state(),
                Some(
                    Utc::now()
                        .with_timezone(&deckle_digest::tz::local_tz())
                        .date_naive(),
                ),
            )
            .unwrap();
            press::PressErrorCode::PressesStopped
        }
        "binding" => {
            let path = f.binding.root.join(".deckle-press/producer.json");
            let mut manifest: ProducerManifest =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            manifest.enabled = false;
            press::setup::write_manifest(&f.binding, &manifest).unwrap();
            press::PressErrorCode::Disabled
        }
        _ => unreachable!(),
    };
    release.wait();
    // Do not send more ticks: that would hide a stale policy bug by supplying a new revision.
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline
        && launches.load(Ordering::SeqCst) == 0
        && !status(&f).is_some_and(|s| s.state == PressState::Failed)
    {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        launches.load(Ordering::SeqCst),
        0,
        "obsolete policy launched a child after acquiring run.lock"
    );
    let rejected = status(&f).expect("durable rejection");
    assert_eq!(rejected.error_code, Some(error));
    assert!(rejected.started_at.is_none());
}
#[test]
fn press_worker_rechecks_latest_snapshot_after_lock() {
    admission_boundary_change("snapshot");
}
#[test]
fn press_worker_rechecks_stop_after_lock() {
    admission_boundary_change("stop");
}
#[test]
fn press_worker_rechecks_binding_after_lock() {
    admission_boundary_change("binding");
}

#[test]
fn press_worker_preserves_fraction_only_shelf_progress() {
    let mut f = fixture("Shelf fraction");
    f.cfg.sections = vec![deckle_digest::SectionKind::Shelf];
    let bytes=serde_json::to_vec(&serde_json::json!({"kindles":{"test-device":{"name":"Test device","seen":Utc::now().to_rfc3339(),"books":[{"file":"a-book.kfx","title":"Test book","last_read":Utc::now().to_rfc3339(),"kindle_fraction":0.613}]}},"shown":{}})).unwrap();
    std::fs::write(f.cfg.shelf_store(), &bytes).unwrap();
    let mut w = PressSupervisor::with_options(f.options.clone());
    until(&f, &mut w, PressState::Published);
    let result = status(&f).unwrap().result.unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(f.binding.root.join(result.bundle).join("edition.json")).unwrap(),
    )
    .unwrap();
    let text = manifest["sections"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|s| s["stories"].as_array().unwrap())
        .filter_map(|s| s["body"]["text"].as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        text.contains("about 61%"),
        "fresh shelf omitted supported fraction progress: {text}"
    );
    assert_eq!(std::fs::read(f.cfg.shelf_store()).unwrap(), bytes);
}

// Exercises the public controls and the real child builder with injected source I/O.
// Removing coalescing, publication inventory, or scheduled isolation breaks this test.
#[test]
fn press_worker_protocol_smoke_exchange() {
    let mut f = fixture("TEST DELAY");
    let now = deckle_digest::chrono::TimeZone::with_ymd_and_hms(
        &deckle_digest::tz::local_tz(),
        2026,
        10,
        27,
        23,
        59,
        59,
    )
    .single()
    .unwrap()
    .with_timezone(&Utc);
    let clock = Arc::new(std::sync::Mutex::new(now));
    let shared_clock = clock.clone();
    f.options.clock = Arc::new(move || *shared_clock.lock().unwrap());
    f.request.created_at = now;
    f.request.expires_at = now + deckle_digest::chrono::Duration::hours(24);
    let write_request = |request: &PressRequest| {
        let path = f.binding.root.join(format!(
            ".deckle-press/requests/{}/{}.json",
            request.reader_id, request.request_id
        ));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, serde_json::to_vec(request).unwrap()).unwrap();
    };
    write_request(&f.request);
    let mut second = f.request.clone();
    second.reader_id = Uuid::new_v4();
    second.request_id = Uuid::new_v4();
    write_request(&second);
    let legacy = f.binding.root.join("scheduled.deckle/edition.json");
    let marks = f.binding.root.join(format!(
        ".deckle-returns/{}/2026-10-27.json",
        f.request.reader_id
    ));
    for (path, bytes) in [
        (
            &legacy,
            include_bytes!("../../../../fixtures/press/bundle-1.3.json").as_slice(),
        ),
        (
            &marks,
            include_bytes!("../../../../fixtures/press/returns-legacy.json").as_slice(),
        ),
    ] {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    let legacy_before = std::fs::read(&legacy).unwrap();
    let marks_before = std::fs::read(&marks).unwrap();
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&f.options.run_lock)
        .unwrap();
    lock.lock().unwrap();
    let mut worker = PressSupervisor::with_options(f.options.clone());
    until(&f, &mut worker, PressState::Queued);
    let queued = status(&f).unwrap();
    drop(lock);
    // Cross midnight while queued; identity is UUID-scoped, not a scheduled date key.
    *clock.lock().unwrap() += deckle_digest::chrono::Duration::seconds(2);
    until(&f, &mut worker, PressState::Building);
    let building = status(&f).unwrap();
    until(&f, &mut worker, PressState::Published);
    let published = status(&f).unwrap();
    let other_path = f
        .binding
        .root
        .join(format!(".deckle-press/status/{}.json", second.request_id));
    let deadline = Instant::now() + Duration::from_secs(30);
    // Each status is an independent file; one publication can arrive before another.
    let other = loop {
        let other: press::PressStatus =
            serde_json::from_slice(&std::fs::read(&other_path).unwrap()).unwrap();
        if other.state == PressState::Published {
            break other;
        }
        assert!(
            Instant::now() < deadline,
            "second coalesced status did not publish: {other:?}"
        );
        worker.tick(std::slice::from_ref(&f.cfg), now).unwrap();
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(other.state, PressState::Published);
    assert_eq!(published.job_id, other.job_id);
    assert_eq!(published.result, other.result);
    assert!(queued.revision < building.revision && building.revision < published.revision);
    assert_eq!(std::fs::read(&legacy).unwrap(), legacy_before);
    assert_eq!(std::fs::read(&marks).unwrap(), marks_before);
    let result = published.result.as_ref().unwrap();
    let bundle = f.binding.root.join(&result.bundle);
    let bytes = std::fs::read(bundle.join("edition.json")).unwrap();
    use sha2::{Digest, Sha256};
    assert_eq!(
        Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
        result.manifest_sha256
    );
    let edition: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(edition["edition"]["id"], result.edition_id.to_string());
    assert_eq!(
        edition["edition"]["paper_id"],
        f.binding.paper_id.to_string()
    );
    assert_eq!(edition["edition"]["kind"], "fresh");
    assert_eq!(edition["edition"]["date"], "2026-10-28");
    assert!(!edition["files"].as_array().unwrap().is_empty());
    for file in edition["files"].as_array().unwrap() {
        let data = std::fs::read(bundle.join(file["path"].as_str().unwrap())).unwrap();
        assert_eq!(data.len() as u64, file["size"].as_u64().unwrap());
        assert_eq!(
            Sha256::digest(&data)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            file["sha256"].as_str().unwrap()
        );
    }
    drop(worker);
    // The status is written before the finished job's folder is removed.
    assert_eq!(std::fs::read_dir(f.root.join("jobs")).unwrap().count(), 0);
    let mut worker = PressSupervisor::with_options(f.options.clone());
    worker.tick(std::slice::from_ref(&f.cfg), now).unwrap();
    assert_eq!(status(&f).unwrap(), published);
    assert_eq!(std::fs::read_dir(f.root.join("jobs")).unwrap().count(), 0);
    if let Some(output) = std::env::var_os("DECKLE_PRESS_SMOKE_OUTPUT") {
        let output = Path::new(&output);
        std::fs::create_dir(output).expect("smoke output must not already exist");
        for (name, value) in [
            ("request.json", serde_json::to_value(&f.request).unwrap()),
            (
                "request-second.json",
                serde_json::to_value(&second).unwrap(),
            ),
            ("status-queued.json", serde_json::to_value(&queued).unwrap()),
            (
                "status-building.json",
                serde_json::to_value(&building).unwrap(),
            ),
            (
                "status-published.json",
                serde_json::to_value(&published).unwrap(),
            ),
            ("status-second.json", serde_json::to_value(&other).unwrap()),
        ] {
            std::fs::write(
                output.join(name),
                serde_json::to_vec_pretty(&value).unwrap(),
            )
            .unwrap();
        }
        let manifest = ProducerManifest {
            format: "deckle-press".into(),
            version: 1,
            producer_id: f.binding.producer_id,
            paper_id: f.binding.paper_id,
            paper_name: "Protocol smoke".into(),
            enabled: true,
            operations: vec![PressOperation::FreshFull],
            reading_editions: true,
            updated_at: now,
        };
        std::fs::write(
            output.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        fn copy_tree(source: &Path, destination: &Path) {
            std::fs::create_dir_all(destination).unwrap();
            for entry in std::fs::read_dir(source).unwrap() {
                let entry = entry.unwrap();
                let dest = destination.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    copy_tree(&entry.path(), &dest);
                } else {
                    std::fs::copy(entry.path(), &dest).unwrap();
                }
            }
        }
        copy_tree(&bundle, &output.join(&result.bundle));
        std::fs::write(output.join("scheduled-before.json"), legacy_before).unwrap();
        std::fs::write(
            output.join("scheduled-after.json"),
            std::fs::read(legacy).unwrap(),
        )
        .unwrap();
        std::fs::write(output.join("marks-before.json"), marks_before).unwrap();
        std::fs::write(
            output.join("marks-after.json"),
            std::fs::read(marks).unwrap(),
        )
        .unwrap();
    }
}

fn regression_admission_boundary_change(change: &str) {
    use std::sync::{
        Barrier, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    };
    let mut f = fixture("Admission boundary");
    let calls = Arc::new(AtomicUsize::new(0));
    let clock_calls = calls.clone();
    let (entered, at_boundary) = mpsc::sync_channel(1);
    let release = Arc::new(Barrier::new(2));
    let clock_release = release.clone();
    let sent = Mutex::new(Some(entered));
    f.options.clock = Arc::new(move || {
        // First sample drives scanning; the second occurs while holding run.lock,
        // immediately before admission. Pause real supervisor execution at that boundary.
        if clock_calls.fetch_add(1, Ordering::SeqCst) == 1 {
            sent.lock().unwrap().take().unwrap().send(()).unwrap();
            clock_release.wait();
        }
        Utc::now()
    });
    let launches = Arc::new(AtomicUsize::new(0));
    let spawned = launches.clone();
    f.options.launch = Arc::new(move |job| {
        spawned.fetch_add(1, Ordering::SeqCst);
        launch(job)
    });
    let mut w = PressSupervisor::with_options(f.options.clone());
    w.tick(std::slice::from_ref(&f.cfg), Utc::now()).unwrap();
    at_boundary.recv_timeout(Duration::from_secs(5)).unwrap();
    let held = std::fs::OpenOptions::new()
        .write(true)
        .open(&f.options.run_lock)
        .unwrap();
    assert!(matches!(
        held.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    let error = match change {
        "snapshot" => {
            f.cfg.export[0].press_enabled = false;
            deckle_digest::config::save(&f.options.config_path, &f.cfg).unwrap();

            press::PressErrorCode::Disabled
        }
        "stop" => {
            press::write_stop(
                &f.cfg.press_state(),
                Some(
                    Utc::now()
                        .with_timezone(&deckle_digest::tz::local_tz())
                        .date_naive(),
                ),
            )
            .unwrap();
            press::PressErrorCode::PressesStopped
        }
        "binding" => {
            let path = f.binding.root.join(".deckle-press/producer.json");
            let mut manifest: ProducerManifest =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            manifest.enabled = false;
            press::setup::write_manifest(&f.binding, &manifest).unwrap();
            press::PressErrorCode::Disabled
        }
        _ => unreachable!(),
    };
    release.wait();
    // Do not send more ticks: that would hide a stale policy bug by supplying a new revision.
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline
        && launches.load(Ordering::SeqCst) == 0
        && !status(&f).is_some_and(|s| s.state == PressState::Failed)
    {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        launches.load(Ordering::SeqCst),
        0,
        "obsolete policy launched a child after acquiring run.lock"
    );
    let rejected = status(&f).expect("durable rejection");
    assert_eq!(rejected.error_code, Some(error));
    assert!(rejected.started_at.is_none());
}

#[test]
fn regression_disk_opt_out_after_lock() {
    regression_admission_boundary_change("snapshot");
}

#[test]
fn regression_missing_root_after_intent_respects_deadline() {
    let mut f = fixture("Recovery offline");
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
                // Interrupt before commit, then disconnect after pinned handles are released.
                // Windows correctly prevents a rename inside this protected transaction.
                lease.deadline
            } else {
                Utc::now()
            }
        })
        .is_err()
    );
    assert_eq!(calls, 3);
    assert!(press::archived_artifact(&j, &lease).unwrap().is_some());
    assert!(j.active().is_some());
    assert!(
        !f.binding
            .root
            .join(format!(
                ".deckle-press/editions/{}.deckle",
                lease.edition_id
            ))
            .exists()
    );
    std::fs::rename(&f.binding.root, f.root.parent().unwrap().join("offline")).unwrap();
    drop(j);
    f.options.clock =
        Arc::new(move || lease.deadline + deckle_digest::chrono::Duration::minutes(1));
    f.options.launch = Arc::new(|_| panic!("expired job must not fetch"));
    let mut w = PressSupervisor::with_options(f.options.clone());
    for _ in 0..10 {
        let _ = w.tick(std::slice::from_ref(&f.cfg), Utc::now());
        std::thread::sleep(Duration::from_millis(100));
    }
    let lock = std::fs::OpenOptions::new()
        .write(true)
        .open(&f.options.run_lock)
        .unwrap();
    let locked = lock.try_lock().is_err();
    assert!(!locked, "expired recovery retained the global lock");
    drop(w);
    let j = press::PressJournal::open(&f.root.join("journal")).unwrap();
    assert!(
        j.active().is_none(),
        "expired recovery still active, scheduled lock held={locked}, status={:?}",
        j.status(f.request.request_id)
    );
}

fn prepare_artifact(
    f: &Fixture,
    j: &mut press::PressJournal,
) -> (press::JobLease, press::FreshArtifact) {
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
    (lease, artifact)
}
#[cfg(unix)]
#[test]
fn regression_publication_rejects_replaced_editions_ancestor() {
    let f = fixture("Replacement");
    let mut j = press::PressJournal::open(&f.root.join("journal")).unwrap();
    let (lease, artifact) = prepare_artifact(&f, &mut j);
    let editions = f.binding.root.join(".deckle-press/editions");
    let outside = f.root.parent().unwrap().join("outside");
    let mut calls = 0;
    let result = press::publish_result(&mut j, &lease, &artifact, || {
        calls += 1;
        if calls == 3 {
            std::fs::rename(&editions, &outside).unwrap();
            std::os::unix::fs::symlink(&outside, &editions).unwrap();
        }
        Utc::now()
    });
    let escaped = outside
        .join(format!("{}.deckle", lease.edition_id))
        .exists();
    assert!(
        !escaped,
        "published outside configured root after editions ancestor replacement, returned={result:?}"
    );
    assert!(result.is_err());
    assert_ne!(
        j.status(f.request.request_id).unwrap().state,
        PressState::Published
    );
}
#[test]
fn regression_terminal_retention_ignores_unavailable_other_folder() {
    let f = fixture("Retention");
    let mut j = press::PressJournal::open(&f.root.join("journal")).unwrap();
    let (lease, artifact) = prepare_artifact(&f, &mut j);
    press::publish_result(&mut j, &lease, &artifact, Utc::now).unwrap();
    let root = f.root.parent().unwrap().join("other");
    std::fs::create_dir(&root).unwrap();
    let other = PressBinding {
        root: root.clone(),
        paper_id: Uuid::new_v4(),
        ..f.binding.clone()
    };
    j.register_binding(other.clone(), true, false).unwrap();
    let mut old = f.request.clone();
    old.request_id = Uuid::new_v4();
    old.paper_id = other.paper_id;
    j.reject_in(&other, old, "old", press::PressErrorCode::Busy, Utc::now())
        .unwrap();
    std::fs::remove_dir(root).unwrap();
    // The healthy first request is 31 days old and still retained as an output; add a
    // terminal request in its available folder which should be pruned independently.
    let mut rejected = f.request.clone();
    rejected.request_id = Uuid::new_v4();
    j.reject_in(
        &f.binding,
        rejected.clone(),
        "healthy",
        press::PressErrorCode::Busy,
        Utc::now(),
    )
    .unwrap();
    let result = press::prune_results(
        &mut j,
        Utc::now() + deckle_digest::chrono::Duration::days(31),
    );
    assert!(
        j.status(rejected.request_id).is_none(),
        "unavailable unrelated root blocked healthy terminal retention: {result:?}"
    );
}
#[test]
fn regression_ancient_request_is_not_rejournaled_after_retention() {
    let f = fixture("Ancient replay");
    let at = Utc::now();
    let mut j = press::PressJournal::open(&f.root.join("journal")).unwrap();
    j.register_binding(f.binding.clone(), true, false).unwrap();
    let hash = "fixture";
    j.reject_in(
        &f.binding,
        f.request.clone(),
        hash,
        press::PressErrorCode::Busy,
        at,
    )
    .unwrap();
    press::flush_statuses(&mut j).unwrap();
    let later = at + deckle_digest::chrono::Duration::days(31);
    press::prune_results(&mut j, later).unwrap();
    assert!(j.status(f.request.request_id).is_none());
    for event in
        press::scan::scan_requests(&f.binding, &mut press::scan::ScanCursor::default(), later)
    {
        if let press::scan::ScanEvent::Request {
            request,
            hash,
            rejection: Some(code),
        } = event
        {
            j.reject_in(&f.binding, request, &hash, code, later)
                .unwrap();
        }
    }
    assert!(
        j.status(f.request.request_id).is_none(),
        "ancient request recreated a new terminal journal/status after pruning: {:?}",
        j.status(f.request.request_id)
    );
}

// Optional real-client bridge. Only a disposable, explicitly supplied root is used;
// fake source responses and the normal child supervisor exercise the actual protocol.
#[test]
#[ignore]
fn controller_exchange_bridge() {
    let Some(root) = std::env::var_os("DECKLE_PRESS_CONTROLLER_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    assert!(!root.exists(), "bridge requires a new disposable root");
    std::fs::create_dir(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let mut f = fixture("Controller bridge");
    f.binding.root = root.clone();
    f.cfg.export[0].dir = root.display().to_string();
    deckle_digest::config::save(&f.options.config_path, &f.cfg).unwrap();
    press::setup::write_manifest(
        &f.binding,
        &ProducerManifest {
            format: "deckle-press".into(),
            version: 1,
            producer_id: f.binding.producer_id,
            paper_id: f.binding.paper_id,
            paper_name: "Controller bridge".into(),
            enabled: true,
            operations: vec![PressOperation::FreshFull],
            reading_editions: true,
            updated_at: Utc::now(),
        },
    )
    .unwrap();
    let manifest = std::fs::read(root.join(".deckle-press/producer.json")).unwrap();
    std::fs::write(root.join("bridge-ready"), b"ready").unwrap();
    let mut supervisor = PressSupervisor::with_options(f.options.clone());
    let deadline = Instant::now() + Duration::from_secs(240);
    while !root.join("bridge-complete.json").exists() {
        assert!(
            Instant::now() < deadline,
            "client did not complete controller exchange"
        );
        supervisor
            .tick(std::slice::from_ref(&f.cfg), Utc::now())
            .unwrap();
        std::thread::sleep(Duration::from_millis(25));
    }
    drop(supervisor);
    let proof: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("bridge-complete.json")).unwrap()).unwrap();
    let id = Uuid::parse_str(proof["request_id"].as_str().unwrap()).unwrap();
    let journal = press::PressJournal::open(&f.root.join("journal")).unwrap();
    let status = journal
        .status(id)
        .expect("actual client request must be journaled");
    assert_eq!(status.state, PressState::Published);
    assert_eq!(
        status.result.unwrap().edition_id.to_string(),
        proof["edition_id"].as_str().unwrap()
    );
    assert_eq!(
        std::fs::read(root.join(".deckle-press/producer.json")).unwrap(),
        manifest
    );
}

#[test]
fn regression_ancient_expiry_boundary_and_known_request_reconciliation() {
    let f = fixture("Expiry horizon");
    let mut j = press::PressJournal::open(&f.root.join("journal")).unwrap();
    j.register_binding(f.binding.clone(), true, false).unwrap();
    let at = Utc::now();
    let known = j
        .reject_in(
            &f.binding,
            f.request.clone(),
            "known",
            press::PressErrorCode::Busy,
            at,
        )
        .unwrap();
    for (seconds, retained) in [(1, true), (0, false), (-1, false)] {
        let mut request = f.request.clone();
        request.request_id = Uuid::new_v4();
        // First seen 48 hours or more after expiry: answered once, never journaled.
        request.expires_at = at - deckle_digest::chrono::Duration::hours(48)
            + deckle_digest::chrono::Duration::seconds(seconds);
        request.created_at = request.expires_at - deckle_digest::chrono::Duration::hours(24);
        j.reject_in(
            &f.binding,
            request.clone(),
            "expired",
            press::PressErrorCode::Expired,
            at,
        )
        .unwrap();
        assert_eq!(j.status(request.request_id).is_some(), retained);
    }
    let reconciled = j
        .reject_in(
            &f.binding,
            f.request.clone(),
            "known",
            press::PressErrorCode::Expired,
            at + deckle_digest::chrono::Duration::days(40),
        )
        .unwrap();
    assert_eq!(reconciled, known);
}

#[cfg(unix)]
#[test]
fn regression_inaccessible_and_corrupt_intent_recovery_releases_global_lock() {
    use std::os::unix::fs::PermissionsExt;
    for failure in ["permission", "corrupt"] {
        let mut f = fixture("Intent deadline");
        let mut j = press::PressJournal::open(&f.root.join("journal")).unwrap();
        let (lease, artifact) = prepare_artifact(&f, &mut j);
        // Interrupt before final rename, leaving a durable intent and verified archive.
        let mut samples = 0;
        assert!(
            press::publish_result(&mut j, &lease, &artifact, || {
                samples += 1;
                if samples >= 3 {
                    lease.deadline
                } else {
                    lease.started_at
                }
            })
            .is_err()
        );
        let final_path = f.binding.root.join(format!(
            ".deckle-press/editions/{}.deckle",
            lease.edition_id
        ));
        if failure == "permission" {
            std::fs::set_permissions(&f.binding.root, std::fs::Permissions::from_mode(0o000))
                .unwrap();
            assert!(
                std::fs::read_dir(&f.binding.root).is_err(),
                "test must actually deny access"
            );
        } else {
            std::fs::create_dir(&final_path).unwrap();
            std::fs::write(final_path.join("edition.json"), b"corrupt").unwrap();
        }
        assert!(press::recover_result(&mut j, &lease, lease.deadline).is_err());
        drop(j);
        f.options.clock =
            Arc::new(move || lease.deadline + deckle_digest::chrono::Duration::seconds(1));
        f.options.launch = Arc::new(|_| panic!("expired intent must not launch"));
        let mut supervisor = PressSupervisor::with_options(f.options.clone());
        for _ in 0..10 {
            let _ = supervisor.tick(std::slice::from_ref(&f.cfg), Utc::now());
            std::thread::sleep(Duration::from_millis(25));
        }
        let lock = std::fs::OpenOptions::new()
            .write(true)
            .open(&f.options.run_lock)
            .unwrap();
        assert!(lock.try_lock().is_ok(), "{failure} retained global lock");
        drop(lock);
        drop(supervisor);
        std::fs::set_permissions(&f.binding.root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let j = press::PressJournal::open(&f.root.join("journal")).unwrap();
        assert!(j.active().is_none());
        assert_eq!(
            j.status(f.request.request_id).unwrap().error_code,
            Some(press::PressErrorCode::TimedOut)
        );
    }
}

fn save_reading_request(f: &mut Fixture, urls: &[&str]) {
    f.request.operation = PressOperation::ReadingEdition;
    f.request.articles = Some(urls.iter().map(|u| (*u).to_owned()).collect());
    let path = f.binding.root.join(format!(
        ".deckle-press/requests/{}/{}.json",
        f.request.reader_id, f.request.request_id
    ));
    std::fs::write(path, serde_json::to_vec(&f.request).unwrap()).unwrap();
}
#[test]
fn press_worker_reading_child_publishes_selection_and_reports_partial_source_failure() {
    for fail in [false, true] {
        let mut f = fixture("Selected process paper");
        save_reading_request(
            &mut f,
            &[
                "https://example.invalid/selected-b",
                if fail {
                    "https://example.invalid/missing"
                } else {
                    "https://example.invalid/selected-a"
                },
            ],
        );
        let mut w = PressSupervisor::with_options(f.options.clone());
        until(
            &f,
            &mut w,
            if fail {
                PressState::Failed
            } else {
                PressState::Published
            },
        );
        drop(w);
        let s = status(&f).unwrap();
        if fail {
            assert_eq!(s.error_code, Some(press::PressErrorCode::SourceFailure));
            assert!(s.result.is_none());
        } else {
            let manifest: serde_json::Value = serde_json::from_slice(
                &std::fs::read(
                    f.binding
                        .root
                        .join(s.result.unwrap().bundle)
                        .join("edition.json"),
                )
                .unwrap(),
            )
            .unwrap();
            assert_eq!(manifest["sections"].as_array().unwrap().len(), 1);
            let stories = manifest["sections"][0]["stories"].as_array().unwrap();
            assert_eq!(stories.len(), 2);
            assert!(stories[0]["title"].as_str().unwrap().contains("selected-b"));
            assert!(stories[1]["title"].as_str().unwrap().contains("selected-a"));
        }
        assert!(!f.cfg.reading.queue_path.as_ref().unwrap().exists());
    }
}

#[test]
fn press_worker_concurrent_reading_selections_publish_distinct_outputs() {
    use std::sync::atomic::{AtomicI64, Ordering};
    let mut f = fixture("TEST DELAY");
    save_reading_request(&mut f, &["https://example.invalid/selected-a"]);
    let instant = Utc::now();
    let offset = Arc::new(AtomicI64::new(0));
    let clock_offset = offset.clone();
    f.options.clock = Arc::new(move || {
        instant + deckle_digest::chrono::Duration::seconds(clock_offset.load(Ordering::SeqCst))
    });
    let first_request = f.request.clone();
    let mut w = PressSupervisor::with_options(f.options.clone());
    until(&f, &mut w, PressState::Building);
    f.request.request_id = Uuid::new_v4();
    save_reading_request(&mut f, &["https://example.invalid/selected-b"]);
    let second_request_id = f.request.request_id;
    until(&f, &mut w, PressState::Queued);
    let second_job = status(&f).unwrap().job_id;
    f.request = first_request;
    until(&f, &mut w, PressState::Published);
    let first_status = status(&f).unwrap();
    assert_ne!(first_status.job_id, second_job);
    f.request.request_id = second_request_id;
    offset.store(301, Ordering::SeqCst);
    until(&f, &mut w, PressState::Published);
    let second_status = status(&f).unwrap();
    drop(w);
    let first = first_status.result.unwrap();
    let second = second_status.result.unwrap();
    assert_ne!(first.edition_id, second.edition_id);
    for (result, expected, excluded) in [
        (first, "selected-a", "selected-b"),
        (second, "selected-b", "selected-a"),
    ] {
        let body = std::fs::read_to_string(f.binding.root.join(result.bundle).join("edition.json"))
            .unwrap();
        assert!(body.contains(expected));
        assert!(!body.contains(excluded));
    }
}

#[test]
fn press_worker_production_reading_denies_private_destination_before_connecting() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut f = fixture("TEST PUBLIC WEB");
    let url = format!("http://{}/private", listener.local_addr().unwrap());
    save_reading_request(&mut f, &[&url]);
    let mut w = PressSupervisor::with_options(f.options.clone());
    until(&f, &mut w, PressState::Failed);
    drop(w);
    assert_eq!(
        status(&f).unwrap().error_code,
        Some(press::PressErrorCode::SourceFailure)
    );
    assert!(listener.accept().is_err());
}

#[test]
fn press_worker_accepts_legacy_descriptor_without_source_input() {
    let mut f = fixture("Legacy descriptor");
    f.options.launch = Arc::new(|path| {
        let mut value: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?).unwrap();
        value.as_object_mut().unwrap().remove("input");
        std::fs::write(path, serde_json::to_vec(&value).unwrap())?;
        launch(path)
    });
    let mut w = PressSupervisor::with_options(f.options.clone());
    until(&f, &mut w, PressState::Published);
    drop(w);
    let body = std::fs::read_to_string(
        f.binding
            .root
            .join(status(&f).unwrap().result.unwrap().bundle)
            .join("edition.json"),
    )
    .unwrap();
    assert!(body.contains("Library reopened"));
}

type Clock = Arc<std::sync::Mutex<deckle_digest::chrono::DateTime<Utc>>>;
fn clocked(f: &mut Fixture) -> Clock {
    let clock = Arc::new(std::sync::Mutex::new(Utc::now()));
    let c = clock.clone();
    f.options.clock = Arc::new(move || *c.lock().unwrap());
    clock
}
fn hours(n: i64) -> deckle_digest::chrono::Duration {
    deckle_digest::chrono::Duration::hours(n)
}
fn request_file(f: &Fixture, r: &PressRequest) -> std::path::PathBuf {
    f.binding.root.join(format!(
        ".deckle-press/requests/{}/{}.json",
        r.reader_id, r.request_id
    ))
}
fn write_request(f: &Fixture, r: &PressRequest) {
    let path = request_file(f, r);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_vec(r).unwrap()).unwrap();
}
fn status_file(f: &Fixture, id: Uuid) -> std::path::PathBuf {
    f.binding
        .root
        .join(format!(".deckle-press/status/{id}.json"))
}
fn status_of(f: &Fixture, id: Uuid) -> Option<press::PressStatus> {
    serde_json::from_slice(&std::fs::read(status_file(f, id)).ok()?).ok()
}
fn wait_for_status(f: &Fixture, w: &mut PressSupervisor, id: Uuid) -> press::PressStatus {
    let end = Instant::now() + Duration::from_secs(30);
    while Instant::now() < end {
        let _ = w.tick(std::slice::from_ref(&f.cfg), Utc::now());
        if let Some(s) = status_of(f, id) {
            return s;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("no status for {id}");
}
/// Several supervisor steps; each one scans the fixture's only folder.
fn steps(f: &Fixture, w: &mut PressSupervisor) -> Vec<String> {
    let mut errors = Vec::new();
    for _ in 0..10 {
        if let Err(e) = w.tick(std::slice::from_ref(&f.cfg), Utc::now()) {
            errors.push(e);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    errors
}
#[test]
fn press_worker_forgotten_refusals_are_not_journaled_again() {
    let mut f = fixture("Stopped refusals");
    let clock = clocked(&mut f);
    press::write_stop(&f.cfg.press_state(), Some(Utc::now().date_naive())).unwrap();
    let mut mismatch = f.request.clone();
    mismatch.request_id = Uuid::new_v4();
    mismatch.expires_at =
        deckle_digest::chrono::TimeZone::with_ymd_and_hms(&Utc, 9999, 1, 1, 0, 0, 0).unwrap();
    write_request(&f, &mismatch);
    let mut w = PressSupervisor::with_options(f.options.clone());
    until(&f, &mut w, PressState::Failed);
    let stopped = status(&f).unwrap();
    let refused = wait_for_status(&f, &mut w, mismatch.request_id);
    assert_eq!(
        refused.error_code,
        Some(press::PressErrorCode::ClockMismatch)
    );
    *clock.lock().unwrap() += hours(73);
    steps(&f, &mut w);
    drop(w);
    let j = press::PressJournal::open(&f.root.join("journal")).unwrap();
    for (request, answer) in [(&f.request, &stopped), (&mismatch, &refused)] {
        assert!(j.status(request.request_id).is_none(), "forgotten");
        assert!(
            request_file(&f, request).exists(),
            "the reader's request stays"
        );
        assert_eq!(&status_of(&f, request.request_id).unwrap(), answer);
    }
}
#[test]
fn press_worker_request_from_a_clock_ahead_is_never_accepted() {
    let mut f = fixture("Clock ahead");
    let clock = clocked(&mut f);
    std::fs::remove_file(request_file(&f, &f.request)).unwrap();
    f.options.launch = Arc::new(|_| panic!("a refused request must not be built"));
    let mut ahead = f.request.clone();
    ahead.created_at = Utc::now() + hours(60);
    ahead.expires_at = ahead.created_at + hours(24);
    write_request(&f, &ahead);
    let mut w = PressSupervisor::with_options(f.options.clone());
    let refused = wait_for_status(&f, &mut w, ahead.request_id);
    assert_eq!(
        refused.error_code,
        Some(press::PressErrorCode::ClockMismatch)
    );
    for later in [73, 133] {
        *clock.lock().unwrap() = ahead.created_at - hours(60) + hours(later);
        steps(&f, &mut w);
        assert_eq!(status_of(&f, ahead.request_id).unwrap(), refused);
    }
}
#[test]
fn press_worker_late_request_gets_one_expired_status_and_no_entry() {
    let f = fixture("Late request");
    std::fs::remove_file(request_file(&f, &f.request)).unwrap();
    let mut late = f.request.clone();
    late.created_at = Utc::now() - hours(4 * 24);
    late.expires_at = late.created_at + hours(24);
    write_request(&f, &late);
    let mut w = PressSupervisor::with_options(f.options.clone());
    let expired = wait_for_status(&f, &mut w, late.request_id);
    assert_eq!(expired.state, PressState::Expired);
    assert_eq!(expired.error_code, Some(press::PressErrorCode::Expired));
    // Later scans leave the file alone, whatever it now says.
    let marked = serde_json::to_vec(&press::PressStatus {
        revision: 9,
        ..expired
    })
    .unwrap();
    std::fs::write(status_file(&f, late.request_id), &marked).unwrap();
    steps(&f, &mut w);
    drop(w);
    assert_eq!(
        std::fs::read(status_file(&f, late.request_id)).unwrap(),
        marked
    );
    let j = press::PressJournal::open(&f.root.join("journal")).unwrap();
    assert!(j.status(late.request_id).is_none());
}
#[test]
fn press_worker_status_file_removed_by_hand_is_back_within_five_minutes() {
    let mut f = fixture("Status rewrite");
    let clock = clocked(&mut f);
    press::write_stop(&f.cfg.press_state(), Some(Utc::now().date_naive())).unwrap();
    let mut w = PressSupervisor::with_options(f.options.clone());
    until(&f, &mut w, PressState::Failed);
    let refused = status(&f).unwrap();
    std::fs::remove_file(status_file(&f, f.request.request_id)).unwrap();
    steps(&f, &mut w);
    assert!(
        status(&f).is_none(),
        "unchanged statuses are not rewritten every tick"
    );
    *clock.lock().unwrap() += deckle_digest::chrono::Duration::minutes(5);
    assert_eq!(wait_for_status(&f, &mut w, f.request.request_id), refused);
}
#[test]
fn press_worker_full_request_history_leaves_new_requests_unanswered() {
    use sha2::{Digest, Sha256};
    let f = fixture("Full history");
    // A journal holding 1,000 refusals for this folder, built from one real entry.
    let mut j = press::PressJournal::open(&f.root.join("journal")).unwrap();
    j.register_binding(f.binding.clone(), true, false).unwrap();
    let mut first = f.request.clone();
    first.request_id = Uuid::new_v4();
    j.reject_in(
        &f.binding,
        first,
        "seed",
        press::PressErrorCode::Busy,
        Utc::now(),
    )
    .unwrap();
    drop(j);
    let path = f.root.join("journal/journal.json");
    let mut snapshot: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let mut data: serde_json::Value =
        serde_json::from_str(snapshot["payload"].as_str().unwrap()).unwrap();
    let seed = data["entries"][0].clone();
    for _ in 1..1000 {
        let mut entry = seed.clone();
        let id = Uuid::new_v4().to_string();
        entry["request"]["request_id"] = id.clone().into();
        entry["status"]["request_id"] = id.into();
        data["entries"].as_array_mut().unwrap().push(entry);
    }
    // Their status files, as an earlier run would have left them.
    std::fs::create_dir_all(f.binding.root.join(".deckle-press/status")).unwrap();
    for entry in data["entries"].as_array().unwrap() {
        let id = Uuid::parse_str(entry["status"]["request_id"].as_str().unwrap()).unwrap();
        std::fs::write(
            status_file(&f, id),
            serde_json::to_vec(&entry["status"]).unwrap(),
        )
        .unwrap();
    }
    let payload = serde_json::to_string(&data).unwrap();
    snapshot["sha256"] = Sha256::digest(payload.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
        .into();
    snapshot["payload"] = payload.into();
    let bytes = serde_json::to_vec(&snapshot).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    std::fs::write(f.root.join("journal/journal.backup.json"), &bytes).unwrap();
    let mut options = f.options.clone();
    options.launch = Arc::new(|_| panic!("a full history admits nothing new"));
    let mut w = PressSupervisor::with_options(options);
    // Opening and committing a journal this size takes a while in a debug build.
    let end = Instant::now() + Duration::from_secs(30);
    let mut errors = Vec::new();
    while !errors
        .iter()
        .any(|e: &String| e.contains("request history full"))
    {
        assert!(Instant::now() < end, "{errors:?}");
        errors.extend(steps(&f, &mut w));
    }
    // Reported once, when the folder reached the cap, not on every tick.
    assert!(steps(&f, &mut w).is_empty());
    drop(w);
    assert!(status(&f).is_none(), "a new request is left unanswered");
    let j = press::PressJournal::open(&f.root.join("journal")).unwrap();
    assert!(j.status(f.request.request_id).is_none());
    assert_eq!(j.statuses().len(), 1000);
}
#[test]
fn press_worker_one_folder_in_conflict_does_not_block_another_paper() {
    let f = fixture("Healthy paper");
    // Another folder the journal binds to paper Z, which still has a request waiting, now
    // configured for a different paper: registering it is a conflict.
    let stuck = f.root.parent().unwrap().join("stuck");
    std::fs::create_dir(&stuck).unwrap();
    let z = PressBinding {
        root: stuck.canonicalize().unwrap(),
        producer_id: f.binding.producer_id,
        paper_id: Uuid::new_v4(),
    };
    press::setup::write_manifest(
        &z,
        &ProducerManifest {
            format: "deckle-press".into(),
            version: 1,
            producer_id: z.producer_id,
            paper_id: z.paper_id,
            paper_name: "Z".into(),
            enabled: true,
            operations: vec![PressOperation::FreshFull],
            reading_editions: true,
            updated_at: Utc::now(),
        },
    )
    .unwrap();
    let mut j = press::PressJournal::open(&f.root.join("journal")).unwrap();
    j.register_binding(z.clone(), true, false).unwrap();
    let mut waiting = f.request.clone();
    waiting.request_id = Uuid::new_v4();
    waiting.paper_id = z.paper_id;
    j.accept_in(&z, waiting, "z", Utc::now()).unwrap();
    drop(j);
    let mut other = f.cfg.clone();
    other.title = "Other paper".into();
    other.paper_id = Some(Uuid::new_v4());
    other.export[0].dir = stuck.display().to_string();
    let papers = [f.cfg.clone(), other];
    let mut w = PressSupervisor::with_options(f.options.clone());
    let end = Instant::now() + Duration::from_secs(30);
    let mut errors = Vec::new();
    while !status(&f).is_some_and(|s| s.state == PressState::Published) {
        assert!(
            Instant::now() < end,
            "the healthy paper was blocked: {:?} {errors:?}",
            status(&f)
        );
        if let Err(e) = w.tick(&papers, Utc::now()) {
            errors.push(e);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(w);
    assert!(
        !errors.is_empty(),
        "the conflicting folder's error is reported"
    );
}
fn journal_sequence(f: &Fixture) -> u64 {
    let snapshot: serde_json::Value =
        serde_json::from_slice(&std::fs::read(f.root.join("journal/journal.json")).unwrap())
            .unwrap();
    snapshot["sequence"].as_u64().unwrap()
}
/// Status files go to reader plugins: they never carry a local explanation or path.
fn assert_statuses_carry_no_local_text(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root.join(".deckle-press/status")) else {
        return;
    };
    for entry in entries {
        let text = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        assert!(!text.contains("fresh-paper folder"), "{text}");
        assert!(!text.contains(&*root.to_string_lossy()), "{text}");
    }
}
fn shared_manifest(b: &PressBinding, name: &str) -> ProducerManifest {
    ProducerManifest {
        format: "deckle-press".into(),
        version: 1,
        producer_id: b.producer_id,
        paper_id: b.paper_id,
        paper_name: name.into(),
        enabled: true,
        operations: vec![PressOperation::FreshFull],
        reading_editions: true,
        updated_at: Utc::now(),
    }
}
#[test]
fn press_worker_blocked_folder_move_is_reported_without_reopening_the_journal() {
    blocked_folder_move(deckle_digest::chrono::Duration::seconds(61));
}
#[test]
fn press_worker_blocked_folder_move_is_retried_after_the_clock_goes_back() {
    // A clock set back hours must not hold the folder for those hours.
    blocked_folder_move(-deckle_digest::chrono::Duration::hours(3));
}
/// A move blocked by a stray file, then unblocked; `then` is how the press clock moves
/// before the move must be retried.
fn blocked_folder_move(then: deckle_digest::chrono::Duration) {
    let mut f = fixture("Moving paper");
    let clock = clocked(&mut f);
    // Paper A published an edition into the folder; a stray .DS_Store sits in it.
    let mut j = press::PressJournal::open(&f.root.join("journal")).unwrap();
    let (lease, artifact) = prepare_artifact(&f, &mut j);
    let published = press::publish_result(&mut j, &lease, &artifact, Utc::now).unwrap();
    drop(j);
    let bundle = f.binding.root.join(published.result.unwrap().bundle);
    let stray = bundle.join(".DS_Store");
    std::fs::write(&stray, b"finder").unwrap();
    // The folder is reconnected to paper B; A's statuses are still in the journal.
    let current: ProducerManifest = serde_json::from_slice(
        &std::fs::read(f.binding.root.join(".deckle-press/producer.json")).unwrap(),
    )
    .unwrap();
    press::setup::disconnect_manifest(&f.binding.root, &current).unwrap();
    let b = PressBinding {
        paper_id: Uuid::new_v4(),
        ..f.binding.clone()
    };
    press::setup::write_manifest(&b, &shared_manifest(&b, "Paper B")).unwrap();
    f.cfg.paper_id = Some(b.paper_id);
    deckle_digest::config::save(&f.options.config_path, &f.cfg).unwrap();
    // A second paper, in its own folder, with a request waiting.
    let base = f.root.parent().unwrap().to_owned();
    std::fs::create_dir(base.join("second")).unwrap();
    let second_binding = PressBinding {
        root: base.join("second").canonicalize().unwrap(),
        producer_id: f.binding.producer_id,
        paper_id: Uuid::new_v4(),
    };
    let mut second = f.cfg.clone();
    second.title = "Second paper".into();
    second.paper_id = Some(second_binding.paper_id);
    second.paper = Some(deckle_digest::config::PaperRef {
        slug: "second".into(),
        state_dir: base.join("second-state"),
    });
    second.export[0].dir = second_binding.root.display().to_string();
    let second_path = deckle_digest::papers::paper_path(&f.options.config_path, "second");
    std::fs::create_dir_all(second_path.parent().unwrap()).unwrap();
    deckle_digest::config::save(&second_path, &second).unwrap();
    press::setup::write_manifest(&second_binding, &shared_manifest(&second_binding, "Second"))
        .unwrap();
    let mut request = f.request.clone();
    request.request_id = Uuid::new_v4();
    request.paper_id = second_binding.paper_id;
    let request_path = second_binding.root.join(format!(
        ".deckle-press/requests/{}/{}.json",
        request.reader_id, request.request_id
    ));
    std::fs::create_dir_all(request_path.parent().unwrap()).unwrap();
    std::fs::write(&request_path, serde_json::to_vec(&request).unwrap()).unwrap();
    let second_status = second_binding
        .root
        .join(format!(".deckle-press/status/{}.json", request.request_id));

    let papers = [f.cfg.clone(), second.clone()];
    let mut w = PressSupervisor::with_options(f.options.clone());
    let mut errors = Vec::new();
    let tick = |w: &mut PressSupervisor, errors: &mut Vec<String>| {
        if let Err(e) = w.tick(&papers, Utc::now()) {
            errors.push(e);
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    // The other paper is still admitted and built.
    let end = Instant::now() + Duration::from_secs(30);
    while !std::fs::read(&second_status).is_ok_and(|bytes| {
        serde_json::from_slice::<press::PressStatus>(&bytes)
            .is_ok_and(|s| s.state == PressState::Published)
    }) {
        assert!(Instant::now() < end, "second paper blocked: {errors:?}");
        tick(&mut w, &mut errors);
    }
    // The folder's own error, naming the bundle, reaches the report channel.
    let named = bundle.display().to_string();
    let end = Instant::now() + Duration::from_secs(10);
    while !errors.iter().any(|e| e.contains(&named)) {
        assert!(Instant::now() < end, "no error names {named}: {errors:?}");
        tick(&mut w, &mut errors);
    }
    // Ten more ticks change nothing durable: no reopen, no re-registering, no retry.
    let before = journal_sequence(&f);
    for _ in 0..10 {
        tick(&mut w, &mut errors);
    }
    assert_eq!(journal_sequence(&f), before, "{errors:?}");
    assert_eq!(std::fs::read(&stray).unwrap(), b"finder");
    assert_statuses_carry_no_local_text(&f.binding.root);
    assert_statuses_carry_no_local_text(&second_binding.root);
    // Once the owner removes the stray file, the move is retried within a minute.
    std::fs::remove_file(&stray).unwrap();
    // Not before: a refused move is tried at most once a minute (press clock), so the
    // bundle is checked (hashed) at most once a minute while blocked.
    for _ in 0..5 {
        tick(&mut w, &mut errors);
    }
    assert!(bundle.exists());
    *clock.lock().unwrap() += then;
    let end = Instant::now() + Duration::from_secs(30);
    while bundle.exists() {
        assert!(Instant::now() < end, "move not retried: {errors:?}");
        tick(&mut w, &mut errors);
    }
    drop(w);
    let j = press::PressJournal::open(&f.root.join("journal")).unwrap();
    assert!(j.bindings().contains(&b));
    assert!(j.status(f.request.request_id).is_none());
}
