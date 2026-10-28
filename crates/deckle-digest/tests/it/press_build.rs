// SPDX-License-Identifier: GPL-3.0-or-later
//! Fresh builds fetch again and keep all scheduled inputs and deliveries untouched.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use deckle_digest::config::{ExportConfig, ExportFormat, FeedConfig, PaperRef};
use deckle_digest::press::{FreshBuildContext, FreshIdentity, PressErrorCode, Uuid, build_fresh};
use deckle_digest::{Config, GatherOptions, SectionKind};
use sha2::{Digest, Sha256};

const FEED: &str = "https://example.invalid/feed";
const PAGE: &str = "https://example.invalid/long";

fn at(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
}
fn now() -> DateTime<Utc> {
    at("2026-10-27T16:00:00Z")
}
fn page() -> Vec<u8> {
    format!("<html><head><title>Saved long story</title></head><body><article><h1>Saved long story</h1>{}</article></body></html>",
        "<p>The town council debated the public library and its overdue roof repairs before voting on the budget.</p>".repeat(100)).into_bytes()
}
fn net(url: &str) -> Result<Vec<u8>, String> {
    match url {
        FEED => Ok(br#"<rss version="2.0"><channel><title>Wire</title><item><title>Library reopened</title><link>https://example.invalid/story</link><description>Library reopened with repairs complete.</description><pubDate>Tue, 27 Oct 2026 12:00:00 +0000</pubDate></item></channel></rss>"#.to_vec()),
        PAGE => Ok(page()),
        _ if url.starts_with("https://api.open-meteo.com/") => Ok(std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/weather.json")).unwrap()),
        _ if url.starts_with("https://archive-api.open-meteo.com/") => Ok(br#"{"daily":{"time":["2025-10-27"],"temperature_2m_min":[-1]}}"#.to_vec()),
        _ => Err("synthetic source unavailable".into()),
    }
}
fn put(root: &Path, path: &str, bytes: &[u8]) {
    let p = root.join(path);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}
fn snapshot(root: &Path) -> BTreeMap<PathBuf, (Vec<u8>, std::time::SystemTime)> {
    fn walk(root: &Path, p: &Path, out: &mut BTreeMap<PathBuf, (Vec<u8>, std::time::SystemTime)>) {
        for entry in std::fs::read_dir(p).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                out.insert(
                    p.strip_prefix(root).unwrap().into(),
                    (
                        std::fs::read(&p).unwrap(),
                        std::fs::metadata(p).unwrap().modified().unwrap(),
                    ),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}
fn config(root: &Path) -> Config {
    for p in [
        "Inbox/a.html",
        "Morgue/2026-10-27/edition.json",
        "layouts/2026-10-27.json",
        "exports/scheduled.deckle/edition.json",
        "device/queue.json",
        "email/queue.json",
        "preview/edition.json",
        "press.json",
    ] {
        put(root, p, b"scheduled sentinel");
    }
    let mut c = Config {
        paper_id: Some(Uuid::parse_str("ea249ae5-771c-4d2b-9f11-5119fbe3feda").unwrap()),
        sections: vec![
            SectionKind::Feeds,
            SectionKind::Puzzle,
            SectionKind::Shelf,
            SectionKind::Weather,
        ],
        feeds: vec![FeedConfig {
            name: "Wire".into(),
            url: FEED.into(),
            max_items: 10,
            ..Default::default()
        }],
        full_text: false,
        photos: false,
        latitude: Some(35.4),
        longitude: Some(-79.5),
        max_age_hours: 24 * 365,
        ..Default::default()
    };
    c.reading.inbox = root.join("Inbox").display().to_string();
    c.reading.queue_path = Some(root.join("reading.jsonl"));
    c.reading.long_read_words = 200;
    c.puzzle_path = Some(root.join("puzzles.json"));
    c.shelf_path = Some(root.join("shelf.json"));
    c.almanac_path = Some(root.join("almanac.json"));
    c.press_path = Some(root.join("press.json"));
    c.export = vec![ExportConfig {
        name: "Other delivery".into(),
        dir: root.join("exports").display().to_string(),
        format: ExportFormat::Pdf,
        books: true,
        ..Default::default()
    }];
    c.remarkable.enabled = true;
    // Pruning this old shelf marker would write the shared store in a normal gather.
    put(
        root,
        "shelf.json",
        br#"{"kindles":{},"shown":{"old":"2020-01-01"}}"#,
    );
    put(root, "almanac.json", b"{}");
    c
}
fn context(work: &Path, at: DateTime<Utc>, books: bool) -> FreshBuildContext {
    FreshBuildContext {
        identity: FreshIdentity {
            paper_id: Uuid::parse_str("ea249ae5-771c-4d2b-9f11-5119fbe3feda").unwrap(),
            edition_id: Uuid::new_v4(),
        },
        started_at: at,
        work_dir: work.into(),
        books,
    }
}
fn manifest(bundle: &Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(bundle.join("edition.json")).unwrap()).unwrap()
}

#[test]
fn fresh_bundle_is_14_and_protected_state_never_changes() {
    let d = tempfile::tempdir().unwrap();
    let protected = d.path().join("protected");
    let c = config(&protected);
    let before = snapshot(&protected);
    let ctx = context(&d.path().join("job"), now(), false);
    let result = build_fresh(&c, &ctx, &net, &GatherOptions::default()).unwrap();
    let v = manifest(&result.bundle);
    assert_eq!(v["version"], "1.4");
    assert_eq!(v["edition"]["id"], ctx.identity.edition_id.to_string());
    assert_eq!(v["edition"]["paper_id"], ctx.identity.paper_id.to_string());
    assert_eq!(v["edition"]["kind"], "fresh");
    assert_eq!(v["edition"]["built_at"], "2026-10-27T16:00:00Z");
    assert_eq!(
        result.bundle.file_name().unwrap(),
        format!("{}.deckle", ctx.identity.edition_id).as_str()
    );
    assert_eq!(
        result.manifest_sha256,
        Sha256::digest(std::fs::read(result.bundle.join("edition.json")).unwrap())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    assert!(deckle_digest::bundle::is_complete(&result.bundle));
    let schema: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../docs/formats/edition-bundle.schema.json"
    ))
    .unwrap();
    assert!(jsonschema::validator_for(&schema).unwrap().is_valid(&v));
    assert_eq!(before, snapshot(&protected));
    let frost: serde_json::Value =
        serde_json::from_slice(&std::fs::read(ctx.work_dir.join("inputs/almanac.json")).unwrap())
            .unwrap();
    assert!(
        frost.get("35.4,-79.5").is_some(),
        "gather writes its frost cache only in the job"
    );
    assert!(ctx.work_dir.join("inputs/puzzles.json").is_file());
    assert!(!ctx.work_dir.join("pdf").exists());
    assert!(!ctx.work_dir.join("color").exists());
    assert!(!ctx.work_dir.join("bundle/paper").exists());
}

#[test]
fn unchanged_sources_are_fetched_for_each_new_edition_and_partial_failure_survives() {
    let d = tempfile::tempdir().unwrap();
    let mut c = config(&d.path().join("protected"));
    c.feeds.push(FeedConfig {
        name: "Offline".into(),
        max_items: 10,
        url: "https://offline.invalid/feed".into(),
        ..Default::default()
    });
    let counts = Mutex::new(BTreeMap::<String, usize>::new());
    let network = |url: &str| {
        *counts.lock().unwrap().entry(url.into()).or_default() += 1;
        net(url)
    };
    let before = snapshot(&d.path().join("protected"));
    let a = build_fresh(
        &c,
        &context(&d.path().join("a"), now(), false),
        &network,
        &GatherOptions::default(),
    )
    .unwrap();
    assert_eq!(counts.lock().unwrap().get(FEED), Some(&1));
    let b = build_fresh(
        &c,
        &context(&d.path().join("b"), now(), false),
        &network,
        &GatherOptions::default(),
    )
    .unwrap();
    assert_eq!(counts.lock().unwrap().get(FEED), Some(&2));
    let puzzle = |p: &Path| {
        manifest(p)["sections"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["key"] == "puzzle")
            .unwrap()["puzzle"]
            .clone()
    };
    assert!(
        puzzle(&a.bundle).is_object(),
        "an absent daily puzzle must be generated locally"
    );
    assert_eq!(puzzle(&a.bundle), puzzle(&b.bundle));
    assert_ne!(
        manifest(&a.bundle)["edition"]["id"],
        manifest(&b.bundle)["edition"]["id"]
    );
    let v = manifest(&b.bundle);
    let sections = v["sections"].as_array().unwrap();
    assert!(
        sections
            .iter()
            .any(|s| s["title"] == "Wire" && !s["stories"].as_array().unwrap().is_empty())
    );
    assert!(
        sections
            .iter()
            .any(|s| s["title"] == "Offline" && s["unavailable"].is_string())
    );
    assert_eq!(before, snapshot(&d.path().join("protected")));
}

#[test]
fn saved_long_reads_are_included_only_when_selected_and_queue_is_not_consumed() {
    for books in [false, true] {
        let d = tempfile::tempdir().unwrap();
        let protected = d.path().join("protected");
        let c = config(&protected);
        let q = deckle_digest::reading::ReadingQueue::new(c.reading.queue_path.clone().unwrap());
        q.add_url(PAGE, None, "test", now()).unwrap();
        let before = snapshot(&protected);
        let ctx = context(&d.path().join("job"), now(), books);
        let result = build_fresh(&c, &ctx, &net, &GatherOptions::default()).unwrap();
        let v = manifest(&result.bundle);
        let files = v["files"].as_array().unwrap();
        let reads = files
            .iter()
            .filter(|f| {
                f["path"]
                    .as_str()
                    .unwrap()
                    .starts_with("epub/deckle-reads-")
            })
            .count();
        assert_eq!(reads, usize::from(books));
        let generated = snapshot(&ctx.work_dir)
            .keys()
            .filter(|p| {
                p.extension().is_some_and(|e| e == "epub")
                    && p.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("deckle-reads-")
            })
            .count();
        assert_eq!(generated, if books { 2 } else { 0 });
        let reading = v["sections"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["key"] == "reading")
            .unwrap();
        let story = &reading["stories"][0];
        let body = story["body"]["text"].as_str().unwrap();
        if books {
            assert!(
                story["long_read"]
                    .as_str()
                    .is_some_and(|p| result.bundle.join(p).is_file())
            );
        } else {
            assert!(
                body.contains("town council debated the public library"),
                "the full reading must stay in the paper when no book is delivered"
            );
            assert!(!body.contains("In your library"));
            assert!(story["long_read"].is_null());
        }

        assert!(
            v["sections"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s["key"] == "reading")
        );
        assert_eq!(before, snapshot(&protected));
    }
}

#[test]
fn daily_puzzle_is_reused_without_changing_solve_or_headlines() {
    let d = tempfile::tempdir().unwrap();
    let protected = d.path().join("protected");
    let c = config(&protected);
    let date = now()
        .with_timezone(&deckle_digest::tz::local_tz())
        .date_naive();
    let mut store = deckle_digest::puzzle::store::Store::default();
    deckle_digest::puzzle::today(
        date,
        &c.puzzle,
        deckle_digest::puzzle::words::Words::embedded(),
        &mut store,
    );
    let day = store.days.get_mut(&date).unwrap();
    day.puzzle.number = 747;
    day.solve = Some(deckle_digest::puzzle::store::Solve {
        solved: 3,
        of: 4,
        ladder: true,
    });
    store.save(&c.puzzle_store()).unwrap();
    let before = snapshot(&protected);
    let a = build_fresh(
        &c,
        &context(&d.path().join("job"), now(), false),
        &net,
        &GatherOptions::default(),
    )
    .unwrap();
    let v = manifest(&a.bundle);
    let puzzle = v["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["key"] == "puzzle")
        .unwrap();
    assert_eq!(puzzle["puzzle"]["number"], 747);
    assert_eq!(before, snapshot(&protected));
}

#[test]
fn start_time_controls_date_at_midnight_and_dst_boundaries() {
    let d = tempfile::tempdir().unwrap();
    let c = config(&d.path().join("protected"));
    for (n, stamp) in [
        "2026-10-28T03:59:59Z",
        "2026-10-28T04:00:00Z",
        "2026-11-01T05:59:59Z",
        "2026-11-01T06:00:00Z",
        "2026-03-08T06:59:59Z",
        "2026-03-08T07:00:00Z",
    ]
    .into_iter()
    .enumerate()
    {
        let ctx = context(&d.path().join(n.to_string()), at(stamp), false);
        let result = build_fresh(&c, &ctx, &net, &GatherOptions::default()).unwrap();
        let v = manifest(&result.bundle);
        assert_eq!(
            v["edition"]["date"],
            ctx.started_at
                .with_timezone(&deckle_digest::tz::local_tz())
                .date_naive()
                .to_string()
        );
        assert_eq!(
            v["edition"]["timezone"],
            deckle_digest::tz::local_tz().name()
        );
        assert_eq!(v["edition"]["built_at"], stamp);
    }
}

#[test]
fn extra_paper_does_not_import_main_paper_saved_reading() {
    let d = tempfile::tempdir().unwrap();
    let mut c = config(&d.path().join("protected"));
    let q = deckle_digest::reading::ReadingQueue::new(c.reading.queue_path.clone().unwrap());
    q.add_url(PAGE, None, "test", now()).unwrap();
    c.paper = Some(PaperRef {
        slug: "extra".into(),
        state_dir: d.path().join("protected/papers/extra"),
    });
    let before = snapshot(&d.path().join("protected"));
    let result = build_fresh(
        &c,
        &context(&d.path().join("job"), now(), true),
        &net,
        &GatherOptions::default(),
    )
    .unwrap();
    assert!(
        manifest(&result.bundle)["sections"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["key"] != "reading")
    );
    assert_eq!(before, snapshot(&d.path().join("protected")));
}

#[test]
fn storage_failure_is_safe_and_publishes_nothing() {
    let d = tempfile::tempdir().unwrap();
    let c = config(&d.path().join("protected"));
    let work = d.path().join("job");
    std::fs::write(&work, b"not a directory").unwrap();
    let err = build_fresh(
        &c,
        &context(&work, now(), false),
        &net,
        &GatherOptions::default(),
    )
    .unwrap_err();
    assert_eq!(err.code, PressErrorCode::StorageUnavailable);
    assert!(!err.to_string().contains(&work.display().to_string()));
}

#[cfg(unix)]
#[test]
fn fresh_writer_refuses_dangling_output_symlinks_without_replacing_them() {
    let d = tempfile::tempdir().unwrap();
    let mut c = config(&d.path().join("protected"));
    c.sections = vec![SectionKind::Feeds];
    c.export.clear();
    c.remarkable.enabled = false;
    let date = now()
        .with_timezone(&deckle_digest::tz::local_tz())
        .date_naive();
    let (built, gathered) = deckle_digest::build_gathered(
        &c,
        date,
        now(),
        &d.path().join("legacy"),
        &net,
        &GatherOptions::default(),
    )
    .unwrap();
    let ctx = context(d.path(), now(), false);
    let dest = d.path().join(format!("{}.deckle", ctx.identity.edition_id));
    std::os::unix::fs::symlink(d.path().join("missing"), &dest).unwrap();
    assert!(
        deckle_digest::bundle::write_fresh(
            &gathered,
            &built,
            d.path(),
            false,
            now(),
            &ctx.identity
        )
        .is_err()
    );
    assert!(
        std::fs::symlink_metadata(dest)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(
        !d.path()
            .join(format!(".{}.deckle.tmp", ctx.identity.edition_id))
            .exists(),
        "invalid destination must be rejected before staging"
    );
}

#[test]
fn saved_page_photos_are_snapshotted_with_the_html() {
    let d = tempfile::tempdir().unwrap();
    let protected = d.path().join("protected");
    let mut c = config(&protected);
    c.photos = true;
    c.reading.long_read_words = usize::MAX;
    put(
        d.path(),
        "saved.html",
        &String::from_utf8(page())
            .unwrap()
            .replace(
                "</head>",
                "<meta property=\"og:image\" content=\"saved_files/photo.jpg\"></head>",
            )
            .into_bytes(),
    );
    std::fs::create_dir(d.path().join("saved_files")).unwrap();
    image::RgbImage::from_pixel(400, 400, image::Rgb([210, 80, 40]))
        .save(d.path().join("saved_files/photo.jpg"))
        .unwrap();
    let q = deckle_digest::reading::ReadingQueue::new(c.reading.queue_path.clone().unwrap());
    q.add_page(&d.path().join("saved.html"), "test", now())
        .unwrap();
    let before = snapshot(&protected);
    let result = build_fresh(
        &c,
        &context(&d.path().join("job"), now(), false),
        &net,
        &GatherOptions::default(),
    )
    .unwrap();
    let v = manifest(&result.bundle);
    let reading = v["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["key"] == "reading")
        .unwrap();
    assert!(
        !reading["stories"][0]["photos"]
            .as_array()
            .unwrap()
            .is_empty(),
        "saved-page pictures must survive snapshotting"
    );
    assert_eq!(before, snapshot(&protected));
}

#[test]
fn fresh_bundles_keep_full_size_photo_masters() {
    const PHOTO: &str = "https://example.invalid/photo.jpg";
    let d = tempfile::tempdir().unwrap();
    let protected = d.path().join("protected");
    let mut c = config(&protected);
    c.sections = vec![SectionKind::Reading];
    c.photos = true;
    let q = deckle_digest::reading::ReadingQueue::new(c.reading.queue_path.clone().unwrap());
    q.add_url(PAGE, None, "test", now()).unwrap();
    let before = snapshot(&protected);
    let html = String::from_utf8(page()).unwrap().replace(
        "</head>",
        &format!("<meta property=\"og:image\" content=\"{PHOTO}\"></head>"),
    );
    let mut photo = std::io::Cursor::new(Vec::new());
    image::RgbImage::from_pixel(1800, 1200, image::Rgb([210, 80, 40]))
        .write_to(&mut photo, image::ImageFormat::Jpeg)
        .unwrap();
    let network = |url: &str| match url {
        PAGE => Ok(html.as_bytes().to_vec()),
        PHOTO => Ok(photo.get_ref().clone()),
        _ => net(url),
    };
    let result = build_fresh(
        &c,
        &context(&d.path().join("job"), now(), false),
        &network,
        &GatherOptions::default(),
    )
    .unwrap();
    let v = manifest(&result.bundle);
    let reading = v["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["key"] == "reading")
        .unwrap();
    let bundled_photo = &reading["stories"][0]["photos"][0];
    let image = image::open(result.bundle.join(bundled_photo["file"].as_str().unwrap())).unwrap();
    assert_eq!((image.width(), image.height()), (1800, 1200));
    assert_eq!(bundled_photo["width"], 1800);
    assert_eq!(bundled_photo["height"], 1200);
    assert_eq!(before, snapshot(&protected));
}

#[test]
fn fresh_inline_reading_keeps_the_tail_beyond_the_scheduled_word_cap() {
    const TAIL: &str =
        "The final archival marker confirms the entire account survived publication.";
    let d = tempfile::tempdir().unwrap();
    let protected = d.path().join("protected");
    let mut c = config(&protected);
    c.sections = vec![SectionKind::Reading];
    let q = deckle_digest::reading::ReadingQueue::new(c.reading.queue_path.clone().unwrap());
    let added = q.add_url(PAGE, None, "test", now()).unwrap();
    let id = match added {
        deckle_digest::reading::Added::New(id) | deckle_digest::reading::Added::Duplicate(id) => id,
    };
    c.reading.inline = vec![id];
    let before = snapshot(&protected);
    let html = format!(
        "<html><head><title>Complete saved account</title></head><body><article><h1>Complete saved account</h1>{}<p>{TAIL}</p></article></body></html>",
        "<p>The town council debated the public library and its overdue roof repairs before voting on the budget.</p>".repeat(500),
    );
    let network = |url: &str| {
        if url == PAGE {
            Ok(html.as_bytes().to_vec())
        } else {
            net(url)
        }
    };
    let date = now()
        .with_timezone(&deckle_digest::tz::local_tz())
        .date_naive();
    let scheduled = deckle_digest::gather(&c, date, now(), &network);
    assert!(scheduled.long_reads.is_empty());
    assert!(
        !scheduled.edition.sections[0].articles[0]
            .body
            .contains(TAIL),
        "scheduled inline reading retains its existing cap"
    );
    let result = build_fresh(
        &c,
        &context(&d.path().join("job"), now(), false),
        &network,
        &GatherOptions::default(),
    )
    .unwrap();
    let v = manifest(&result.bundle);
    let story = &v["sections"][0]["stories"][0];
    assert!(
        story["body"]["text"].as_str().unwrap().contains(TAIL),
        "fresh reading must retain the final paragraph beyond 6000 words"
    );
    assert!(story["words"].as_u64().unwrap() > 6000);
    assert!(story["long_read"].is_null());
    assert_eq!(before, snapshot(&protected));
}
