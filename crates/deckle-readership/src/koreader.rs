// SPDX-License-Identifier: GPL-3.0-or-later
//! KOReader's reading positions as reads (KOReader hub design §C): the spine file an xpointer
//! names, set against where each section of the edition starts.

use chrono::NaiveDate;

use crate::state::State;

/// The 0-based spine index in a KOReader EPUB position: `/body/DocFragment[N]/…` is file `N - 1`.
pub fn spine_index(xpointer: &str) -> Option<usize> {
    let rest = &xpointer[xpointer.find("DocFragment[")? + "DocFragment[".len()..];
    let n: usize = rest[..rest.find(']')?].trim().parse().ok()?;
    n.checked_sub(1)
}

/// Records where KOReader is in the edition of `date`, as `device`'s read of it.
///
/// - `device`: the reads' device id, `koreader/<device_id>` from the sync server and
///   `koreader/<drive id>` from a drive.
/// - `xpointer`: KOReader's position, `/body/DocFragment[N]/…` for an EPUB. The read lands
///   just past the start of the section whose file it is, in the coordinates the edition's
///   layout uses (KFX positions when a KFX was built, else the EPUB's).
/// - `percentage`: how far through, 0–1. Used only when the xpointer can't be placed (an
///   edition recorded before its EPUB spine was kept, or another kind of position).
/// - `when`: when KOReader was there, ISO 8601 UTC (`2026-10-30T07:45:00Z`).
///
/// The furthest point only moves forward; an older position than the one recorded changes
/// nothing else. Returns `false`, recording nothing, when Deckle has no layout for `date` or
/// the position can't be placed. The caller saves the state.
pub fn record_koreader_position(
    state: &mut State,
    date: NaiveDate,
    device: &str,
    xpointer: &str,
    percentage: Option<f64>,
    when: &str,
) -> bool {
    let Some(layout) = state.editions.get(&date) else {
        return false;
    };
    let by_spine = layout.epub.as_ref().and_then(|epub| {
        let section = epub.section_at(spine_index(xpointer)?)?;
        let start = layout.sections.iter().find(|s| s.key == section.key)?.start;
        Some(start + 1)
    });
    let by_percent = percentage
        .filter(|p| p.is_finite())
        .map(|p| (p.clamp(0.0, 1.0) * layout.end as f64) as i64);
    let Some(pos) = by_spine.or(by_percent) else {
        return false;
    };
    let read = state
        .reads
        .entry(date)
        .or_default()
        .entry(device.to_owned())
        .or_default();
    read.opened = true;
    read.furthest = read.furthest.max(Some(pos));
    if read.last_read.as_deref().is_none_or(|t| t <= when) {
        read.last = Some(pos);
        read.last_read = Some(when.to_owned());
        read.seen = when.to_owned();
    }
    true
}

/// The edition a file on a reader is, among those `state` has a layout for: its stem
/// (`deckle-daily-2026-10-30` for `Deckle/deckle-daily-2026-10-30.kepub.epub`) is the
/// layout's `file`.
pub fn edition_of(state: &State, book: &str) -> Option<NaiveDate> {
    let name = book.rsplit('/').next().unwrap_or(book);
    let stem = [".kepub.epub", ".epub"]
        .iter()
        .find_map(|x| name.strip_suffix(x))?;
    state
        .editions
        .values()
        .find(|l| l.file == stem)
        .map(|l| l.date)
}
