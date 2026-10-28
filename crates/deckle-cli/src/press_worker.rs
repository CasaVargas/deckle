// SPDX-License-Identifier: GPL-3.0-or-later
//! Local supervisor and isolated child. Shared request bytes never select process arguments.
use deckle_digest::{
    Config, Net,
    chrono::{DateTime, Utc},
    press::{self, *},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Stdio},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
};

pub type WorkerLauncher = Arc<dyn Fn(&Path) -> std::io::Result<Child> + Send + Sync>;
#[derive(Clone)]
pub struct WorkerOptions {
    pub root: PathBuf,
    pub run_lock: PathBuf,
    pub config_path: PathBuf,
    pub launch: WorkerLauncher,
    pub clock: Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>,
}
impl WorkerOptions {
    pub fn production(config_path: PathBuf) -> Result<Self, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        Ok(Self {
            root: press::setup::private_root(),
            run_lock: deckle_core::wordwise::default_cache_dir().join("digest/run.lock"),
            config_path,
            launch: Arc::new(move |job| {
                deckle_core::quiet::command(&exe)
                    .args(["digest", "press-worker", "--job"])
                    .arg(job)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
            }),
            clock: Arc::new(Utc::now),
        })
    }
}
#[derive(Default)]
struct Mailbox {
    latest: Option<(Vec<Config>, u64)>,
    stop: bool,
}
/// `tick` replaces one pending snapshot; all disk/process/publication work is on the
/// supervisor thread. A revision prevents admitting stale work after a newer tick arrives.
pub struct PressSupervisor {
    mailbox: Arc<(Mutex<Mailbox>, Condvar)>,
    revision: Arc<AtomicU64>,
    errors: mpsc::Receiver<String>,
    thread: Option<thread::JoinHandle<()>>,
}
impl PressSupervisor {
    pub fn new(config_path: PathBuf) -> Result<Self, String> {
        Ok(Self::with_options(WorkerOptions::production(config_path)?))
    }
    pub fn with_options(options: WorkerOptions) -> Self {
        let mailbox = Arc::new((Mutex::new(Mailbox::default()), Condvar::new()));
        let revision = Arc::new(AtomicU64::new(0));
        let (tx, errors) = mpsc::sync_channel(1);
        let inbox = mailbox.clone();
        let version = revision.clone();
        let thread = thread::spawn(move || {
            let mut engine = Engine::new(options);
            let mut last: Option<(Vec<Config>, u64)> = None;
            loop {
                let (papers, rev) = {
                    let (lock, wake) = &*inbox;
                    let mut state = lock.lock().unwrap();
                    while state.latest.is_none() && !state.stop {
                        if last.is_some() && engine.journal.is_some() {
                            let (next, timed) = wake
                                .wait_timeout(state, std::time::Duration::from_secs(1))
                                .unwrap();
                            state = next;
                            if timed.timed_out() {
                                break;
                            }
                        } else {
                            state = wake.wait(state).unwrap();
                        }
                    }
                    if state.stop {
                        break;
                    }
                    if let Some(input) = state.latest.take() {
                        last = Some(input);
                    }
                    last.clone().unwrap()
                };
                if let Err(error) = engine.step(&papers, &version, rev) {
                    // Any uncertain snapshot commit requires a fresh locked journal. Stop the
                    // child first, keep run.lock until the next reconciliation or shutdown.
                    let live = engine.running.as_ref().is_some_and(|r| {
                        engine
                            .journal
                            .as_ref()
                            .is_some_and(|j| j.validate_lease(&r.lease, r.lease.started_at).is_ok())
                    });
                    if !live {
                        let _ = engine.stop_child();
                        engine.journal = None;
                        engine.registered.clear();
                    }
                    let _ = tx.try_send(error.to_string());
                }
            }
        });
        Self {
            mailbox,
            revision,
            errors,
            thread: Some(thread),
        }
    }
    pub fn tick(&mut self, papers: &[Config], _now: DateTime<Utc>) -> Result<(), String> {
        let (lock, wake) = &*self.mailbox;
        let mut state = lock.lock().map_err(|_| "press supervisor unavailable")?;
        let rev = self.revision.fetch_add(1, Ordering::SeqCst) + 1;
        state.latest = Some((papers.to_vec(), rev));
        wake.notify_one();
        drop(state);
        match self.errors.try_recv() {
            Ok(e) => Err(e),
            _ => Ok(()),
        }
    }
}
impl Drop for PressSupervisor {
    fn drop(&mut self) {
        let (lock, wake) = &*self.mailbox;
        if let Ok(mut s) = lock.lock() {
            s.stop = true;
            wake.notify_one();
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JobDescriptor {
    version: u32,
    lease: JobLease,
    config: Config,
    config_path: PathBuf,
    // Config intentionally omits runtime paths in ordinary TOML serialization.
    paper: Option<(String, PathBuf)>,
    puzzles: PathBuf,
    shelf: PathBuf,
    almanac: PathBuf,
    reading: Option<PathBuf>,
    work: PathBuf,
    books: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerResult {
    lease: JobLease,
    manifest_sha256: String,
}
fn storage() -> PressError {
    PressError::new(PressErrorCode::StorageUnavailable)
}
fn build_error() -> PressError {
    PressError::new(PressErrorCode::BuildFailed)
}
fn save_new(path: &Path, value: &impl Serialize) -> Result<(), PressError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|_| storage())?;
    file.write_all(&serde_json::to_vec(value).map_err(|_| storage())?)
        .and_then(|_| file.sync_all())
        .map_err(|_| storage())
}
fn read_job(path: &Path) -> Result<JobDescriptor, String> {
    let bad = || "invalid local press worker descriptor".to_string();
    if path.file_name().and_then(|s| s.to_str()) != Some("job.json") {
        return Err(bad());
    }
    let parent = path.parent().ok_or_else(bad)?;
    if parent
        .file_name()
        .and_then(|s| s.to_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .is_none()
    {
        return Err(bad());
    }
    let jobs = parent.parent().ok_or_else(bad)?;
    if jobs.file_name().and_then(|s| s.to_str()) != Some("jobs") {
        return Err(bad());
    }
    let root = jobs.parent().ok_or_else(bad)?;
    let bytes = press::scan::read_bounded(
        root,
        path.strip_prefix(root).map_err(|_| bad())?,
        1024 * 1024,
    )
    .map_err(|_| bad())?;
    let job: JobDescriptor = serde_json::from_slice(&bytes).map_err(|_| bad())?;
    if job.version != 1
        || job.work != parent.join("attempt")
        || job.config.paper_id != Some(job.lease.paper_id)
        || !(1..=3).contains(&job.lease.attempt)
        || job.lease.generation != u64::from(job.lease.attempt)
        || job.work.starts_with(&job.lease.root)
    {
        return Err(bad());
    }
    Ok(job)
}
/// Production child loads source authorization read-only. Refreshed jars stay in memory;
/// this entry point never calls save_jars or any scheduler/delivery/journal operation.
pub fn run_job(path: &Path) -> Result<(), String> {
    let job = read_job(path)?;
    let net = crate::household_net_quiet(&job.config_path, &job.config);
    build_job(path, job, &net)
}
/// Same child pipeline with an injected source transport, for local process fixtures.
pub fn run_job_with_net(path: &Path, net: &dyn Net) -> Result<(), String> {
    let job = read_job(path)?;
    build_job(path, job, net)
}
fn build_job(path: &Path, mut job: JobDescriptor, net: &dyn Net) -> Result<(), String> {
    job.config.paper = job
        .paper
        .map(|(slug, state_dir)| deckle_digest::config::PaperRef { slug, state_dir });
    job.config.puzzle_path = Some(job.puzzles);
    job.config.shelf_path = Some(job.shelf);
    job.config.almanac_path = Some(job.almanac);
    job.config.reading.queue_path = job.reading;
    let context = FreshBuildContext {
        identity: FreshIdentity {
            paper_id: job.lease.paper_id,
            edition_id: job.lease.edition_id,
        },
        started_at: job.lease.started_at,
        work_dir: job.work,
        books: job.books,
    };
    let artifact = build_fresh(&job.config, &context, net, &crate::gather_options())
        .map_err(|e| e.to_string())?;
    save_new(
        &path.with_file_name("result.json"),
        &WorkerResult {
            lease: job.lease,
            manifest_sha256: artifact.manifest_sha256,
        },
    )
    .map_err(|e| e.to_string())
}
struct Running {
    child: Child,
    lease: JobLease,
    descriptor: PathBuf,
}
struct Engine {
    options: WorkerOptions,
    journal: Option<PressJournal>,
    producer: Option<Uuid>,
    registered: HashMap<PathBuf, (PressBinding, bool, bool)>,
    cursors: HashMap<PathBuf, press::scan::ScanCursor>,
    scan_index: usize,
    reconciled: HashMap<PathBuf, (String, bool)>,
    running: Option<Running>,
    lock: Option<File>,
}
impl Engine {
    fn new(options: WorkerOptions) -> Self {
        Self {
            options,
            journal: None,
            producer: None,
            registered: HashMap::new(),
            cursors: HashMap::new(),
            scan_index: 0,
            reconciled: HashMap::new(),
            running: None,
            lock: None,
        }
    }
    fn stop_child(&mut self) -> Result<(), PressError> {
        if let Some(running) = self.running.as_mut() {
            // Even if kill reports that the process has exited, wait must confirm/reap it.
            let _ = running.child.kill();
            running.child.wait().map_err(|_| storage())?;
        }
        if let Some(running) = self.running.take() {
            cleanup_attempt(&running.descriptor)?;
        }
        Ok(())
    }
    fn lock(&mut self) -> Result<bool, PressError> {
        if self.lock.is_some() {
            return Ok(true);
        }
        fs::create_dir_all(self.options.run_lock.parent().ok_or_else(storage)?)
            .map_err(|_| storage())?;
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.options.run_lock)
            .map_err(|_| storage())?;
        match file.try_lock() {
            Ok(()) => {
                self.lock = Some(file);
                Ok(true)
            }
            Err(fs::TryLockError::WouldBlock) => Ok(false),
            Err(_) => Err(storage()),
        }
    }
    fn initialize(&mut self, papers: &[Config]) -> Result<bool, PressError> {
        if self.journal.is_some() {
            return Ok(true);
        }
        if !papers
            .iter()
            .any(|p| p.export.iter().any(|e| e.press_enabled))
            && !self.options.root.join("journal").exists()
        {
            return Ok(false);
        }
        // Refuse local state inside an export, and exports inside local state.
        let parent = self.options.root.parent().ok_or_else(storage)?;
        fs::create_dir_all(parent).map_err(|_| storage())?;
        let prospective = parent
            .canonicalize()
            .map_err(|_| storage())?
            .join(self.options.root.file_name().ok_or_else(storage)?);
        for e in papers.iter().flat_map(|p| &p.export) {
            if let Ok(root) = deckle_digest::inbox::expand_home(&e.dir).canonicalize()
                && (prospective.starts_with(&root) || root.starts_with(&prospective))
            {
                return Err(PressError::new(PressErrorCode::ProtocolConflict));
            }
        }
        if self.options.root.exists()
            && self.options.root.canonicalize().map_err(|_| storage())? != prospective
        {
            return Err(storage());
        }
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(&prospective) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(_) => return Err(storage()),
        }
        self.options.root = prospective;
        self.producer = Some(press::setup::producer_identity(
            &self.options.root.join("producer.json"),
        )?);
        self.journal = Some(PressJournal::open(&self.options.root.join("journal"))?);
        Ok(true)
    }
    fn policies(
        &mut self,
        papers: &[Config],
        now: DateTime<Utc>,
    ) -> Result<Vec<(PressBinding, Config, usize)>, PressError> {
        let producer = self.producer.unwrap();
        let mut bindings = Vec::new();
        for paper in papers {
            let Some(id) = paper.paper_id else { continue };
            for (index, export) in paper
                .export
                .iter()
                .enumerate()
                .filter(|(_, e)| e.format == deckle_digest::config::ExportFormat::Bundle)
            {
                let Ok(root) = deckle_digest::inbox::expand_home(&export.dir).canonicalize() else {
                    continue;
                };
                if root.starts_with(&self.options.root) || self.options.root.starts_with(&root) {
                    return Err(PressError::new(PressErrorCode::ProtocolConflict));
                }
                let binding = PressBinding {
                    root,
                    producer_id: producer,
                    paper_id: id,
                };
                let known = self.journal.as_ref().unwrap().bindings();
                let stored = known.iter().find(|b| **b == binding);
                let owned = press::scan::read_control(
                    &binding.root,
                    Path::new(".deckle-press/producer.json"),
                )
                .ok()
                .and_then(|bytes| press::decode_manifest(&bytes, &binding).ok())
                .is_some();
                if !export.press_enabled && stored.is_none() && !owned {
                    continue;
                }
                // A moved/new root for an established paper needs explicit setup ownership.
                // Do not turn a raw path edit into an advertisement in an unrelated folder.
                if stored.is_none() && known.iter().any(|b| b.paper_id == id) && !owned {
                    continue;
                }
                // Reconcile only opt-in state at startup/change. Already inside the running
                // watcher, so the service-start prerequisite is satisfied without OS writes.
                let stamp = (export.dir.clone(), export.press_enabled);
                if self.reconciled.get(&binding.root) != Some(&stamp) {
                    let path = if paper.slug() == deckle_digest::papers::MAIN {
                        self.options.config_path.clone()
                    } else {
                        deckle_digest::papers::paper_path(&self.options.config_path, paper.slug())
                    };
                    let mut copy = paper.clone();
                    press::setup::reconcile_with_watcher(
                        papers,
                        &mut copy,
                        index,
                        producer,
                        &path,
                        || Ok(()),
                    )?;
                    self.reconciled.insert(binding.root.clone(), stamp);
                }
                let path = if paper.slug() == deckle_digest::papers::MAIN {
                    self.options.config_path.clone()
                } else {
                    deckle_digest::papers::paper_path(&self.options.config_path, paper.slug())
                };
                let persisted_enabled = Config::load(&path).is_ok_and(|current| {
                    current.paper_id == paper.paper_id
                        && current.export.get(index).is_some_and(|saved| {
                            saved.dir == export.dir
                                && saved.format == export.format
                                && saved.press_enabled
                        })
                });
                let enabled = persisted_enabled
                    && press::setup::binding_is_enabled(papers, paper, index, producer);
                let today = now
                    .with_timezone(&deckle_digest::tz::local_tz())
                    .date_naive();
                let stopped = press::read_stop(&paper.press_state(), today).is_some();
                bindings.push((binding.clone(), paper.clone(), index));
                let policy = (binding.clone(), enabled, stopped);
                if self.registered.get(&binding.root) != Some(&policy) {
                    self.journal
                        .as_mut()
                        .unwrap()
                        .register_binding(binding, enabled, stopped)?;
                    self.registered.insert(policy.0.root.clone(), policy);
                }
            }
        }
        // A removed/invalid mapping may still have durable queued work. Explicitly disable
        // its stored binding before admission; a running job keeps its original snapshot.
        for b in self.journal.as_ref().unwrap().bindings() {
            if !bindings.iter().any(|(current, _, _)| *current == b) {
                self.journal
                    .as_mut()
                    .unwrap()
                    .set_binding_policy(&b, false, false)?;
                self.registered.insert(b.root.clone(), (b, false, false));
            }
        }
        Ok(bindings)
    }
    fn step(
        &mut self,
        papers: &[Config],
        revision: &AtomicU64,
        rev: u64,
    ) -> Result<(), PressError> {
        if !self.initialize(papers)? {
            return Ok(());
        }
        let now = (self.options.clock)();
        if let Some(running) = self.running.as_mut() {
            if now >= running.lease.deadline {
                // Fence first, then terminate and reap before releasing the build slot.
                let failed = self.journal.as_mut().unwrap().fail(
                    &running.lease,
                    PressErrorCode::TimedOut,
                    now,
                );
                self.stop_child()?;
                failed?;
            } else if let Some(exit) = running.child.try_wait().map_err(|_| storage())? {
                let running = self.running.take().unwrap();
                if exit.success() {
                    let parent = running.descriptor.parent().unwrap();
                    let bytes = press::scan::read_control(parent, Path::new("result.json"))?;
                    let result: WorkerResult =
                        serde_json::from_slice(&bytes).map_err(|_| build_error())?;
                    if result.lease != running.lease {
                        return Err(build_error());
                    }
                    let artifact = FreshArtifact {
                        bundle: parent
                            .join("attempt")
                            .join(format!("{}.deckle", running.lease.edition_id)),
                        manifest_sha256: result.manifest_sha256,
                    };
                    press::publish_result(
                        self.journal.as_mut().unwrap(),
                        &running.lease,
                        &artifact,
                        || (self.options.clock)(),
                    )?;
                }
                cleanup_attempt(&running.descriptor)?;
                // A failed child remains active for bounded recovery below.
            }
        }
        let policy_result = self.policies(papers, now);
        let policy_error = policy_result.as_ref().err().cloned();
        let bindings = policy_result.unwrap_or_default();
        // One binding per cycle bounds total request candidates globally to 100.
        let selected = if bindings.is_empty() {
            None
        } else {
            let index = self.scan_index % bindings.len();
            self.scan_index = self.scan_index.wrapping_add(1);
            bindings.get(index)
        };
        if let Some((binding, _, _)) = selected {
            for event in press::scan::scan_requests(
                binding,
                self.cursors.entry(binding.root.clone()).or_default(),
                now,
            ) {
                if let press::scan::ScanEvent::Diagnostic { ref path, code } = event {
                    eprintln!(
                        "fresh-paper control {}: {}",
                        path.display(),
                        PressError::new(code)
                    );
                }
                if let press::scan::ScanEvent::Request {
                    request,
                    hash,
                    rejection,
                } = event
                {
                    let journal = self.journal.as_mut().unwrap();
                    let result = if let Some(code) = rejection {
                        journal.reject_in(binding, request.clone(), &hash, code, now)
                    } else {
                        journal.accept_in(binding, request.clone(), &hash, now)
                    };
                    if let Err(e) = result {
                        match e.code {
                            PressErrorCode::Busy
                            | PressErrorCode::Disabled
                            | PressErrorCode::PressesStopped
                            | PressErrorCode::Expired
                            | PressErrorCode::ClockMismatch => {
                                journal.reject_in(binding, request, &hash, e.code, now)?;
                            }
                            PressErrorCode::ProtocolConflict | PressErrorCode::Unsupported => (),
                            _ => return Err(e),
                        }
                    }
                }
            }
        }
        if self.running.is_none() {
            if let Some(previous) = self.journal.as_ref().unwrap().active() {
                let recovery_now = (self.options.clock)();
                let recover = if self.lock()? {
                    match press::recover_result(
                        self.journal.as_mut().unwrap(),
                        &previous,
                        recovery_now,
                    ) {
                        Ok(Some(_)) => false,
                        Ok(None) => true,
                        Err(_) if self.journal.as_ref().unwrap().active().is_none() => false,
                        Err(_) if recovery_now >= previous.deadline => {
                            // No child remains. Fence durably before releasing the global slot,
                            // even when the shared root cannot confirm an output intent.
                            self.journal.as_mut().unwrap().fail(
                                &previous,
                                PressErrorCode::TimedOut,
                                recovery_now,
                            )?;
                            self.lock = None;
                            false
                        }
                        Err(error) => return Err(error),
                    }
                } else {
                    false
                };
                if recover {
                    let lease = match self
                        .journal
                        .as_mut()
                        .unwrap()
                        .recover(&previous, (self.options.clock)())
                    {
                        Ok(l) => Some(l),
                        Err(e)
                            if matches!(
                                e.code,
                                PressErrorCode::TimedOut | PressErrorCode::BuildFailed
                            ) =>
                        {
                            None
                        }
                        Err(e) => return Err(e),
                    };
                    if let Some(lease) = lease {
                        if let Some(artifact) =
                            press::archived_artifact(self.journal.as_ref().unwrap(), &lease)?
                        {
                            press::publish_result(
                                self.journal.as_mut().unwrap(),
                                &lease,
                                &artifact,
                                || (self.options.clock)(),
                            )?;
                        } else if let Some((_, cfg, index)) = bindings
                            .iter()
                            .find(|(b, _, _)| b.paper_id == lease.paper_id && b.root == lease.root)
                        {
                            self.spawn(lease, cfg, *index)?;
                        } else {
                            self.journal.as_mut().unwrap().fail(
                                &lease,
                                PressErrorCode::BuildFailed,
                                (self.options.clock)(),
                            )?;
                        }
                    }
                }
            } else if policy_error.is_none()
                && revision.load(Ordering::SeqCst) == rev
                && self.lock()?
            {
                // Lock acquisition and the earlier scan may have taken time. Refresh
                // current policy only after holding the lock, and never admit a snapshot
                // superseded by a newer tick during either operation.
                let policy_now = (self.options.clock)();
                if revision.load(Ordering::SeqCst) == rev {
                    let bindings = self.policies(papers, policy_now)?;
                    let admission_now = (self.options.clock)();
                    if revision.load(Ordering::SeqCst) == rev
                        && let Some(lease) = self.journal.as_mut().unwrap().admit(admission_now)?
                    {
                        if let Some((_, cfg, index)) = bindings
                            .iter()
                            .find(|(b, _, _)| b.paper_id == lease.paper_id && b.root == lease.root)
                        {
                            self.spawn(lease, cfg, *index)?;
                        } else {
                            self.journal.as_mut().unwrap().fail(
                                &lease,
                                PressErrorCode::BuildFailed,
                                (self.options.clock)(),
                            )?;
                        }
                    }
                }
            }
        }

        if self.running.is_none() && self.journal.as_ref().unwrap().active().is_none() {
            self.lock = None;
        }
        let statuses = press::flush_statuses(self.journal.as_mut().unwrap());
        let retention =
            press::prune_results(self.journal.as_mut().unwrap(), (self.options.clock)());
        statuses.and(retention)?;
        if let Some(error) = policy_error {
            return Err(error);
        }
        Ok(())
    }
    fn spawn(&mut self, lease: JobLease, cfg: &Config, index: usize) -> Result<(), PressError> {
        let jobs = self.options.root.join("jobs");
        fs::create_dir_all(&jobs).map_err(|_| storage())?;
        let dir = jobs.join(Uuid::new_v4().to_string());
        fs::create_dir(&dir).map_err(|_| storage())?;
        let descriptor = dir.join("job.json");
        let job = JobDescriptor {
            version: 1,
            lease: lease.clone(),
            config: cfg.clone(),
            config_path: self.options.config_path.clone(),
            paper: cfg
                .paper
                .as_ref()
                .map(|p| (p.slug.clone(), p.state_dir.clone())),
            puzzles: cfg.puzzle_store(),
            shelf: cfg.shelf_store(),
            almanac: cfg.almanac_cache(),
            reading: cfg.reading.queue_path.clone(),
            work: dir.join("attempt"),
            books: cfg.export[index].books,
        };
        save_new(&descriptor, &job)?;
        let child = (self.options.launch)(&descriptor).map_err(|_| build_error())?;
        self.running = Some(Running {
            child,
            lease,
            descriptor,
        });
        Ok(())
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        if self.stop_child().is_err() && self.running.is_some() {
            // Exceptional OS wait failure: keep the lock until this parent process exits.
            // Releasing it without confirming termination would permit a concurrent build.
            if let Some(lock) = self.lock.take() {
                std::mem::forget(lock);
            }
        }
    }
}

/// Only a positively reaped child's unique staging is removable. Neither shared outputs
/// nor archived results are under this path, and private descriptor reads reject links.
fn cleanup_attempt(descriptor: &Path) -> Result<(), PressError> {
    let job = read_job(descriptor).map_err(|_| storage())?;
    match fs::symlink_metadata(&job.work) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(meta)
            if meta.file_type().is_dir()
                && job.work.canonicalize().ok().as_deref() == Some(job.work.as_path()) =>
        {
            fs::remove_dir_all(&job.work).map_err(|_| storage())
        }
        _ => Err(storage()),
    }
}
