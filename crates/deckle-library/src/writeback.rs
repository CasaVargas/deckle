// SPDX-License-Identifier: GPL-3.0-or-later
//! Reading data written back into calibre (calibre bridge design §5): what the readers say of
//! each book (progress, finished, last read, stars) goes into the calibre columns the owner
//! chose, only through calibre's own `calibredb`, only when it changed, and never over a value
//! someone set in calibre since Deckle last wrote.
//!
//! The readers' side is the reading log (`reading-log.json`), filled as each reader is read.
//! The plan is made from the log, the library (read-only) and what Deckle last wrote; the run
//! hands each change to `calibredb set_custom`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use deckle_core::reading::Record;
use serde::{Deserialize, Serialize};

use crate::calibre::{self, Column};

/// A `calibredb` that runs longer is stopped.
pub const TIMEOUT: Duration = Duration::from_secs(60);

/// What the readers said, by reader and book: the newest of each.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReadLog {
    /// `<reader>\u{1f}<book>` → its record, the book its path on the reader, else its title.
    pub books: BTreeMap<String, Logged>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Logged {
    pub reader: String,
    pub record: Record,
}

impl ReadLog {
    pub fn load(path: &Path) -> ReadLog {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        write_json(path, self)
    }

    /// Takes `records` from `reader`, keeping an older record where the new one is older.
    pub fn record(&mut self, reader: &str, records: &[Record]) -> usize {
        let mut changed = 0;
        for r in records {
            if r.title.trim().is_empty() && r.path.is_none() {
                continue;
            }
            let book = r.path.clone().unwrap_or_else(|| norm(&r.title));
            let key = format!("{reader}\u{1f}{book}");
            let newer = self
                .books
                .get(&key)
                .is_none_or(|old| old.record != *r && old.record.last_read <= r.last_read);
            if newer {
                self.books.insert(
                    key,
                    Logged {
                        reader: reader.to_owned(),
                        record: r.clone(),
                    },
                );
                changed += 1;
            }
        }
        changed
    }
}

/// Which calibre column each piece goes in, by label (without `#`); `None`: not written.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Columns {
    pub progress: Option<String>,
    pub read: Option<String>,
    pub last_read: Option<String>,
    pub rating: Option<String>,
}

impl Columns {
    fn pairs(&self) -> [(Field, Option<&String>); 4] {
        [
            (Field::Progress, self.progress.as_ref()),
            (Field::Read, self.read.as_ref()),
            (Field::LastRead, self.last_read.as_ref()),
            (Field::Rating, self.rating.as_ref()),
        ]
    }
}

/// `calibre-writeback.json` beside `library.json`: the owner's choices, and what Deckle wrote.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Writeback {
    pub enabled: bool,
    /// `calibredb` itself; `None`: found on `PATH` or where calibre installs it.
    pub calibredb: Option<PathBuf>,
    /// The content server, `http://host:port/#library_id`, for writing while calibre is open.
    pub server: Option<String>,
    /// The content server's user; the password is in the system keychain.
    pub username: Option<String>,
    pub columns: Columns,
    /// Books matched by title and author, confirmed once: log key → calibre uuid.
    pub confirmed: BTreeMap<String, String>,
    /// What Deckle last wrote: calibre uuid → column label → value.
    pub written: BTreeMap<String, BTreeMap<String, String>>,
    /// When the last batch ran, and what it said.
    pub last_run: Option<String>,
    pub last_result: Vec<String>,
}

impl Writeback {
    pub fn load(path: &Path) -> Writeback {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        write_json(path, self)
    }
}

/// The calibre library written to: the one Settings → Books names, else `~/Calibre Library`,
/// when it has a `metadata.db`. (Unlike finding originals, it doesn't wait on that switch.)
pub fn library_root(cfg: &crate::LibraryConfig, home: Option<&Path>) -> Option<PathBuf> {
    cfg.calibre
        .clone()
        .or_else(|| home.map(|h| h.join("Calibre Library")))
        .filter(|r| r.join("metadata.db").is_file())
}

/// The content server's password: `DECKLE_CALIBRE_PASSWORD`, else what the caller found in
/// the system keychain.
pub const KEYCHAIN_SERVICE: &str = "com.casavargas.deckle.calibre";

/// `calibre-writeback.json` beside `library.json` (`config`).
pub fn config_path(config: &Path) -> PathBuf {
    config.with_file_name("calibre-writeback.json")
}

/// `reading-log.json` beside `library.json`.
pub fn log_path(config: &Path) -> PathBuf {
    config.with_file_name("reading-log.json")
}

fn write_json(path: &Path, v: &impl Serialize) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(v)?)?;
    std::fs::rename(tmp, path)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    Progress,
    Read,
    LastRead,
    Rating,
}

impl Field {
    /// The calibre column types that can hold it.
    pub fn fits(self, datatype: &str) -> bool {
        match self {
            Field::Progress => matches!(datatype, "int" | "float"),
            Field::Read => datatype == "bool",
            Field::LastRead => datatype == "datetime",
            Field::Rating => matches!(datatype, "rating" | "int"),
        }
    }
}

/// One value to write.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Change {
    pub uuid: String,
    pub book_id: i64,
    pub title: String,
    pub field: Field,
    pub column: String,
    pub old: Option<String>,
    pub new: String,
}

/// A value someone set in calibre since Deckle last wrote it: left alone.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Conflict {
    pub title: String,
    pub field: Field,
    pub column: String,
    /// What's in calibre.
    pub calibre: String,
    /// What the reader says.
    pub reader: String,
}

/// A book matched only by title and author: written once the owner says it's the same book.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ToConfirm {
    pub key: String,
    pub reader: String,
    pub title: String,
    pub authors: Vec<String>,
    pub uuid: String,
    pub calibre_title: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Plan {
    pub changes: Vec<Change>,
    pub conflicts: Vec<Conflict>,
    pub to_confirm: Vec<ToConfirm>,
    /// Books on a reader that aren't in calibre (by title).
    pub unmatched: Vec<String>,
}

fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn same_book(r: &Record, b: &calibre::Book) -> bool {
    let first = |a: &[String]| a.first().map(|s| norm(s)).unwrap_or_default();
    !r.title.trim().is_empty()
        && norm(&r.title) == norm(&b.title)
        && first(&r.authors) == first(&b.authors)
}

/// The value `field` takes from `r` in a column of `datatype`; `None`: the reader doesn't say.
fn value_of(field: Field, datatype: &str, r: &Record) -> Option<String> {
    match field {
        Field::Progress => {
            let p = if r.finished { 100.0 } else { r.percent? };
            Some(if datatype == "float" {
                format!("{:.1}", p)
            } else {
                format!("{}", p.round() as i64)
            })
        }
        Field::Read => Some(r.finished.to_string()),
        Field::LastRead => r.last_read.clone(),
        Field::Rating => {
            let stars = r.rating?;
            // calibre keeps a rating column in half-stars, 0–10.
            Some(if datatype == "rating" {
                (stars * 2).to_string()
            } else {
                stars.to_string()
            })
        }
    }
}

/// Whether calibre's `current` and Deckle's `new` say the same.
fn same_value(datatype: &str, current: &str, new: &str) -> bool {
    match datatype {
        "int" | "float" | "rating" => match (current.parse::<f64>(), new.parse::<f64>()) {
            (Ok(a), Ok(b)) => (a - b).abs() < 0.05,
            _ => current == new,
        },
        // calibre may keep a time with its offset or fraction: compare to the second.
        "datetime" => {
            current.get(..19).map(|c| c.replace(' ', "T")) == new.get(..19).map(str::to_owned)
        }
        _ => current.eq_ignore_ascii_case(new),
    }
}

/// What a batch would write now. `library` is the calibre library's folder.
pub fn plan(log: &ReadLog, library: &Path, wb: &Writeback) -> Result<Plan, String> {
    let columns = calibre::columns(library)?;
    let books = calibre::books(library)?;
    let mut plan = Plan::default();

    // Each calibre book's newest record, from whichever reader read it last.
    let mut newest: BTreeMap<String, (&Record, &calibre::Book)> = BTreeMap::new();
    for (key, l) in &log.books {
        let r = &l.record;
        let by_uuid = |u: &str| books.iter().find(|b| b.uuid == u);
        let found = r
            .calibre_id
            .as_deref()
            .and_then(by_uuid)
            .or_else(|| wb.confirmed.get(key).and_then(|u| by_uuid(u)));
        let book = match found {
            Some(b) => b,
            None => {
                let like: Vec<&calibre::Book> = books.iter().filter(|b| same_book(r, b)).collect();
                match like.as_slice() {
                    [one] => plan.to_confirm.push(ToConfirm {
                        key: key.clone(),
                        reader: l.reader.clone(),
                        title: r.title.clone(),
                        authors: r.authors.clone(),
                        uuid: one.uuid.clone(),
                        calibre_title: one.title.clone(),
                    }),
                    _ => plan.unmatched.push(r.title.clone()),
                }
                continue;
            }
        };
        let keep = newest
            .get(&book.uuid)
            .is_none_or(|(old, _)| old.last_read < r.last_read);
        if keep {
            newest.insert(book.uuid.clone(), (r, book));
        }
    }

    for (uuid, (r, book)) in newest {
        let written = wb.written.get(&uuid);
        for (field, label) in wb.columns.pairs() {
            let Some(label) = label else { continue };
            let Some(col) = columns.iter().find(|c| &c.label == label) else {
                continue;
            };
            let Some(new) = value_of(field, &col.datatype, r) else {
                continue;
            };
            let current = calibre::value(library, col, book.id)?;
            let last = written.and_then(|w| w.get(label));
            if current
                .as_deref()
                .is_some_and(|c| same_value(&col.datatype, c, &new))
            {
                continue;
            }
            // A value Deckle didn't put there (or changed since) is the owner's.
            if let Some(c) = &current
                && last.is_none_or(|l| !same_value(&col.datatype, c, l))
            {
                plan.conflicts.push(Conflict {
                    title: book.title.clone(),
                    field,
                    column: label.clone(),
                    calibre: c.clone(),
                    reader: new,
                });
                continue;
            }
            // Progress only goes forward, unless the book was started again.
            if field == Field::Progress
                && let (Some(c), Ok(n)) = (
                    current.as_deref().and_then(|c| c.parse::<f64>().ok()),
                    new.parse::<f64>(),
                )
                && n < c
                && !(written
                    .and_then(|w| wb.columns.read.as_ref().and_then(|rl| w.get(rl)))
                    .map(String::as_str)
                    == Some("true")
                    && !r.finished)
            {
                continue;
            }
            plan.changes.push(Change {
                uuid: uuid.clone(),
                book_id: book.id,
                title: book.title.clone(),
                field,
                column: label.clone(),
                old: current,
                new,
            });
        }
    }
    plan.unmatched.sort();
    plan.unmatched.dedup();
    Ok(plan)
}

/// Where calibre's `calibredb` is: the configured one, else on `PATH`, else where calibre
/// installs it on this system.
pub fn find_calibredb(configured: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = configured {
        return p.is_file().then(|| p.to_path_buf());
    }
    let exe = if cfg!(windows) {
        "calibredb.exe"
    } else {
        "calibredb"
    };
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let p = dir.join(exe);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    [
        "/Applications/calibre.app/Contents/MacOS/calibredb",
        "C:\\Program Files\\Calibre2\\calibredb.exe",
        "/usr/bin/calibredb",
        "/opt/calibre/calibredb",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|p| p.is_file())
}

/// How a `calibredb` run went.
#[derive(Debug, PartialEq, Eq)]
pub enum RunError {
    /// The calibre app (or `calibre-server`) has the library open and no content server is set.
    CalibreOpen,
    Failed(String),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::CalibreOpen => write!(
                f,
                "calibre is open: turn on its content server and give Deckle its address, or close calibre"
            ),
            RunError::Failed(why) => write!(f, "{why}"),
        }
    }
}

/// Runs `calibredb` with `args`; the error is what it printed.
pub type Runner<'a> = dyn FnMut(&Path, &[String]) -> Result<String, String> + 'a;

/// `calibredb` started for real, stopped after [`TIMEOUT`].
pub fn system_runner(exe: &Path, args: &[String]) -> Result<String, String> {
    use std::io::Read;
    let mut child = std::process::Command::new(exe)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("{}: {e}", exe.display()))?;
    let start = std::time::Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? {
            break s;
        }
        if start.elapsed() > TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "calibredb took over {}s and was stopped",
                TIMEOUT.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let (mut out, mut err) = (String::new(), String::new());
    if let Some(mut o) = child.stdout.take() {
        let _ = o.read_to_string(&mut out);
    }
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_string(&mut err);
    }
    if status.success() {
        Ok(out)
    } else {
        Err(format!("{}\n{}", err.trim(), out.trim()).trim().to_owned())
    }
}

/// The `--with-library` (and sign-in) arguments: the content server when one is set, else the
/// library's folder.
pub fn library_args(wb: &Writeback, library: &Path, password: Option<&str>) -> Vec<String> {
    let mut a = vec!["--with-library".to_owned()];
    match wb.server.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(url) => {
            a.push(url.trim().to_owned());
            if let Some(u) = wb.username.as_deref().filter(|u| !u.is_empty()) {
                a.push("--username".into());
                a.push(u.to_owned());
                if let Some(p) = password {
                    a.push("--password".into());
                    a.push(p.to_owned());
                }
            }
        }
        None => a.push(library.to_string_lossy().into_owned()),
    }
    a
}

fn classify(e: String) -> RunError {
    // calibredb refuses a library another calibre program has open.
    if e.contains("Another calibre program") || e.contains("another calibre program") {
        RunError::CalibreOpen
    } else {
        RunError::Failed(e)
    }
}

/// What a batch did.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Report {
    pub written: usize,
    pub failed: Vec<String>,
    /// The batch stopped here (calibre open); what's left goes next time.
    pub stopped: Option<String>,
}

/// Writes `plan`'s changes with `calibredb set_custom`, recording each in `wb.written`.
pub fn run(
    plan: &Plan,
    wb: &mut Writeback,
    exe: &Path,
    library: &Path,
    password: Option<&str>,
    runner: &mut Runner<'_>,
) -> Report {
    let mut report = Report::default();
    let base = library_args(wb, library, password);
    for c in &plan.changes {
        let mut args = vec!["set_custom".to_owned()];
        args.extend(base.iter().cloned());
        args.extend([c.column.clone(), c.book_id.to_string(), c.new.clone()]);
        match runner(exe, &args).map_err(classify) {
            Ok(_) => {
                wb.written
                    .entry(c.uuid.clone())
                    .or_default()
                    .insert(c.column.clone(), c.new.clone());
                report.written += 1;
            }
            Err(RunError::CalibreOpen) => {
                report.stopped = Some(RunError::CalibreOpen.to_string());
                break;
            }
            Err(RunError::Failed(why)) => report
                .failed
                .push(format!("{} · #{}: {why}", c.title, c.column)),
        }
    }
    wb.last_run = Some(deckle_core::devices::now_iso());
    wb.last_result = std::iter::once(format!("{} written", report.written))
        .chain(report.stopped.clone())
        .chain(report.failed.iter().cloned())
        .collect();
    report
}

/// The columns "Create the usual columns" adds: `(label, heading, datatype)`.
pub const USUAL: [(&str, &str, &str); 4] = [
    ("deckle_progress", "Progress (Deckle)", "int"),
    ("deckle_read", "Read (Deckle)", "bool"),
    ("deckle_last_read", "Last read (Deckle)", "datetime"),
    ("deckle_rating", "Stars (Deckle)", "rating"),
];

/// Adds [`USUAL`]'s columns that aren't there, with `calibredb add_custom_column`, and maps
/// them. Changes the library's layout, so it needs calibre closed (no content server).
pub fn create_usual(
    wb: &mut Writeback,
    exe: &Path,
    library: &Path,
    runner: &mut Runner<'_>,
) -> Result<Vec<String>, RunError> {
    let have: Vec<String> = calibre::columns(library)
        .map_err(RunError::Failed)?
        .into_iter()
        .map(|c| c.label)
        .collect();
    let mut added = Vec::new();
    for (label, name, datatype) in USUAL {
        if !have.iter().any(|h| h == label) {
            let args = vec![
                "add_custom_column".to_owned(),
                "--with-library".into(),
                library.to_string_lossy().into_owned(),
                label.into(),
                name.into(),
                datatype.into(),
            ];
            runner(exe, &args).map_err(classify)?;
            added.push(label.to_owned());
        }
    }
    wb.columns = Columns {
        progress: Some(USUAL[0].0.into()),
        read: Some(USUAL[1].0.into()),
        last_read: Some(USUAL[2].0.into()),
        rating: Some(USUAL[3].0.into()),
    };
    Ok(added)
}

/// The columns each field could go in.
pub fn choices(columns: &[Column]) -> BTreeMap<&'static str, Vec<String>> {
    let mut out = BTreeMap::new();
    for (name, field) in [
        ("progress", Field::Progress),
        ("read", Field::Read),
        ("last_read", Field::LastRead),
        ("rating", Field::Rating),
    ] {
        out.insert(
            name,
            columns
                .iter()
                .filter(|c| !c.is_multiple && field.fits(&c.datatype))
                .map(|c| c.label.clone())
                .collect(),
        );
    }
    out
}

/// Where things stand: the library, `calibredb`, and the next batch.
#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub library: Option<PathBuf>,
    pub calibredb: Option<PathBuf>,
    pub writeback: Writeback,
    /// Each field's possible columns.
    pub choices: BTreeMap<&'static str, Vec<String>>,
    /// The next batch; `None` when there's no library to plan against.
    pub plan: Option<Plan>,
    pub error: Option<String>,
}

/// The files and library the write-back works with, from `library.json`'s path.
pub struct Paths {
    pub config: PathBuf,
    pub log: PathBuf,
    pub library: Option<PathBuf>,
}

impl Paths {
    pub fn new(library_json: &Path, home: Option<&Path>) -> Paths {
        Paths {
            config: config_path(library_json),
            log: log_path(library_json),
            library: library_root(&crate::LibraryConfig::load(library_json), home),
        }
    }
}

/// The page's state, the next batch planned (nothing written).
pub fn status(p: &Paths) -> Status {
    let wb = Writeback::load(&p.config);
    let calibredb = find_calibredb(wb.calibredb.as_deref());
    let mut out = Status {
        library: p.library.clone(),
        calibredb,
        choices: BTreeMap::new(),
        writeback: wb,
        plan: None,
        error: None,
    };
    let Some(lib) = &p.library else {
        out.error = Some("no calibre library found: name it in Settings → Books".into());
        return out;
    };
    match calibre::columns(lib) {
        Ok(cols) => out.choices = choices(&cols),
        Err(e) => out.error = Some(e),
    }
    match plan(&ReadLog::load(&p.log), lib, &out.writeback) {
        Ok(pl) => out.plan = Some(pl),
        Err(e) => out.error = Some(e),
    }
    out
}

/// Plans and runs a batch, saving what it wrote. `Ok(None)`: nothing to write.
pub fn write_now(
    p: &Paths,
    password: Option<&str>,
    runner: &mut Runner<'_>,
) -> Result<Option<Report>, String> {
    let mut wb = Writeback::load(&p.config);
    let lib = p.library.as_deref().ok_or("no calibre library found")?;
    let exe = find_calibredb(wb.calibredb.as_deref())
        .ok_or("calibredb wasn't found: install calibre, or say where calibredb is")?;
    let pl = plan(&ReadLog::load(&p.log), lib, &wb)?;
    if pl.changes.is_empty() {
        return Ok(None);
    }
    let report = run(&pl, &mut wb, &exe, lib, password, runner);
    wb.save(&p.config).map_err(|e| e.to_string())?;
    Ok(Some(report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calibre::fixture;
    use rusqlite::types::Value as V;

    fn rec(
        title: &str,
        author: &str,
        id: Option<&str>,
        percent: f64,
        finished: bool,
        when: &str,
    ) -> Record {
        Record {
            title: title.into(),
            authors: vec![author.into()],
            calibre_id: id.map(str::to_owned),
            path: Some(format!("{author}/{title}.epub")),
            percent: Some(percent),
            finished,
            last_read: Some(when.into()),
            rating: None,
            from: "koreader".into(),
        }
    }

    fn library(root: &Path) {
        fixture::library(
            root,
            &[
                ("progress", "Progress", "int", false),
                ("read", "Read", "bool", false),
                ("last", "Last read", "datetime", false),
                ("stars", "Stars", "rating", true),
                ("tags2", "Shelves", "text", false),
            ],
            &[
                (1, "u-dune", "Dune", "Frank Herbert"),
                (2, "u-emma", "Emma", "Jane Austen"),
                (3, "u-hail", "Project Hail Mary", "Andy Weir"),
            ],
        );
    }

    fn mapped() -> Writeback {
        Writeback {
            enabled: true,
            columns: Columns {
                progress: Some("progress".into()),
                read: Some("read".into()),
                last_read: Some("last".into()),
                rating: Some("stars".into()),
            },
            ..Default::default()
        }
    }

    #[test]
    fn the_newest_record_of_each_book_becomes_its_changes() {
        let tmp = tempfile::tempdir().unwrap();
        library(tmp.path());
        let mut log = ReadLog::default();
        log.record(
            "kobo/1",
            &[rec(
                "Dune",
                "Frank Herbert",
                Some("u-dune"),
                40.0,
                false,
                "2026-10-01T00:00:00Z",
            )],
        );
        let mut r = rec(
            "Dune",
            "Frank Herbert",
            Some("u-dune"),
            62.4,
            false,
            "2026-10-30T09:00:00Z",
        );
        r.rating = Some(4);
        log.record("koreader/a", &[r]);
        let p = plan(&log, tmp.path(), &mapped()).unwrap();
        let got: Vec<(&str, &str)> = p
            .changes
            .iter()
            .map(|c| (c.column.as_str(), c.new.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("progress", "62"),
                ("read", "false"),
                ("last", "2026-10-30T09:00:00Z"),
                ("stars", "8")
            ]
        );
        assert!(p.changes.iter().all(|c| c.book_id == 1 && c.old.is_none()));
    }

    #[test]
    fn a_value_the_owner_set_is_a_conflict_and_deckles_own_is_updated() {
        let tmp = tempfile::tempdir().unwrap();
        library(tmp.path());
        // The owner marked Dune read in calibre; Deckle wrote 30% before.
        fixture::set(tmp.path(), 2, 1, V::Integer(1));
        fixture::set(tmp.path(), 1, 1, V::Integer(30));
        let mut wb = mapped();
        wb.written.insert(
            "u-dune".into(),
            [("progress".to_owned(), "30".to_owned())].into(),
        );
        let mut log = ReadLog::default();
        log.record(
            "r",
            &[rec(
                "Dune",
                "Frank Herbert",
                Some("u-dune"),
                50.0,
                false,
                "2026-10-30T00:00:00Z",
            )],
        );
        let p = plan(&log, tmp.path(), &wb).unwrap();
        assert_eq!(p.conflicts.len(), 1);
        assert_eq!(
            (
                p.conflicts[0].column.as_str(),
                p.conflicts[0].calibre.as_str()
            ),
            ("read", "true")
        );
        let progress = p.changes.iter().find(|c| c.column == "progress").unwrap();
        assert_eq!(
            (progress.old.as_deref(), progress.new.as_str()),
            (Some("30"), "50")
        );

        // Someone changed Deckle's 30 to 35 in calibre: that's theirs now.
        fixture::set(tmp.path(), 1, 1, V::Integer(35));
        let p = plan(&log, tmp.path(), &wb).unwrap();
        assert!(p.changes.iter().all(|c| c.column != "progress"));
        assert!(p.conflicts.iter().any(|c| c.column == "progress"));
    }

    #[test]
    fn progress_goes_only_forward_unless_the_book_was_started_again() {
        let tmp = tempfile::tempdir().unwrap();
        library(tmp.path());
        fixture::set(tmp.path(), 1, 1, V::Integer(80));
        let mut wb = mapped();
        wb.written.insert(
            "u-dune".into(),
            [("progress".to_owned(), "80".to_owned())].into(),
        );
        let mut log = ReadLog::default();
        log.record(
            "r",
            &[rec(
                "Dune",
                "Frank Herbert",
                Some("u-dune"),
                20.0,
                false,
                "2026-10-30T00:00:00Z",
            )],
        );
        let p = plan(&log, tmp.path(), &wb).unwrap();
        assert!(p.changes.iter().all(|c| c.column != "progress"));

        // Deckle had marked it read; now the reader says not finished: started again.
        fixture::set(tmp.path(), 2, 1, V::Integer(1));
        wb.written
            .get_mut("u-dune")
            .unwrap()
            .insert("read".into(), "true".into());
        let p = plan(&log, tmp.path(), &wb).unwrap();
        let progress = p.changes.iter().find(|c| c.column == "progress").unwrap();
        assert_eq!(progress.new, "20");
        assert!(
            p.changes
                .iter()
                .any(|c| c.column == "read" && c.new == "false")
        );
    }

    #[test]
    fn a_book_without_calibres_id_is_asked_about_once_then_written() {
        let tmp = tempfile::tempdir().unwrap();
        library(tmp.path());
        let mut log = ReadLog::default();
        log.record(
            "kobo/1",
            &[
                rec(
                    "Project Hail Mary",
                    "Andy Weir",
                    None,
                    100.0,
                    true,
                    "2026-10-29T00:00:00Z",
                ),
                rec(
                    "Not In Calibre",
                    "Nobody",
                    None,
                    10.0,
                    false,
                    "2026-10-29T00:00:00Z",
                ),
            ],
        );
        let mut wb = mapped();
        let p = plan(&log, tmp.path(), &wb).unwrap();
        assert!(p.changes.is_empty());
        assert_eq!(p.unmatched, ["Not In Calibre"]);
        assert_eq!(p.to_confirm.len(), 1);
        assert_eq!(p.to_confirm[0].uuid, "u-hail");
        wb.confirmed
            .insert(p.to_confirm[0].key.clone(), "u-hail".into());
        let p = plan(&log, tmp.path(), &wb).unwrap();
        assert!(p.to_confirm.is_empty());
        let read = p.changes.iter().find(|c| c.column == "read").unwrap();
        assert_eq!((read.book_id, read.new.as_str()), (3, "true"));
        assert!(
            p.changes
                .iter()
                .any(|c| c.column == "progress" && c.new == "100")
        );
    }

    #[test]
    fn a_run_calls_set_custom_records_what_it_wrote_and_stops_when_calibre_is_open() {
        let tmp = tempfile::tempdir().unwrap();
        library(tmp.path());
        let mut log = ReadLog::default();
        log.record(
            "r",
            &[rec(
                "Dune",
                "Frank Herbert",
                Some("u-dune"),
                50.0,
                false,
                "2026-10-30T00:00:00Z",
            )],
        );
        let mut wb = mapped();
        let p = plan(&log, tmp.path(), &wb).unwrap();
        let mut calls: Vec<Vec<String>> = Vec::new();
        let mut ok = |_: &Path, a: &[String]| -> Result<String, String> {
            calls.push(a.to_vec());
            Ok(String::new())
        };
        let report = run(
            &p,
            &mut wb,
            Path::new("calibredb"),
            tmp.path(),
            None,
            &mut ok,
        );
        assert_eq!(report.written, 3);
        assert_eq!(
            calls[0],
            [
                "set_custom",
                "--with-library",
                &tmp.path().to_string_lossy(),
                "progress",
                "1",
                "50"
            ]
        );
        assert_eq!(wb.written["u-dune"]["progress"], "50");
        assert_eq!(wb.last_result[0], "3 written");

        // Written: the same plan again changes nothing.
        fixture::set(tmp.path(), 1, 1, V::Integer(50));
        fixture::set(tmp.path(), 2, 1, V::Integer(0));
        fixture::set(
            tmp.path(),
            3,
            1,
            V::Text("2026-10-30T00:00:00+00:00".into()),
        );
        assert!(plan(&log, tmp.path(), &wb).unwrap().changes.is_empty());

        // calibre open, through its content server it goes.
        wb.server = Some("http://localhost:8080/#Books".into());
        wb.username = Some("me".into());
        assert_eq!(
            library_args(&wb, tmp.path(), Some("pw")),
            [
                "--with-library",
                "http://localhost:8080/#Books",
                "--username",
                "me",
                "--password",
                "pw"
            ]
        );
        let mut open = |_: &Path, _: &[String]| -> Result<String, String> {
            Err("Another calibre program such as calibre-server or the main calibre program is running.".into())
        };
        let mut wb2 = mapped();
        let report = run(
            &p,
            &mut wb2,
            Path::new("calibredb"),
            tmp.path(),
            None,
            &mut open,
        );
        assert_eq!(report.written, 0);
        assert!(report.stopped.unwrap().contains("calibre is open"));
        assert!(wb2.written.is_empty());
    }

    #[test]
    fn the_usual_columns_are_added_once_and_mapped() {
        let tmp = tempfile::tempdir().unwrap();
        fixture::library(
            tmp.path(),
            &[("deckle_read", "Read (Deckle)", "bool", false)],
            &[],
        );
        let mut wb = Writeback::default();
        let mut calls = Vec::new();
        let mut ok = |_: &Path, a: &[String]| -> Result<String, String> {
            calls.push(a[3].clone());
            Ok(String::new())
        };
        let added = create_usual(&mut wb, Path::new("calibredb"), tmp.path(), &mut ok).unwrap();
        assert_eq!(
            added,
            ["deckle_progress", "deckle_last_read", "deckle_rating"]
        );
        assert_eq!(calls, added);
        assert_eq!(wb.columns.read.as_deref(), Some("deckle_read"));
        let cols = calibre::columns(tmp.path()).unwrap();
        assert_eq!(choices(&cols)["read"], ["deckle_read"]);
        assert!(choices(&cols)["progress"].is_empty());
    }

    #[test]
    fn the_log_keeps_each_readers_newest_record() {
        let mut log = ReadLog::default();
        let old = rec(
            "Dune",
            "Frank Herbert",
            None,
            10.0,
            false,
            "2026-10-01T00:00:00Z",
        );
        let new = rec(
            "Dune",
            "Frank Herbert",
            None,
            20.0,
            false,
            "2026-10-02T00:00:00Z",
        );
        assert_eq!(log.record("a", std::slice::from_ref(&new)), 1);
        assert_eq!(
            log.record("a", &[old]),
            0,
            "an older record doesn't replace a newer one"
        );
        assert_eq!(
            log.record("a", std::slice::from_ref(&new)),
            0,
            "nothing new"
        );
        assert_eq!(log.record("b", &[new]), 1, "another reader's is its own");
        assert_eq!(log.books.len(), 2);
    }
}
