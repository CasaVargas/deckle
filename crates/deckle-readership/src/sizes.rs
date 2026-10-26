// SPDX-License-Identifier: GPL-3.0-or-later
//! What tomorrow's paper needs from Readership: how long each section ran, and how the last
//! week's papers were read (tomorrow's paper design §6.3, §7).

use std::collections::BTreeMap;

use chrono::{Days, NaiveDate};
use deckle_digest::next::{History, WeekDay, WeekState};

use crate::layout::Layout;
use crate::report::median;
use crate::state::State;

/// Layouts the sizes are taken over.
pub const SIZE_WINDOW: usize = 14;

/// Each section's length in a layout: the next start minus its own, or the end for the last.
fn lengths(l: &Layout) -> impl Iterator<Item = (&str, i64, Option<u32>)> {
    l.sections.iter().enumerate().map(|(i, s)| {
        let next = l.sections.get(i + 1).map_or(l.end, |n| n.start);
        (s.key.as_str(), next - s.start, s.items)
    })
}

/// Median lengths over the newest [`SIZE_WINDOW`] layouts: per section, and per item where the
/// layout knows its item count.
pub fn history(state: &State) -> History {
    let mut whole: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut per_item: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for l in state.editions.values().rev().take(SIZE_WINDOW) {
        for (key, len, items) in lengths(l) {
            whole.entry(key.to_owned()).or_default().push(len as f64);
            if let Some(n) = items.filter(|n| *n > 0) {
                per_item
                    .entry(key.to_owned())
                    .or_default()
                    .push(len as f64 / f64::from(n));
            }
        }
    }
    let medians = |m: BTreeMap<String, Vec<f64>>| {
        m.into_iter()
            .filter_map(|(k, v)| Some((k, median(v)?)))
            .collect()
    };
    History {
        per_item: medians(per_item),
        whole: medians(whole),
    }
}

/// The `days` dates before `before`, oldest first, each with what became of its paper. `built`
/// says whether an edition was made that day without a layout to show for it.
pub fn recent(
    state: &State,
    before: NaiveDate,
    days: usize,
    built: &dyn Fn(NaiveDate) -> bool,
) -> Vec<WeekDay> {
    (1..=days as u64)
        .rev()
        .map(|back| {
            let date = before - Days::new(back);
            let state = match state.editions.get(&date) {
                Some(l) => {
                    let furthest = state
                        .reads
                        .get(&date)
                        .into_iter()
                        .flat_map(|by| by.values())
                        .filter(|r| r.opened)
                        .filter_map(|r| r.furthest)
                        .max();
                    match furthest {
                        Some(f) if l.end > 0 => {
                            let pct = (f as f64 / l.end as f64 * 100.0).round();
                            WeekState::Read(pct.clamp(0.0, 100.0) as u8)
                        }
                        _ => WeekState::NotOpened,
                    }
                }
                None if built(date) => WeekState::Sent,
                None => WeekState::NoPaper,
            };
            WeekDay { date, state }
        })
        .collect()
}

/// Tomorrow's paper from local files only (the CLI's `digest next` and the Daily tab): the
/// config's queue and stop, Readership's `state`, the live headline cache at `live`, and the
/// EPUBs `digest run` left in `built` (for "sent" in the week). Never the network.
pub fn next_paper(
    cfg: &deckle_digest::Config,
    now: chrono::DateTime<deckle_digest::chrono_tz::Tz>,
    at: Option<(u8, u8)>,
    state: &std::path::Path,
    live: &std::path::Path,
    built: &std::path::Path,
) -> deckle_digest::next::plan::NextPaper {
    use deckle_digest::next::live::LiveCache;
    use deckle_digest::reading::ReadingQueue;
    let stopped = deckle_digest::press::read_stop(&cfg.press_state(), now.date_naive());
    let edition = deckle_digest::press::press(at, stopped, now).edition;
    let st = State::load(state);
    let shares = crate::report(&st, cfg)
        .sections
        .into_iter()
        .filter_map(|s| Some((s.key, s.share?)))
        .collect();
    let sent = |d: NaiveDate| {
        built
            .join(format!("{}.epub", deckle_digest::edition_stem(cfg, d)))
            .exists()
    };
    let queue = ReadingQueue::new(
        cfg.reading
            .queue_path
            .clone()
            .unwrap_or_else(ReadingQueue::default_path),
    )
    .load();
    deckle_digest::next::plan::plan(&deckle_digest::next::plan::Inputs {
        cfg,
        now,
        at,
        stopped,
        queue: &queue,
        live: &LiveCache::load(live),
        history: &history(&st),
        shares: &shares,
        week: recent(&st, edition, 6, &sent),
    })
}
