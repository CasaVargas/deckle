// SPDX-License-Identifier: GPL-3.0-or-later
//! The outbox: finished KFX files waiting for a Kindle. Built when no Kindle is connected,
//! delivered by the next `push` (or `watch`) over one connection, which also prunes old
//! editions in that same session.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::deliver::{
    Conn, DeliverError, DeliverOptions, Target, deliver_named_on, prune_named_on,
};

/// A folder of `<name>.kfx` files, delivered under the same names.
#[derive(Clone, Debug)]
pub struct Queue {
    pub dir: PathBuf,
}

/// Where a run of [`Queue::flush`] got to.
#[derive(Clone, Debug, Default, Serialize)]
pub struct FlushReport {
    /// Device paths written.
    pub delivered: Vec<PathBuf>,
    /// Old editions removed from the device.
    pub pruned: Vec<String>,
}

/// After a flush, editions named `<prefix>…` beyond the newest `keep` are removed.
#[derive(Clone, Debug)]
pub struct Prune {
    pub prefix: String,
    pub keep: usize,
}

impl Queue {
    /// `<cache dir>/outbox`.
    pub fn default_dir() -> PathBuf {
        crate::wordwise::default_cache_dir().join("outbox")
    }

    pub fn new(dir: PathBuf) -> Self {
        Queue { dir }
    }

    /// Queued files, oldest name first.
    pub fn pending(&self) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "kfx"))
            .collect();
        files.sort();
        files
    }

    /// Copies `kfx` into the queue as `name`, first dropping queued files that `replaces`
    /// (an edition supersedes yesterday's undelivered one).
    pub fn add(&self, kfx: &Path, name: &str, replaces: Option<&str>) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(&self.dir)?;
        if let Some(prefix) = replaces {
            for old in self.pending() {
                if old
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with(prefix))
                {
                    std::fs::remove_file(old)?;
                }
            }
        }
        let dest = self.dir.join(name);
        std::fs::copy(kfx, &dest)?;
        Ok(dest)
    }

    /// Delivers every queued file to `target` over one connection, then prunes, then removes
    /// the delivered files from the queue. A file stays queued if its delivery fails.
    pub fn flush(
        &self,
        target: &Target,
        prune: Option<&Prune>,
    ) -> Result<FlushReport, DeliverError> {
        self.flush_on(&mut Conn::new(target), prune)
    }

    /// [`Queue::flush`] over the caller's connection.
    pub fn flush_on(
        &self,
        conn: &mut Conn<'_>,
        prune: Option<&Prune>,
    ) -> Result<FlushReport, DeliverError> {
        let mut report = FlushReport::default();
        for file in self.pending() {
            let io = |source| DeliverError::Io {
                path: file.clone(),
                source,
            };
            let bytes = std::fs::read(&file).map_err(io)?;
            let name = file
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let path = deliver_named_on(&bytes, &name, conn, &DeliverOptions::default())?;
            report.delivered.push(path);
            std::fs::remove_file(&file).map_err(io)?;
        }
        if let Some(p) = prune {
            report.pruned = prune_named_on(conn, &p.prefix, p.keep)?;
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_edition_replaces_the_undelivered_one_and_flush_prunes() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("book.kfx");
        std::fs::write(&src, b"KFX").unwrap();
        let q = Queue::new(tmp.path().join("outbox"));
        q.add(&src, "deckle-daily-2026-10-19.kfx", Some("deckle-daily-"))
            .unwrap();
        q.add(&src, "deckle-daily-2026-10-20.kfx", Some("deckle-daily-"))
            .unwrap();
        q.add(&src, "other.kfx", None).unwrap();
        let names: Vec<_> = q
            .pending()
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["deckle-daily-2026-10-20.kfx", "other.kfx"]);

        let kindle = tmp.path().join("kindle");
        std::fs::create_dir_all(&kindle).unwrap();
        for old in ["deckle-daily-2026-10-17.kfx", "deckle-daily-2026-10-18.kfx"] {
            std::fs::write(kindle.join(old), b"KFX").unwrap();
        }
        let prune = Prune {
            prefix: "deckle-daily-".into(),
            keep: 2,
        };
        let r = q
            .flush(&Target::Folder(kindle.clone()), Some(&prune))
            .unwrap();
        assert_eq!(r.delivered.len(), 2);
        assert_eq!(r.pruned, ["deckle-daily-2026-10-17.kfx"]);
        assert!(q.pending().is_empty());
        assert!(kindle.join("deckle-daily-2026-10-20.kfx").exists());
        assert!(kindle.join("other.kfx").exists());
    }
}
