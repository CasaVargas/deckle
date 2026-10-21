// SPDX-License-Identifier: GPL-3.0-or-later
//! Scheduling (design §5b): the edition every morning, and the watcher from login on.
//! macOS: two LaunchAgents. Windows: two Task Scheduler tasks. Linux: printed instructions.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use crate::esc;

pub const DIGEST_LABEL: &str = "com.casavargas.deckle.digest";
pub const WATCH_LABEL: &str = "com.casavargas.deckle.watch";
const TASK_FOLDER: &str = "Deckle";

/// What the jobs run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Schedule {
    pub hour: u8,
    pub minute: u8,
    /// The `deckle` binary, absolute.
    pub exe: PathBuf,
    /// `--config`, carried into both jobs.
    pub config: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Status {
    /// `HH:MM` of the installed edition job.
    pub at: Option<String>,
    pub watch: bool,
    /// The binary the installed jobs run.
    pub exe: Option<String>,
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
    fn args(&self, job: &str) -> Vec<String> {
        let mut a = vec!["digest".to_owned()];
        if let Some(c) = &self.config {
            a.push("--config".into());
            a.push(c.to_string_lossy().into_owned());
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

/// The two LaunchAgents: (label, file name, XML).
pub fn launchd_plists(s: &Schedule, logs: &Path) -> [(&'static str, String); 2] {
    let exe = s.exe.to_string_lossy().into_owned();
    let program = |job: &str| {
        let mut p = vec![exe.clone()];
        p.extend(s.args(job));
        p
    };
    [
        (
            DIGEST_LABEL,
            plist(
                DIGEST_LABEL,
                &program("run"),
                &logs.join("digest.log"),
                &format!(
                    "  <key>StartCalendarInterval</key>\n  <dict>\n\
                     \x20   <key>Hour</key><integer>{}</integer>\n\
                     \x20   <key>Minute</key><integer>{}</integer>\n  </dict>\n",
                    s.hour, s.minute
                ),
            ),
        ),
        (
            WATCH_LABEL,
            plist(
                WATCH_LABEL,
                &program("watch"),
                &logs.join("watch.log"),
                "  <key>RunAtLoad</key><true/>\n  <key>KeepAlive</key><true/>\n",
            ),
        ),
    ]
}

static PLIST_HOUR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<key>Hour</key><integer>(\d+)</integer>").unwrap());
static PLIST_MINUTE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<key>Minute</key><integer>(\d+)</integer>").unwrap());
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
    let out = cmd
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

fn macos_install(s: &Schedule) -> Result<Vec<String>, String> {
    let home = home()?;
    let agents = home.join("Library/LaunchAgents");
    let logs = home.join("Library/Logs/Deckle");
    std::fs::create_dir_all(&agents).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
    let domain = gui_domain()?;
    let mut done = Vec::new();
    for (label, xml) in launchd_plists(s, &logs) {
        let path = agents.join(format!("{label}.plist"));
        // Replace a running job: unload (ignoring "not loaded"), write, load.
        let _ = run(Command::new("launchctl").args(["bootout", &format!("{domain}/{label}")]));
        std::fs::write(&path, xml).map_err(|e| format!("{}: {e}", path.display()))?;
        run(Command::new("launchctl")
            .args(["bootstrap", &domain])
            .arg(&path))?;
        done.push(format!("loaded {}", path.display()));
    }
    Ok(done)
}

fn macos_remove() -> Result<Vec<String>, String> {
    let agents = home()?.join("Library/LaunchAgents");
    let domain = gui_domain()?;
    let mut done = Vec::new();
    for label in [DIGEST_LABEL, WATCH_LABEL] {
        let _ = run(Command::new("launchctl").args(["bootout", &format!("{domain}/{label}")]));
        let path = agents.join(format!("{label}.plist"));
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            done.push(format!("removed {}", path.display()));
        }
    }
    Ok(done)
}

fn macos_status() -> Status {
    let Ok(agents) = home().map(|h| h.join("Library/LaunchAgents")) else {
        return Status::default();
    };
    let (at, exe) = std::fs::read_to_string(agents.join(format!("{DIGEST_LABEL}.plist")))
        .map(|x| parse_digest_plist(&x))
        .unwrap_or((None, None));
    Status {
        at,
        watch: agents.join(format!("{WATCH_LABEL}.plist")).exists(),
        exe,
    }
}

// ---- Windows --------------------------------------------------------------------------

/// A Task Scheduler definition (XML, as `schtasks /Create /XML` takes it).
pub fn task_xml(s: &Schedule, job: &str) -> String {
    let trigger = if job == "run" {
        format!(
            "<CalendarTrigger><StartBoundary>2026-01-01T{}:00</StartBoundary><Enabled>true</Enabled>\
             <ScheduleByDay><DaysInterval>1</DaysInterval></ScheduleByDay></CalendarTrigger>",
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
    let name = if job == "run" { "Digest" } else { "Watch" };
    format!("\\{TASK_FOLDER}\\{name}")
}

fn windows_install(s: &Schedule) -> Result<Vec<String>, String> {
    let mut done = Vec::new();
    for job in ["run", "watch"] {
        let xml = std::env::temp_dir().join(format!("deckle-{job}.xml"));
        // schtasks reads the file in the encoding it declares: UTF-16LE with a BOM.
        let mut bytes = vec![0xFF, 0xFE];
        for u in task_xml(s, job).encode_utf16() {
            bytes.extend(u.to_le_bytes());
        }
        std::fs::write(&xml, bytes).map_err(|e| e.to_string())?;
        run(Command::new("schtasks")
            .args(["/Create", "/F", "/TN", &task_name(job), "/XML"])
            .arg(&xml))?;
        let _ = std::fs::remove_file(&xml);
        done.push(format!("created task {}", task_name(job)));
    }
    // Start the watcher now rather than at the next logon.
    let _ = run(Command::new("schtasks").args(["/Run", "/TN", &task_name("watch")]));
    Ok(done)
}

fn windows_remove() -> Result<Vec<String>, String> {
    let mut done = Vec::new();
    for job in ["run", "watch"] {
        let _ = run(Command::new("schtasks").args(["/End", "/TN", &task_name(job)]));
        if run(Command::new("schtasks").args(["/Delete", "/F", "/TN", &task_name(job)])).is_ok() {
            done.push(format!("deleted task {}", task_name(job)));
        }
    }
    Ok(done)
}

static TASK_TIME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"StartBoundary>\d{4}-\d\d-\d\dT(\d\d:\d\d)").unwrap());

fn windows_status() -> Status {
    let xml = run(Command::new("schtasks").args(["/Query", "/XML", "/TN", &task_name("run")]));
    let at = xml
        .as_ref()
        .ok()
        .and_then(|x| TASK_TIME.captures(x).map(|c| c[1].to_owned()));
    Status {
        at,
        watch: run(Command::new("schtasks").args(["/Query", "/TN", &task_name("watch")])).is_ok(),
        exe: None,
    }
}

// ---- Entry points -----------------------------------------------------------------------

/// The lines to use on Linux, where Deckle doesn't install jobs itself.
pub fn linux_instructions(s: &Schedule) -> String {
    let cmd = |job: &str| {
        let mut parts = vec![s.exe.to_string_lossy().into_owned()];
        parts.extend(s.args(job));
        parts.join(" ")
    };
    format!(
        "Deckle doesn't install jobs on Linux. Add this to `crontab -e`:\n\n\
         {} {} * * * {}\n\n\
         and run the watcher at login (e.g. a systemd user service with ExecStart={}).",
        s.minute,
        s.hour,
        cmd("run"),
        cmd("watch")
    )
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

/// Removes both jobs; returns what was removed.
pub fn remove() -> Result<Vec<String>, String> {
    if cfg!(target_os = "macos") {
        macos_remove()
    } else if cfg!(target_os = "windows") {
        windows_remove()
    } else {
        Ok(Vec::new())
    }
}

pub fn status() -> Status {
    if cfg!(target_os = "macos") {
        macos_status()
    } else if cfg!(target_os = "windows") {
        windows_status()
    } else {
        Status::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sched(config: Option<&str>) -> Schedule {
        Schedule {
            hour: 5,
            minute: 30,
            exe: PathBuf::from("/Users/me/.cargo/bin/deckle"),
            config: config.map(PathBuf::from),
        }
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
        assert_eq!((dl, wl), (DIGEST_LABEL, WATCH_LABEL));
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
