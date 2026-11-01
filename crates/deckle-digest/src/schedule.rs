// SPDX-License-Identifier: GPL-3.0-or-later
//! Scheduling (design §5b): each paper's edition at its time (many papers §5), and the watcher
//! from login on. macOS: LaunchAgents. Windows: Task Scheduler tasks. Linux: printed
//! instructions. The main paper keeps today's job names; another paper's carry its slug.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use crate::config::{Config, Day};
use crate::esc;
use crate::papers::MAIN;

pub const DIGEST_LABEL: &str = "com.casavargas.deckle.digest";
pub const WATCH_LABEL: &str = "com.casavargas.deckle.watch";
const TASK_FOLDER: &str = "Deckle";

/// What one paper's edition job runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Schedule {
    pub hour: u8,
    pub minute: u8,
    /// Empty: every day.
    pub days: Vec<Day>,
    /// The paper (`main` for digest.toml's).
    pub slug: String,
    /// The `deckle` binary, absolute.
    pub exe: PathBuf,
    /// `--config`, carried into the jobs.
    pub config: Option<PathBuf>,
}

/// What's installed for one paper.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PaperStatus {
    pub slug: String,
    /// `HH:MM`.
    pub at: Option<String>,
    pub days: Vec<Day>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Status {
    /// `HH:MM` of the main paper's installed edition job.
    pub at: Option<String>,
    pub watch: bool,
    /// The binary the installed jobs run.
    pub exe: Option<String>,
    /// Every paper's installed edition job, main first.
    pub papers: Vec<PaperStatus>,
}

/// A paper's LaunchAgent label: `…digest` for the main paper, `…digest.<slug>` for another.
pub fn label(slug: &str) -> String {
    match slug {
        MAIN => DIGEST_LABEL.to_owned(),
        s => format!("{DIGEST_LABEL}.{s}"),
    }
}

/// A paper's Windows task: `\Deckle\Digest`, or `\Deckle\Digest-<slug>`.
pub fn task_name_for(slug: &str) -> String {
    match slug {
        MAIN => format!("\\{TASK_FOLDER}\\Digest"),
        s => format!("\\{TASK_FOLDER}\\Digest-{s}"),
    }
}

/// launchd's weekday: 0 is Sunday.
fn weekday(d: Day) -> u8 {
    match d {
        Day::Sun => 0,
        Day::Mon => 1,
        Day::Tue => 2,
        Day::Wed => 3,
        Day::Thu => 4,
        Day::Fri => 5,
        Day::Sat => 6,
    }
}

/// Task Scheduler's day element.
fn task_day(d: Day) -> &'static str {
    match d {
        Day::Mon => "Monday",
        Day::Tue => "Tuesday",
        Day::Wed => "Wednesday",
        Day::Thu => "Thursday",
        Day::Fri => "Friday",
        Day::Sat => "Saturday",
        Day::Sun => "Sunday",
    }
}

/// One job per paper whose file has `[schedule] at` (many papers §5).
pub fn jobs_for(papers: &[Config], exe: &Path, config: Option<&Path>) -> Vec<Schedule> {
    papers
        .iter()
        .filter_map(|p| {
            let (hour, minute) = parse_at(p.schedule.at.as_deref()?).ok()?;
            let mut days = p.schedule.days.clone();
            days.sort();
            days.dedup();
            Some(Schedule {
                hour,
                minute,
                days,
                slug: p.slug().to_owned(),
                exe: exe.to_path_buf(),
                config: config.map(Path::to_path_buf),
            })
        })
        .collect()
}

/// Installed per-paper labels (`…digest.<slug>`) no wanted job has. Never the main paper's:
/// a main paper with no `[schedule] at` keeps whatever is installed.
pub fn stale_labels(installed: &[String], wanted: &[Schedule]) -> Vec<String> {
    installed
        .iter()
        .filter(|l| l.starts_with(&format!("{DIGEST_LABEL}.")))
        .filter(|l| !wanted.iter().any(|s| label(&s.slug) == **l))
        .cloned()
        .collect()
}

/// `HH:MM` (24-hour) → (hour, minute).
pub fn parse_at(s: &str) -> Result<(u8, u8), String> {
    let bad = || format!("--at expects a 24-hour time like 05:30, got {s:?}");
    let (h, m) = s.trim().split_once(':').ok_or_else(bad)?;
    let (h, m): (u8, u8) = (h.parse().map_err(|_| bad())?, m.parse().map_err(|_| bad())?);
    if h > 23 || m > 59 || s.trim().len() > 5 {
        return Err(bad());
    }
    Ok((h, m))
}

/// A warning when `exe` is a Cargo build output, which `cargo clean` deletes.
pub fn unstable_exe(exe: &Path) -> Option<String> {
    exe.components()
        .any(|c| c.as_os_str() == "target")
        .then(|| {
            format!(
                "{} is a build output and goes away on `cargo clean`; \
                 `cargo install --path crates/deckle-cli` gives a stable `deckle` to schedule",
                exe.display()
            )
        })
}

impl Schedule {
    /// The arguments after the binary for one job (`run` or `watch`).
    pub fn args(&self, job: &str) -> Vec<String> {
        let mut a = vec!["digest".to_owned()];
        if let Some(c) = &self.config {
            a.push("--config".into());
            a.push(c.to_string_lossy().into_owned());
        }
        if job == "run" && self.slug != MAIN {
            a.push("--paper".into());
            a.push(self.slug.clone());
        }
        a.push(job.into());
        if job == "run" {
            a.push("--to".into());
            a.push("device".into());
        }
        a
    }

    fn at(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }
}

// ---- macOS ----------------------------------------------------------------------------

fn plist(label: &str, program: &[String], log: &Path, schedule: &str) -> String {
    let args: String = program
        .iter()
        .map(|a| format!("    <string>{}</string>\n", esc(a)))
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n\
         \x20 <key>Label</key><string>{label}</string>\n\
         \x20 <key>ProgramArguments</key>\n  <array>\n{args}  </array>\n\
         {schedule}\
         \x20 <key>StandardOutPath</key><string>{log}</string>\n\
         \x20 <key>StandardErrorPath</key><string>{log}</string>\n\
         \x20 <key>ProcessType</key><string>Background</string>\n\
         </dict>\n</plist>\n",
        log = esc(&log.to_string_lossy()),
    )
}

/// A paper's edition LaunchAgent: (label, XML). Days become one calendar entry per weekday.
pub fn launchd_digest_plist(s: &Schedule, logs: &Path) -> (String, String) {
    let mut program = vec![s.exe.to_string_lossy().into_owned()];
    program.extend(s.args("run"));
    let entry = |day: Option<Day>| {
        let weekday = day
            .map(|d| {
                format!(
                    "\x20   <key>Weekday</key><integer>{}</integer>\n",
                    weekday(d)
                )
            })
            .unwrap_or_default();
        format!(
            "  <dict>\n\
             \x20   <key>Hour</key><integer>{}</integer>\n\
             \x20   <key>Minute</key><integer>{}</integer>\n\
             {weekday}  </dict>\n",
            s.hour, s.minute
        )
    };
    let calendar = match s.days.is_empty() {
        true => format!("  <key>StartCalendarInterval</key>\n{}", entry(None)),
        false => format!(
            "  <key>StartCalendarInterval</key>\n  <array>\n{}  </array>\n",
            s.days.iter().map(|d| entry(Some(*d))).collect::<String>()
        ),
    };
    let label = label(&s.slug);
    let log = match s.slug.as_str() {
        MAIN => logs.join("digest.log"),
        slug => logs.join(format!("digest-{slug}.log")),
    };
    let xml = plist(&label, &program, &log, &calendar);
    (label, xml)
}

/// The watcher's LaunchAgent: (label, XML).
pub fn launchd_watch_plist(exe: &Path, config: Option<&Path>, logs: &Path) -> (String, String) {
    let s = Schedule {
        hour: 0,
        minute: 0,
        days: Vec::new(),
        slug: MAIN.to_owned(),
        exe: exe.to_path_buf(),
        config: config.map(Path::to_path_buf),
    };
    let mut program = vec![exe.to_string_lossy().into_owned()];
    program.extend(s.args("watch"));
    (
        WATCH_LABEL.to_owned(),
        plist(
            WATCH_LABEL,
            &program,
            &logs.join("watch.log"),
            "  <key>RunAtLoad</key><true/>\n  <key>KeepAlive</key><true/>\n",
        ),
    )
}

/// The main paper's job and the watcher, as before many papers: (label, XML) each.
pub fn launchd_plists(s: &Schedule, logs: &Path) -> [(String, String); 2] {
    [
        launchd_digest_plist(s, logs),
        launchd_watch_plist(&s.exe, s.config.as_deref(), logs),
    ]
}

static PLIST_HOUR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<key>Hour</key><integer>(\d+)</integer>").unwrap());
static PLIST_MINUTE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<key>Minute</key><integer>(\d+)</integer>").unwrap());
static PLIST_WEEKDAY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<key>Weekday</key><integer>(\d+)</integer>").unwrap());
static PLIST_EXE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<key>ProgramArguments</key>\s*<array>\s*<string>([^<]+)</string>").unwrap()
});

/// Reads what an installed edition plist schedules.
pub fn parse_digest_plist(xml: &str) -> (Option<String>, Option<String>) {
    let num = |r: &Regex| r.captures(xml).and_then(|c| c[1].parse::<u8>().ok());
    let at = num(&PLIST_HOUR)
        .zip(num(&PLIST_MINUTE))
        .map(|(h, m)| format!("{h:02}:{m:02}"));
    let exe = PLIST_EXE
        .captures(xml)
        .map(|c| html_escape::decode_html_entities(&c[1]).into_owned());
    (at, exe)
}

fn home() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not set".into())
}

fn run(cmd: &mut Command) -> Result<String, String> {
    // Every schtasks/launchctl call lands here: no console window on Windows.
    let out = deckle_core::quiet::hide(cmd)
        .output()
        .map_err(|e| format!("{:?}: {e}", cmd.get_program()))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "{:?} failed: {}",
            cmd.get_program(),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

fn gui_domain() -> Result<String, String> {
    Ok(format!("gui/{}", run(Command::new("id").arg("-u"))?.trim()))
}

/// Writes and (re)loads LaunchAgents: unload (ignoring "not loaded"), write, load.
fn macos_load(jobs: &[(String, String)]) -> Result<Vec<String>, String> {
    let home = home()?;
    let agents = home.join("Library/LaunchAgents");
    std::fs::create_dir_all(&agents).map_err(|e| e.to_string())?;
    let domain = gui_domain()?;
    let mut done = Vec::new();
    for (label, xml) in jobs {
        let path = agents.join(format!("{label}.plist"));
        let _ = run(Command::new("launchctl").args(["bootout", &format!("{domain}/{label}")]));
        std::fs::write(&path, xml).map_err(|e| format!("{}: {e}", path.display()))?;
        run(Command::new("launchctl")
            .args(["bootstrap", &domain])
            .arg(&path))?;
        done.push(format!("loaded {}", path.display()));
    }
    Ok(done)
}

fn macos_logs() -> Result<PathBuf, String> {
    let logs = home()?.join("Library/Logs/Deckle");
    std::fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
    Ok(logs)
}

fn macos_install(s: &Schedule) -> Result<Vec<String>, String> {
    let logs = macos_logs()?;
    macos_load(&launchd_plists(s, &logs))
}

/// Unloads and deletes the LaunchAgents `labels`.
fn macos_unload(labels: &[String]) -> Result<Vec<String>, String> {
    let agents = home()?.join("Library/LaunchAgents");
    let domain = gui_domain()?;
    let mut done = Vec::new();
    for label in labels {
        let _ = run(Command::new("launchctl").args(["bootout", &format!("{domain}/{label}")]));
        let path = agents.join(format!("{label}.plist"));
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            done.push(format!("removed {}", path.display()));
        }
    }
    Ok(done)
}

/// Every installed edition LaunchAgent's label, main and per paper.
fn macos_digest_labels() -> Vec<String> {
    let Ok(agents) = home().map(|h| h.join("Library/LaunchAgents")) else {
        return Vec::new();
    };
    let mut labels: Vec<String> = std::fs::read_dir(agents)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    let label = name.strip_suffix(".plist")?;
                    (label == DIGEST_LABEL || label.starts_with(&format!("{DIGEST_LABEL}.")))
                        .then(|| label.to_owned())
                })
                .collect()
        })
        .unwrap_or_default();
    labels.sort();
    labels
}

fn macos_remove() -> Result<Vec<String>, String> {
    let mut labels = macos_digest_labels();
    labels.push(WATCH_LABEL.to_owned());
    macos_unload(&labels)
}

/// The days an installed plist runs on (empty: every day).
fn plist_days(xml: &str) -> Vec<Day> {
    let mut days: Vec<Day> = PLIST_WEEKDAY
        .captures_iter(xml)
        .filter_map(|c| c[1].parse::<u8>().ok())
        .filter_map(|n| Day::ALL.into_iter().find(|d| weekday(*d) == n % 7))
        .collect();
    days.sort();
    days.dedup();
    days
}

fn macos_status() -> Status {
    let Ok(agents) = home().map(|h| h.join("Library/LaunchAgents")) else {
        return Status::default();
    };
    let mut papers = Vec::new();
    let mut main = (None, None);
    for l in macos_digest_labels() {
        let Ok(xml) = std::fs::read_to_string(agents.join(format!("{l}.plist"))) else {
            continue;
        };
        let (at, exe) = parse_digest_plist(&xml);
        let slug = l
            .strip_prefix(&format!("{DIGEST_LABEL}."))
            .unwrap_or(MAIN)
            .to_owned();
        if slug == MAIN {
            main = (at.clone(), exe);
        }
        papers.push(PaperStatus {
            slug,
            at,
            days: plist_days(&xml),
        });
    }
    papers.sort_by_key(|p| (p.slug != MAIN, p.slug.clone()));
    Status {
        at: main.0,
        watch: agents.join(format!("{WATCH_LABEL}.plist")).exists(),
        exe: main.1,
        papers,
    }
}

// ---- Windows --------------------------------------------------------------------------

/// A Task Scheduler definition (XML, as `schtasks /Create /XML` takes it).
pub fn task_xml(s: &Schedule, job: &str) -> String {
    let trigger = if job == "run" {
        let by = match s.days.is_empty() {
            true => "<ScheduleByDay><DaysInterval>1</DaysInterval></ScheduleByDay>".to_owned(),
            false => format!(
                "<ScheduleByWeek><DaysOfWeek>{}</DaysOfWeek><WeeksInterval>1</WeeksInterval></ScheduleByWeek>",
                s.days
                    .iter()
                    .map(|d| format!("<{}/>", task_day(*d)))
                    .collect::<String>()
            ),
        };
        format!(
            "<CalendarTrigger><StartBoundary>2026-01-01T{}:00</StartBoundary><Enabled>true</Enabled>\
             {by}</CalendarTrigger>",
            s.at()
        )
    } else {
        "<LogonTrigger><Enabled>true</Enabled></LogonTrigger>".to_owned()
    };
    let args = s
        .args(job)
        .iter()
        .map(|a| {
            if a.contains(' ') {
                format!("\"{a}\"")
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n\
         <Task version=\"1.2\" xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\">\n\
         <Triggers>{trigger}</Triggers>\n\
         <Principals><Principal id=\"Author\"><LogonType>InteractiveToken</LogonType>\
         <RunLevel>LeastPrivilege</RunLevel></Principal></Principals>\n\
         <Settings><StartWhenAvailable>true</StartWhenAvailable>\
         <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>\
         <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>\
         <ExecutionTimeLimit>{limit}</ExecutionTimeLimit>\
         <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy></Settings>\n\
         <Actions Context=\"Author\"><Exec><Command>{exe}</Command><Arguments>{args}</Arguments></Exec></Actions>\n\
         </Task>\n",
        limit = if job == "run" { "PT1H" } else { "PT0S" },
        exe = esc(&s.exe.to_string_lossy()),
        args = esc(&args),
    )
}

fn task_name(job: &str) -> String {
    match job {
        "run" => task_name_for(MAIN),
        _ => format!("\\{TASK_FOLDER}\\Watch"),
    }
}

/// Creates (or replaces) the task `name` from `xml`.
fn windows_create(name: &str, xml_text: &str) -> Result<String, String> {
    let xml = std::env::temp_dir().join(format!("deckle-{}.xml", name.replace('\\', "_")));
    // schtasks reads the file in the encoding it declares: UTF-16LE with a BOM.
    let mut bytes = vec![0xFF, 0xFE];
    for u in xml_text.encode_utf16() {
        bytes.extend(u.to_le_bytes());
    }
    std::fs::write(&xml, bytes).map_err(|e| e.to_string())?;
    let created = run(Command::new("schtasks")
        .args(["/Create", "/F", "/TN", name, "/XML"])
        .arg(&xml));
    let _ = std::fs::remove_file(&xml);
    created?;
    Ok(format!("created task {name}"))
}

fn windows_install(s: &Schedule) -> Result<Vec<String>, String> {
    let done = vec![
        windows_create(&task_name_for(&s.slug), &task_xml(s, "run"))?,
        windows_create(&task_name("watch"), &task_xml(s, "watch"))?,
    ];
    // Start the watcher now rather than at the next logon.
    let _ = run(Command::new("schtasks").args(["/Run", "/TN", &task_name("watch")]));
    Ok(done)
}

fn windows_delete(names: &[String]) -> Vec<String> {
    let mut done = Vec::new();
    for name in names {
        let _ = run(Command::new("schtasks").args(["/End", "/TN", name]));
        if run(Command::new("schtasks").args(["/Delete", "/F", "/TN", name])).is_ok() {
            done.push(format!("deleted task {name}"));
        }
    }
    done
}

/// Every edition task under `\Deckle\`, main and per paper.
fn windows_digest_tasks() -> Vec<String> {
    let Ok(csv) = run(Command::new("schtasks").args(["/Query", "/FO", "CSV", "/NH"])) else {
        return Vec::new();
    };
    let main = task_name_for(MAIN);
    let mut names: Vec<String> = csv
        .lines()
        .filter_map(|l| l.split(',').next())
        .map(|n| n.trim_matches('"').to_owned())
        .filter(|n| *n == main || n.starts_with(&format!("{main}-")))
        .collect();
    names.sort();
    names.dedup();
    names
}

fn windows_remove() -> Result<Vec<String>, String> {
    let mut names = windows_digest_tasks();
    names.push(task_name("watch"));
    Ok(windows_delete(&names))
}

static TASK_TIME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"StartBoundary>\d{4}-\d\d-\d\dT(\d\d:\d\d)").unwrap());

fn windows_status() -> Status {
    let main = task_name_for(MAIN);
    let mut papers = Vec::new();
    for name in windows_digest_tasks() {
        let xml = run(Command::new("schtasks").args(["/Query", "/XML", "/TN", &name]));
        let at = xml
            .as_ref()
            .ok()
            .and_then(|x| TASK_TIME.captures(x).map(|c| c[1].to_owned()));
        let days = xml
            .as_ref()
            .map(|x| {
                Day::ALL
                    .into_iter()
                    .filter(|d| x.contains(&format!("<{}", task_day(*d))))
                    .collect()
            })
            .unwrap_or_default();
        let slug = name
            .strip_prefix(&format!("{main}-"))
            .unwrap_or(MAIN)
            .to_owned();
        papers.push(PaperStatus { slug, at, days });
    }
    papers.sort_by_key(|p| (p.slug != MAIN, p.slug.clone()));
    Status {
        at: papers
            .iter()
            .find(|p| p.slug == MAIN)
            .and_then(|p| p.at.clone()),
        watch: run(Command::new("schtasks").args(["/Query", "/TN", &task_name("watch")])).is_ok(),
        exe: None,
        papers,
    }
}

// ---- Entry points -----------------------------------------------------------------------

/// A cron line for one paper's edition: `M H * * D cmd`.
fn cron_line(s: &Schedule) -> String {
    let mut parts = vec![s.exe.to_string_lossy().into_owned()];
    parts.extend(s.args("run"));
    let days = match s.days.is_empty() {
        true => "*".to_owned(),
        false => s
            .days
            .iter()
            .map(|d| weekday(*d).to_string())
            .collect::<Vec<_>>()
            .join(","),
    };
    format!("{} {} * * {days} {}", s.minute, s.hour, parts.join(" "))
}

/// `DECKLE_SCHEDULE_DRY_RUN` set: say what would be installed or removed, and touch nothing
/// (tests; a real install loads jobs into the login session whatever `HOME` says).
fn dry_run() -> bool {
    std::env::var_os("DECKLE_SCHEDULE_DRY_RUN").is_some_and(|v| !v.is_empty())
}

/// The lines to use on Linux, where Deckle doesn't install jobs itself.
pub fn linux_instructions(s: &Schedule) -> String {
    linux_instructions_all(std::slice::from_ref(s), &s.exe, s.config.as_deref())
}

/// [`linux_instructions`] for every paper's job.
fn linux_instructions_all(jobs: &[Schedule], exe: &Path, config: Option<&Path>) -> String {
    let watch = Schedule {
        hour: 0,
        minute: 0,
        days: Vec::new(),
        slug: MAIN.to_owned(),
        exe: exe.to_path_buf(),
        config: config.map(Path::to_path_buf),
    };
    let mut w = vec![exe.to_string_lossy().into_owned()];
    w.extend(watch.args("watch"));
    format!(
        "Deckle doesn't install jobs on Linux. Add this to `crontab -e`:\n\n{}\n\n\
         and run the watcher at login (e.g. a systemd user service with ExecStart={}).",
        jobs.iter().map(cron_line).collect::<Vec<_>>().join("\n"),
        w.join(" ")
    )
}

/// OS service boundary for watcher-only setup. Implementations must use the existing
/// watcher identity and must never install, remove or reschedule edition jobs.
pub trait WatcherService {
    fn matches(&mut self, exe: &Path, config: &Path) -> Result<bool, String>;
    fn install(&mut self, exe: &Path, config: &Path) -> Result<(), String>;
    /// Starts the watcher if it isn't running; a running one is left as it is.
    fn start(&mut self) -> Result<(), String>;
    /// Stops the running watcher, if any, and starts it again from its program on disk.
    fn restart(&mut self) -> Result<(), String>;
}

/// Ensure the single watcher is configured and started, preserving all edition schedules.
pub fn ensure_watcher_with(
    exe: &Path,
    config: &Path,
    service: &mut impl WatcherService,
) -> Result<(), String> {
    if !service.matches(exe, config)? {
        service.install(exe, config)?;
    }
    service.start()
}

struct NativeWatcher;
fn watch_schedule(exe: &Path, config: &Path) -> Schedule {
    Schedule {
        hour: 0,
        minute: 0,
        days: Vec::new(),
        slug: MAIN.to_owned(),
        exe: exe.to_owned(),
        config: Some(config.to_owned()),
    }
}

fn task_action(xml: &str) -> Result<(String, String), String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    let exec = doc
        .descendants()
        .find(|n| n.has_tag_name("Exec"))
        .ok_or("Watcher task has no action")?;
    let text = |tag| {
        exec.children()
            .find(|n| n.has_tag_name(tag))
            .and_then(|n| n.text())
            .unwrap_or_default()
            .to_owned()
    };
    Ok((text("Command"), text("Arguments")))
}
impl WatcherService for NativeWatcher {
    fn matches(&mut self, exe: &Path, config: &Path) -> Result<bool, String> {
        if cfg!(target_os = "macos") {
            let home = home()?;
            let path = home
                .join("Library/LaunchAgents")
                .join(format!("{WATCH_LABEL}.plist"));
            let expected =
                launchd_watch_plist(exe, Some(config), &home.join("Library/Logs/Deckle")).1;
            if std::fs::read_to_string(path).ok().as_deref() != Some(&expected) {
                return Ok(false);
            }
            Ok(run(Command::new("launchctl")
                .args(["print", &format!("{}/{WATCH_LABEL}", gui_domain()?)]))
            .is_ok())
        } else if cfg!(target_os = "windows") {
            let Ok(xml) =
                run(Command::new("schtasks").args(["/Query", "/TN", &task_name("watch"), "/XML"]))
            else {
                return Ok(false);
            };
            Ok(
                task_action(&xml)?
                    == task_action(&task_xml(&watch_schedule(exe, config), "watch"))?,
            )
        } else {
            Err(linux_instructions_all(&[], exe, Some(config)))
        }
    }
    fn install(&mut self, exe: &Path, config: &Path) -> Result<(), String> {
        if cfg!(target_os = "macos") {
            let job = launchd_watch_plist(exe, Some(config), &macos_logs()?);
            macos_load(&[job])?;
            Ok(())
        } else if cfg!(target_os = "windows") {
            windows_create(
                &task_name("watch"),
                &task_xml(&watch_schedule(exe, config), "watch"),
            )?;
            Ok(())
        } else {
            Err(linux_instructions_all(&[], exe, Some(config)))
        }
    }
    fn start(&mut self) -> Result<(), String> {
        if cfg!(target_os = "macos") {
            run(Command::new("launchctl")
                .args(["kickstart", &format!("{}/{WATCH_LABEL}", gui_domain()?)]))?;
        } else if cfg!(target_os = "windows") {
            run(Command::new("schtasks").args(["/Run", "/TN", &task_name("watch")]))?;
        } else {
            return Err("Watcher installation is unsupported on this platform".into());
        }
        Ok(())
    }
    fn restart(&mut self) -> Result<(), String> {
        if cfg!(target_os = "macos") {
            // `-k` replaces a running instance; plain `kickstart` leaves it be.
            run(Command::new("launchctl").args([
                "kickstart",
                "-k",
                &format!("{}/{WATCH_LABEL}", gui_domain()?),
            ]))?;
        } else if cfg!(target_os = "windows") {
            // `/Run` won't start a second instance; `/End` fails when none is running.
            let _ = run(Command::new("schtasks").args(["/End", "/TN", &task_name("watch")]));
            run(Command::new("schtasks").args(["/Run", "/TN", &task_name("watch")]))?;
        } else {
            return Err("Watcher installation is unsupported on this platform".into());
        }
        Ok(())
    }
}

/// Which build a watcher started from: its program and that file's size and modified time then.
/// A program replaced since (an update) no longer matches.
#[derive(Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
struct WatcherBuild {
    exe: PathBuf,
    len: u64,
    modified_ns: u64,
}

impl WatcherBuild {
    fn of(exe: &Path) -> Option<Self> {
        let meta = std::fs::metadata(exe).ok()?;
        let modified = meta
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?;
        Some(Self {
            exe: exe.to_owned(),
            len: meta.len(),
            modified_ns: modified.as_nanos() as u64,
        })
    }
}

/// Where the watcher notes its build when it starts: one per user, like the watcher.
pub fn watcher_marker_path() -> PathBuf {
    crate::config::default_path().with_file_name("watcher.json")
}

/// Notes the running program as the watcher's build (`digest watch` calls this on start).
pub fn record_watcher_start() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    record_watcher_start_at(&watcher_marker_path(), &exe)
}

/// [`record_watcher_start`] for an explicit marker and program.
pub fn record_watcher_start_at(marker: &Path, exe: &Path) -> Result<(), String> {
    let build = WatcherBuild::of(exe).ok_or_else(|| format!("{}: unreadable", exe.display()))?;
    if let Some(dir) = marker.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let json = serde_json::to_string(&build).map_err(|e| e.to_string())?;
    std::fs::write(marker, json).map_err(|e| format!("{}: {e}", marker.display()))
}

/// Restarts an installed watcher that runs an older build than its program on disk: one whose
/// program was replaced since it started, or one from before the marker (#234). A watcher whose
/// program is gone is left alone. Returns whether it restarted.
pub fn refresh_watcher_with(
    installed: bool,
    marker: &Path,
    service: &mut impl WatcherService,
) -> Result<bool, String> {
    if !installed {
        return Ok(false);
    }
    let started: Option<WatcherBuild> = std::fs::read(marker)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    let stale = match started {
        Some(s) => WatcherBuild::of(&s.exe).is_some_and(|now| now != s),
        None => true,
    };
    if stale {
        service.restart()?;
    }
    Ok(stale)
}

/// [`refresh_watcher_with`] for this user's watcher job; the app calls it when it opens. A
/// daemon install's watcher is the container's to restart.
pub fn refresh_watcher() -> Result<bool, String> {
    if dry_run()
        || crate::secret::daemon_install()
        || !cfg!(any(target_os = "macos", target_os = "windows"))
    {
        return Ok(false);
    }
    refresh_watcher_with(status().watch, &watcher_marker_path(), &mut NativeWatcher)
}

/// Install/start only the existing watcher using the supported platform scheduler. Service
/// failure is returned to setup; a scheduled edition's time and weekdays are never changed.
pub fn ensure_watcher(exe: &Path, config: &Path) -> Result<(), String> {
    if dry_run() {
        return Err("Watcher setup was not performed in schedule dry-run mode".into());
    }
    ensure_watcher_with(exe, config, &mut NativeWatcher)
}

/// Installs every paper's job in `jobs` and the watcher, then removes the jobs of papers that
/// are gone (many papers §5). The main paper's job is replaced only when `jobs` has one for it.
pub fn install_all(
    jobs: &[Schedule],
    exe: &Path,
    config: Option<&Path>,
) -> Result<Vec<String>, String> {
    if dry_run() {
        let mut done: Vec<String> = jobs
            .iter()
            .map(|s| {
                let days: Vec<&str> = s.days.iter().map(|d| d.key()).collect();
                format!(
                    "would install {} at {} {}",
                    label(&s.slug),
                    s.at(),
                    days.join(",")
                )
                .trim_end()
                .to_owned()
            })
            .collect();
        done.push(format!("would install {WATCH_LABEL}"));
        return Ok(done);
    }
    if cfg!(target_os = "macos") {
        let logs = macos_logs()?;
        let mut plists: Vec<(String, String)> = jobs
            .iter()
            .map(|s| launchd_digest_plist(s, &logs))
            .collect();
        plists.push(launchd_watch_plist(exe, config, &logs));
        let mut done = macos_load(&plists)?;
        done.extend(macos_unload(&stale_labels(&macos_digest_labels(), jobs))?);
        Ok(done)
    } else if cfg!(target_os = "windows") {
        let mut done = Vec::new();
        for s in jobs {
            done.push(windows_create(
                &task_name_for(&s.slug),
                &task_xml(s, "run"),
            )?);
        }
        let watch = Schedule {
            hour: 0,
            minute: 0,
            days: Vec::new(),
            slug: MAIN.to_owned(),
            exe: exe.to_path_buf(),
            config: config.map(Path::to_path_buf),
        };
        done.push(windows_create(
            &task_name("watch"),
            &task_xml(&watch, "watch"),
        )?);
        let _ = run(Command::new("schtasks").args(["/Run", "/TN", &task_name("watch")]));
        let main = task_name_for(MAIN);
        let stale: Vec<String> = windows_digest_tasks()
            .into_iter()
            .filter(|n| n.starts_with(&format!("{main}-")))
            .filter(|n| !jobs.iter().any(|s| task_name_for(&s.slug) == *n))
            .collect();
        done.extend(windows_delete(&stale));
        Ok(done)
    } else {
        Err(linux_instructions_all(jobs, exe, config))
    }
}

/// Installs (or replaces) both jobs; returns what was done.
pub fn install(s: &Schedule) -> Result<Vec<String>, String> {
    if cfg!(target_os = "macos") {
        macos_install(s)
    } else if cfg!(target_os = "windows") {
        windows_install(s)
    } else {
        Err(linux_instructions(s))
    }
}

/// Removes one paper's edition job, and the watcher too when no edition job is left (the
/// watcher delivers what the other papers' jobs build).
pub fn remove_paper(slug: &str) -> Result<Vec<String>, String> {
    if dry_run() {
        return Ok(vec![format!("would remove {}", label(slug))]);
    }
    if cfg!(target_os = "macos") {
        let mut done = macos_unload(&[label(slug)])?;
        if macos_digest_labels().is_empty() {
            done.extend(macos_unload(&[WATCH_LABEL.to_owned()])?);
        }
        Ok(done)
    } else if cfg!(target_os = "windows") {
        let mut done = windows_delete(&[task_name_for(slug)]);
        if windows_digest_tasks().is_empty() {
            done.extend(windows_delete(&[task_name("watch")]));
        }
        Ok(done)
    } else {
        Ok(Vec::new())
    }
}

/// Removes every edition job and the watcher; returns what was removed.
pub fn remove() -> Result<Vec<String>, String> {
    if cfg!(target_os = "macos") {
        macos_remove()
    } else if cfg!(target_os = "windows") {
        windows_remove()
    } else {
        Ok(Vec::new())
    }
}

/// What's scheduled. In a daemon install that's the config (a broken one reads as nothing
/// scheduled; [`daemon_status`] says why), on any OS.
pub fn status() -> Status {
    if crate::secret::daemon_install() {
        daemon_status().unwrap_or(Status {
            watch: true,
            ..Status::default()
        })
    } else if cfg!(target_os = "macos") {
        macos_status()
    } else if cfg!(target_os = "windows") {
        windows_status()
    } else {
        Status::default()
    }
}

// ---- a daemon install: the config is the schedule ------------------------------------
// The container's daemon re-reads `[schedule]` every tick, so `digest schedule` there and the
// web UI's schedule switch both edit the file and install nothing.

/// Writes paper `paper`'s (`None`: the main paper's) `[schedule] at` (`None` clears it) and,
/// when given, its `days`; days not given are kept. Returns the saved paper.
pub fn save_config_schedule(
    main: &Path,
    paper: Option<&str>,
    at: Option<&str>,
    days: Option<Vec<Day>>,
) -> Result<Config, String> {
    if let Some(at) = at {
        parse_at(at)?;
    }
    let mut cfg = crate::papers::load_one(main, paper.unwrap_or(MAIN))?;
    cfg.schedule.at = at.map(str::to_owned);
    if let Some(days) = days {
        cfg.schedule.days = days;
    }
    crate::papers::save(main, &cfg)?;
    Ok(cfg)
}

/// Clears paper `paper`'s whole `[schedule]`, days too (`digest schedule --off`).
pub fn clear_config_schedule(main: &Path, paper: Option<&str>) -> Result<Config, String> {
    save_config_schedule(main, paper, None, Some(Vec::new()))
}

/// [`status`] read from the config: every paper with `[schedule] at`, main first. There is no
/// watcher job or binary to report.
pub fn config_status(main: &Path) -> Result<Status, String> {
    let papers: Vec<PaperStatus> = crate::papers::load_all(main)?
        .papers
        .iter()
        .filter_map(|c| {
            Some(PaperStatus {
                slug: c.slug().to_owned(),
                at: Some(c.schedule.at.clone()?),
                days: c.schedule.days.clone(),
            })
        })
        .collect();
    let at = papers
        .iter()
        .find(|p| p.slug == MAIN)
        .and_then(|p| p.at.clone());
    Ok(Status {
        at,
        papers,
        ..Status::default()
    })
}

/// [`status`] in a daemon install: the config's schedule, and the watcher is on (the daemon
/// runs it).
pub fn daemon_status() -> Result<Status, String> {
    Ok(Status {
        watch: true,
        ..config_status(&crate::config::default_path())?
    })
}

/// The next press from a paper's `[schedule]` (`at`, and `days` when set), in `now`'s zone.
pub fn config_next_press(
    sched: &crate::config::ScheduleConfig,
    now: chrono::DateTime<chrono_tz::Tz>,
) -> Option<chrono::NaiveDate> {
    use chrono::{Datelike, Days};
    let at = parse_at(sched.at.as_deref()?).ok()?;
    let mut date = crate::press::next_press(at, now).date_naive();
    for _ in 0..7 {
        let day = Day::ALL[date.weekday().num_days_from_monday() as usize];
        if sched.days.is_empty() || sched.days.contains(&day) {
            return Some(date);
        }
        date = date + Days::new(1);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A main paper at `dir/digest.toml` and one extra, `courier`.
    fn two_papers(dir: &Path) -> PathBuf {
        let main = dir.join("digest.toml");
        std::fs::write(&main, "title = \"The Gazette\"\n").unwrap();
        crate::papers::create(&main, "courier", "The Courier", "Sam").unwrap();
        main
    }

    #[test]
    fn a_config_schedule_saves_at_and_days_and_reads_back() {
        let tmp = tempfile::tempdir().unwrap();
        let main = two_papers(tmp.path());
        save_config_schedule(&main, None, Some("06:15"), Some(vec![Day::Sat])).unwrap();
        save_config_schedule(&main, Some("courier"), Some("07:00"), None).unwrap();
        let s = config_status(&main).unwrap();
        assert_eq!(s.at.as_deref(), Some("06:15"));
        assert!(!s.watch && s.exe.is_none());
        assert_eq!(
            s.papers,
            vec![
                PaperStatus {
                    slug: MAIN.into(),
                    at: Some("06:15".into()),
                    days: vec![Day::Sat]
                },
                PaperStatus {
                    slug: "courier".into(),
                    at: Some("07:00".into()),
                    days: Vec::new()
                },
            ]
        );
    }

    #[test]
    fn status_in_a_daemon_install_is_the_config_with_the_watcher_on() {
        let tmp = tempfile::tempdir().unwrap();
        // nextest runs each test in its own process, so the environment is this test's alone.
        unsafe {
            std::env::set_var("HOME", tmp.path());
            std::env::set_var("APPDATA", tmp.path());
            std::env::set_var("XDG_CONFIG_HOME", tmp.path());
            std::env::set_var("DECKLE_DAEMON", "1");
        }
        let main = crate::config::default_path();
        std::fs::create_dir_all(main.parent().unwrap()).unwrap();
        two_papers(main.parent().unwrap());
        save_config_schedule(&main, None, Some("06:15"), Some(vec![Day::Sat])).unwrap();
        save_config_schedule(&main, Some("courier"), Some("07:00"), None).unwrap();
        let s = status();
        assert_eq!(s.at.as_deref(), Some("06:15"));
        assert!(s.watch, "the daemon owns the watcher");
        assert_eq!(
            s.papers.iter().map(|p| p.slug.as_str()).collect::<Vec<_>>(),
            [MAIN, "courier"]
        );
        assert_eq!(s, daemon_status().unwrap());

        std::fs::write(&main, "title = [").unwrap();
        assert!(daemon_status().is_err(), "a broken config is an error");
        assert!(status().watch && status().papers.is_empty());
    }

    #[test]
    fn a_config_schedule_keeps_days_unless_given_and_refuses_a_bad_time() {
        let tmp = tempfile::tempdir().unwrap();
        let main = two_papers(tmp.path());
        save_config_schedule(&main, None, Some("06:15"), Some(vec![Day::Mon])).unwrap();
        // Off keeps the days, as the desktop's switch does; on again finds them.
        let cfg = save_config_schedule(&main, None, None, None).unwrap();
        assert_eq!(
            (cfg.schedule.at, cfg.schedule.days.clone()),
            (None, vec![Day::Mon])
        );
        assert_eq!(config_status(&main).unwrap(), Status::default());
        assert!(save_config_schedule(&main, None, Some("25:00"), None).is_err());
        clear_config_schedule(&main, None).unwrap();
        let cfg = crate::papers::load_one(&main, MAIN).unwrap();
        assert!(cfg.schedule.is_default());
    }

    #[test]
    fn the_next_config_press_skips_to_a_scheduled_day() {
        use chrono::TimeZone;
        let tz = chrono_tz::Europe::London;
        // A Thursday, before 06:00.
        let now = tz.with_ymd_and_hms(2026, 10, 29, 5, 0, 0).unwrap();
        let sched = |days: Vec<Day>| crate::config::ScheduleConfig {
            at: Some("06:00".into()),
            days,
        };
        let d = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        assert_eq!(
            config_next_press(&sched(Vec::new()), now),
            Some(d("2026-10-29"))
        );
        assert_eq!(
            config_next_press(&sched(vec![Day::Sat]), now),
            Some(d("2026-10-31"))
        );
        assert_eq!(config_next_press(&Default::default(), now), None);
    }

    fn sched(config: Option<&str>) -> Schedule {
        Schedule {
            hour: 5,
            minute: 30,
            days: Vec::new(),
            slug: MAIN.into(),
            exe: PathBuf::from("/Users/me/.cargo/bin/deckle"),
            config: config.map(PathBuf::from),
        }
    }

    fn courier(days: &[Day]) -> Schedule {
        Schedule {
            hour: 7,
            minute: 0,
            days: days.to_vec(),
            slug: "kid-courier".into(),
            ..sched(None)
        }
    }

    #[test]
    fn a_paper_on_days_gets_one_calendar_entry_per_weekday() {
        let (l, xml) = launchd_digest_plist(&courier(&[Day::Sat]), Path::new("/L"));
        assert_eq!(l, "com.casavargas.deckle.digest.kid-courier");
        assert!(
            xml.contains("<key>StartCalendarInterval</key>\n  <array>"),
            "{xml}"
        );
        assert!(xml.contains("<key>Weekday</key><integer>6</integer>"));
        assert!(xml.contains("<key>Hour</key><integer>7</integer>"));
        assert_eq!(xml.matches("<key>Weekday</key>").count(), 1);
        assert_eq!(plist_days(&xml), [Day::Sat]);
        let (_, two) = launchd_digest_plist(&courier(&[Day::Mon, Day::Wed]), Path::new("/L"));
        assert_eq!(two.matches("<key>Weekday</key>").count(), 2);
        assert_eq!(plist_days(&two), [Day::Mon, Day::Wed]);
        assert!(xml.ends_with("</plist>\n"));
        // The job builds that paper.
        let args: Vec<String> = courier(&[]).args("run");
        assert_eq!(
            args,
            ["digest", "--paper", "kid-courier", "run", "--to", "device"]
        );
        assert!(xml.contains("digest-kid-courier.log"));
    }

    #[test]
    fn the_main_papers_daily_plist_is_unchanged() {
        let (l, xml) = launchd_digest_plist(&sched(None), Path::new("/L"));
        assert_eq!(l, DIGEST_LABEL);
        assert!(xml.contains(
            "<key>StartCalendarInterval</key>\n  <dict>\n    <key>Hour</key><integer>5</integer>\n    \
             <key>Minute</key><integer>30</integer>\n  </dict>\n"
        ), "{xml}");
        assert!(!xml.contains("--paper") && !xml.contains("Weekday"));
        assert_eq!(plist_days(&xml), []);
    }

    #[test]
    fn weekly_task_xml_and_names_per_paper() {
        let xml = task_xml(&courier(&[Day::Sat]), "run");
        assert!(xml.contains(
            "<ScheduleByWeek><DaysOfWeek><Saturday/></DaysOfWeek><WeeksInterval>1</WeeksInterval></ScheduleByWeek>"
        ), "{xml}");
        assert!(xml.contains("<Arguments>digest --paper kid-courier run --to device</Arguments>"));
        assert_eq!(task_name_for(MAIN), "\\Deckle\\Digest");
        assert_eq!(task_name_for("kid-courier"), "\\Deckle\\Digest-kid-courier");
        assert_eq!(label(MAIN), DIGEST_LABEL);
    }

    #[test]
    fn stale_labels_keep_main_and_wanted_papers() {
        let installed = [
            DIGEST_LABEL.to_owned(),
            label("lakeside"),
            label("old"),
            WATCH_LABEL.to_owned(),
        ];
        let lakeside = Schedule {
            slug: "lakeside".into(),
            ..sched(None)
        };
        assert_eq!(stale_labels(&installed, &[lakeside]), [label("old")]);
        assert_eq!(
            stale_labels(&installed, &[]),
            [label("lakeside"), label("old")]
        );
    }

    #[test]
    fn jobs_for_takes_papers_with_a_time_only() {
        let mut main = Config::default();
        let kid = Config {
            paper: Some(crate::config::PaperRef {
                slug: "kid".into(),
                state_dir: PathBuf::from("/x"),
            }),
            schedule: crate::config::ScheduleConfig {
                at: Some("07:00".into()),
                days: vec![Day::Sun, Day::Sat, Day::Sat],
            },
            ..Config::default()
        };
        let jobs = jobs_for(&[main.clone(), kid.clone()], Path::new("/d"), None);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].slug, "kid");
        assert_eq!(jobs[0].days, [Day::Sat, Day::Sun]);
        main.schedule.at = Some("05:30".into());
        assert_eq!(jobs_for(&[main, kid], Path::new("/d"), None).len(), 2);
    }

    #[test]
    fn cron_lines_carry_days_and_paper() {
        assert_eq!(
            cron_line(&courier(&[Day::Sat, Day::Sun])),
            "0 7 * * 6,0 /Users/me/.cargo/bin/deckle digest --paper kid-courier run --to device"
        );
    }

    #[test]
    fn times_parse_strictly() {
        assert_eq!(parse_at("05:30"), Ok((5, 30)));
        assert_eq!(parse_at("5:05"), Ok((5, 5)));
        for bad in ["24:00", "05:60", "0530", "5:30pm", "", "05:30:00"] {
            assert!(parse_at(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn plists_run_the_edition_daily_and_keep_the_watcher_alive() {
        let [(dl, digest), (wl, watch)] =
            launchd_plists(&sched(Some("/tmp/my config.toml")), Path::new("/L"));
        assert_eq!((dl.as_str(), wl.as_str()), (DIGEST_LABEL, WATCH_LABEL));
        assert!(digest.contains("<key>Hour</key><integer>5</integer>"));
        assert!(digest.contains("<key>Minute</key><integer>30</integer>"));
        for arg in [
            "/Users/me/.cargo/bin/deckle",
            "digest",
            "--config",
            "/tmp/my config.toml",
            "run",
            "--to",
            "device",
        ] {
            assert!(digest.contains(&format!("<string>{arg}</string>")), "{arg}");
        }
        let log = Path::new("/L").join("digest.log");
        assert!(digest.contains(&format!("<string>{}</string>", log.display())));
        assert!(watch.contains("<key>KeepAlive</key><true/>"));
        assert!(
            watch.contains("<string>watch</string>") && !watch.contains("<string>run</string>")
        );
        // What status reads back.
        assert_eq!(
            parse_digest_plist(&digest),
            (
                Some("05:30".into()),
                Some("/Users/me/.cargo/bin/deckle".into())
            )
        );
        // Plists are XML (roxmltree refuses the DTD only when told to parse it).
        let opts = roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        };
        roxmltree::Document::parse_with_options(&digest, opts).unwrap();
    }

    #[test]
    fn windows_tasks_catch_up_after_a_missed_start() {
        let run = task_xml(&sched(Some(r"C:\Users\me\deckle config.toml")), "run");
        assert!(run.contains("T05:30:00</StartBoundary>"));
        assert!(run.contains("<StartWhenAvailable>true</StartWhenAvailable>"));
        assert!(run.contains(r#"<Arguments>digest --config &quot;C:\Users\me\deckle config.toml&quot; run --to device</Arguments>"#), "{run}");
        let watch = task_xml(&sched(None), "watch");
        assert!(
            watch.contains("<LogonTrigger>")
                && watch.contains("<Arguments>digest watch</Arguments>")
        );
    }

    #[test]
    fn cargo_build_outputs_are_flagged() {
        assert!(unstable_exe(Path::new("/src/deckle/target/release/deckle")).is_some());
        assert!(unstable_exe(Path::new("/Users/me/.cargo/bin/deckle")).is_none());
    }
}
