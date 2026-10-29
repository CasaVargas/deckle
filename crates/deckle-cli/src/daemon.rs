// SPDX-License-Identifier: GPL-3.0-or-later
//! `digest daemon` (Linux Docker design): a clock and a supervisor for a container, where
//! there is no launchd, Task Scheduler or cron. Each paper's edition is `digest run` and the
//! device watcher is `digest watch`, both as children: no second implementation of either.

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use deckle_digest::Config;
use deckle_digest::chrono::{DateTime, NaiveDate, Utc};
use deckle_digest::chrono_tz::Tz;
use deckle_digest::daemon::{CATCH_UP_UNTIL, DaemonState, Decision, RETRY_EVERY_MIN, SkipWhy};
use deckle_digest::schedule::{self, Schedule};

/// A watcher that exits is started again after this long.
const WATCH_RESTART: Duration = Duration::from_secs(30);
/// On SIGTERM, a running edition gets this long to finish before it's killed.
const STOP_GRACE: Duration = Duration::from_secs(60);
/// How often a wait looks at the stop flag and the children.
const POLL: Duration = Duration::from_millis(200);

/// Tests only: the daemon's clock, RFC 3339 (like `DECKLE_SCHEDULE_DRY_RUN`).
const FAKE_NOW: &str = "DECKLE_DAEMON_FAKE_NOW";
/// One tick, then exit: tests and the image's smoke test. No watcher is started.
const ONCE: &str = "DECKLE_DAEMON_ONCE";

/// Runs until SIGTERM or SIGINT. `papers` is the set read at start; `config` the explicit
/// `--config`, carried to the children.
pub fn run(
    path: &Path,
    config: Option<&Path>,
    papers: Vec<Config>,
    tick: u64,
) -> Result<(), String> {
    let stop = Arc::new(AtomicBool::new(false));
    for sig in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        // A second signal while stopping exits at once.
        signal_hook::flag::register_conditional_shutdown(sig, 1, Arc::clone(&stop))
            .and_then(|_| signal_hook::flag::register(sig, Arc::clone(&stop)))
            .map_err(|e| format!("can't handle signals: {e}"))?;
    }
    let exe = std::env::current_exe().map_err(|e| format!("can't find the deckle binary: {e}"))?;
    let config = config.map(|c| c.canonicalize().unwrap_or_else(|_| c.to_path_buf()));
    let once = std::env::var_os(ONCE).is_some_and(|v| !v.is_empty());
    let zone = zone();
    let state_path = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("daemon.json");
    eprintln!(
        "deckle {} daemon: config {}, zone {zone}, checking every {tick}s",
        env!("CARGO_PKG_VERSION"),
        path.display()
    );
    let mut d = Daemon {
        path: path.to_path_buf(),
        config,
        zone,
        jobs: Vec::new(),
        papers,
        papers_error: None,
        state: load_state(&state_path),
        state_path,
        said: BTreeSet::new(),
        stop: Arc::clone(&stop),
        exe,
    };
    d.say_schedules();
    check_paths(&d.papers);

    let mut watch = (!once).then(|| Watch::new(&d.exe, d.config.as_deref()));
    let every = Duration::from_secs(tick.max(1));
    loop {
        d.tick();
        if once || stop.load(Ordering::SeqCst) {
            break;
        }
        let next = Instant::now() + every;
        while Instant::now() < next && !stop.load(Ordering::SeqCst) {
            if let Some(w) = watch.as_mut() {
                w.supervise();
            }
            std::thread::sleep(POLL);
        }
        if stop.load(Ordering::SeqCst) {
            break;
        }
    }
    if let Some(mut w) = watch {
        w.stop();
    }
    if stop.load(Ordering::SeqCst) {
        eprintln!("stopped");
    }
    Ok(())
}

/// The start-up check of the config, Morgue and export folders: Task 5 fills it in.
fn check_paths(_papers: &[Config]) {}

/// The zone the clock reads. `TZ` first: `digest run` (chrono's local time) follows it, while
/// the machine zone on Linux is `/etc/localtime`, which a container's `TZ` doesn't change.
fn zone() -> Tz {
    std::env::var("TZ")
        .ok()
        .and_then(|z| z.trim_start_matches(':').parse().ok())
        .unwrap_or_else(deckle_digest::tz::local_tz)
}

fn now(zone: Tz) -> DateTime<Tz> {
    std::env::var(FAKE_NOW)
        .ok()
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|t| t.with_timezone(&zone))
        .unwrap_or_else(|| Utc::now().with_timezone(&zone))
}

/// The saved state; a file that's there but unreadable is said, then started over.
fn load_state(path: &Path) -> DaemonState {
    let bytes = std::fs::read(path).unwrap_or_default();
    if !bytes.is_empty() && serde_json::from_slice::<DaemonState>(&bytes).is_err() {
        eprintln!("{}: daemon.json unreadable; starting fresh", path.display());
    }
    DaemonState::load(path)
}

struct Daemon {
    path: PathBuf,
    config: Option<PathBuf>,
    zone: Tz,
    exe: PathBuf,
    papers: Vec<Config>,
    /// The last error re-reading the papers, said once.
    papers_error: Option<String>,
    jobs: Vec<Schedule>,
    state: DaemonState,
    state_path: PathBuf,
    /// (paper, day, what) already said today, so a skip is logged once.
    said: BTreeSet<(String, NaiveDate, &'static str)>,
    stop: Arc<AtomicBool>,
}

impl Daemon {
    fn tick(&mut self) {
        self.reload();
        let now = now(self.zone);
        let today = now.date_naive();
        self.said.retain(|(_, d, _)| *d == today);
        for job in self.jobs.clone() {
            if self.stop.load(Ordering::SeqCst) {
                return;
            }
            let Some(cfg) = self.papers.iter().find(|p| p.slug() == job.slug) else {
                continue;
            };
            // The sidecar is written last: it means the edition exists, whatever printed it.
            let exists = deckle_morgue::Morgue::from_config(cfg)
                .paths(today)
                .1
                .is_file();
            match deckle_digest::daemon::decide(&job, now, &self.state, exists) {
                Decision::Run => self.print(&job, now),
                Decision::Done => self.once(&job.slug, today, "done", || {
                    format!("{}: printed {today}", job.slug)
                }),
                Decision::Skip(SkipWhy::PastNoon) => self.once(&job.slug, today, "noon", || {
                    format!(
                        "{}: today's paper skipped — missed {} and it's past {CATCH_UP_UNTIL}:00",
                        job.slug,
                        at(&job)
                    )
                }),
                Decision::Skip(SkipWhy::NotToday) => self.once(&job.slug, today, "day", || {
                    format!("{}: no paper today ({})", job.slug, days(&job))
                }),
                Decision::Skip(SkipWhy::RetryLater) | Decision::Wait => {}
            }
        }
    }

    /// A new or edited paper applies at the next tick; a file that fails keeps the last good
    /// set, and its error is said once (as `digest watch` does).
    fn reload(&mut self) {
        match deckle_digest::papers::load_all(&self.path) {
            Ok(l) => {
                self.papers = l.papers;
                self.papers_error = None;
            }
            Err(e) if self.papers_error.as_ref() != Some(&e) => {
                eprintln!("papers: {e}; keeping the last good set");
                self.papers_error = Some(e);
            }
            Err(_) => {}
        }
        let jobs = schedule::jobs_for(&self.papers, &self.exe, self.config.as_deref());
        if jobs != self.jobs {
            self.say_schedules();
        }
    }

    /// Each paper's schedule, and the papers the daemon leaves alone.
    fn say_schedules(&mut self) {
        self.jobs = schedule::jobs_for(&self.papers, &self.exe, self.config.as_deref());
        for p in &self.papers {
            match self.jobs.iter().find(|j| j.slug == p.slug()) {
                Some(j) => eprintln!("{} {} {}", j.slug, at(j), days(j)),
                None => eprintln!("{}: not scheduled (no [schedule] at)", p.slug()),
            }
        }
    }

    fn once(
        &mut self,
        slug: &str,
        today: NaiveDate,
        what: &'static str,
        line: impl FnOnce() -> String,
    ) {
        if self.said.insert((slug.to_owned(), today, what)) {
            eprintln!("{}", line());
        }
    }

    /// One edition, through `digest run`, waited for before anything else is decided: two
    /// copies can't be in flight. Printed is recorded only on success.
    fn print(&mut self, job: &Schedule, now: DateTime<Tz>) {
        let today = now.date_naive();
        eprintln!("{}: printing (due {})", job.slug, at(job));
        let status = spawn(&self.exe, &job.args("run"), &job.slug)
            .and_then(|mut c| c.wait_or_stop(&self.stop));
        match status {
            Ok(s) if s.success() => {
                self.state.printed.insert(job.slug.clone(), today);
                self.state.last_attempt.remove(&job.slug);
                self.said.insert((job.slug.clone(), today, "done"));
                eprintln!("{}: printed {today}", job.slug);
            }
            // Stopped by us: not a failure to wait out after a restart.
            Ok(_) if self.stop.load(Ordering::SeqCst) => return,
            r => {
                let why = match r {
                    Ok(s) => s.to_string(),
                    Err(e) => e.to_string(),
                };
                self.state
                    .last_attempt
                    .insert(job.slug.clone(), now.with_timezone(&Utc));
                eprintln!(
                    "{}: digest run failed ({why}); trying again in {RETRY_EVERY_MIN} min",
                    job.slug
                );
            }
        }
        if let Err(e) = self.state.save(&self.state_path) {
            eprintln!("{}: {e}", self.state_path.display());
        }
    }
}

fn at(job: &Schedule) -> String {
    format!("{:02}:{:02}", job.hour, job.minute)
}

fn days(job: &Schedule) -> String {
    if job.days.is_empty() {
        "every day".to_owned()
    } else {
        job.days
            .iter()
            .map(|d| d.key())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// A child whose output comes through line by line, each prefixed `[<name>] `.
struct Prefixed {
    child: Child,
    pipes: Vec<JoinHandle<()>>,
}

fn spawn(exe: &Path, args: &[String], name: &str) -> std::io::Result<Prefixed> {
    let mut child = Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut pipes = Vec::new();
    if let Some(out) = child.stdout.take() {
        pipes.push(pass_through(out, name, false));
    }
    if let Some(err) = child.stderr.take() {
        pipes.push(pass_through(err, name, true));
    }
    Ok(Prefixed { child, pipes })
}

fn pass_through(from: impl Read + Send + 'static, name: &str, to_stderr: bool) -> JoinHandle<()> {
    let prefix = format!("[{name}] ");
    std::thread::spawn(move || {
        let mut from = BufReader::new(from);
        let mut line = Vec::new();
        while from.read_until(b'\n', &mut line).is_ok_and(|n| n > 0) {
            let text = String::from_utf8_lossy(&line);
            let text = text.trim_end_matches(['\n', '\r']);
            let _ = if to_stderr {
                writeln!(std::io::stderr().lock(), "{prefix}{text}")
            } else {
                writeln!(std::io::stdout().lock(), "{prefix}{text}")
            };
            line.clear();
        }
    })
}

impl Prefixed {
    /// Waits for the child; once `stop` is set, asks it to stop and gives it [`STOP_GRACE`].
    fn wait_or_stop(&mut self, stop: &AtomicBool) -> std::io::Result<ExitStatus> {
        let mut deadline = None;
        let status = loop {
            if let Some(s) = self.child.try_wait()? {
                break s;
            }
            if stop.load(Ordering::SeqCst) && deadline.is_none() {
                terminate(&mut self.child);
                deadline = Some(Instant::now() + STOP_GRACE);
            }
            if deadline.is_some_and(|d| Instant::now() >= d) {
                let _ = self.child.kill();
                break self.child.wait()?;
            }
            std::thread::sleep(POLL);
        };
        for p in self.pipes.drain(..) {
            let _ = p.join();
        }
        Ok(status)
    }
}

/// SIGTERM on Unix, so the child can end as it would under a service manager; Windows has no
/// such signal, so it's killed.
fn terminate(child: &mut Child) {
    #[cfg(unix)]
    {
        if let Ok(pid) = libc::pid_t::try_from(child.id()) {
            // SAFETY: kill(2) on our own child's pid; no memory is touched.
            unsafe {
                libc::kill(pid, libc::SIGTERM);
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
}

/// `digest watch`, kept running: when it exits it's started again after [`WATCH_RESTART`].
struct Watch {
    exe: PathBuf,
    args: Vec<String>,
    child: Option<Prefixed>,
    restart_at: Option<Instant>,
}

impl Watch {
    fn new(exe: &Path, config: Option<&Path>) -> Watch {
        let mut args = vec!["digest".to_owned()];
        if let Some(c) = config {
            args.push("--config".into());
            args.push(c.to_string_lossy().into_owned());
        }
        args.push("watch".into());
        let mut w = Watch {
            exe: exe.to_path_buf(),
            args,
            child: None,
            restart_at: Some(Instant::now()),
        };
        w.supervise();
        w
    }

    fn supervise(&mut self) {
        if let Some(c) = self.child.as_mut() {
            match c.child.try_wait() {
                Ok(None) => return,
                Ok(Some(s)) => eprintln!(
                    "watch: exited ({s}); restarting in {}s",
                    WATCH_RESTART.as_secs()
                ),
                Err(e) => eprintln!("watch: {e}; restarting in {}s", WATCH_RESTART.as_secs()),
            }
            self.child = None;
            self.restart_at = Some(Instant::now() + WATCH_RESTART);
            return;
        }
        if self.restart_at.is_some_and(|t| Instant::now() >= t) {
            match spawn(&self.exe, &self.args, "watch") {
                Ok(c) => {
                    self.child = Some(c);
                    self.restart_at = None;
                }
                Err(e) => {
                    eprintln!(
                        "watch: can't start ({e}); trying again in {}s",
                        WATCH_RESTART.as_secs()
                    );
                    self.restart_at = Some(Instant::now() + WATCH_RESTART);
                }
            }
        }
    }

    fn stop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let stop = AtomicBool::new(true);
            let _ = c.wait_or_stop(&stop);
        }
    }
}
