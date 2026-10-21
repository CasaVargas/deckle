// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle serve` end to end: a real listener on 127.0.0.1, ureq as the Kindle's browser.

use std::net::TcpListener;
use std::path::Path;

use deckle_core::deliver::Target;
use deckle_core::queue::Queue;
use deckle_core::serve;

const TOKEN: &str = "0123456789abcdef0123456789abcdef";

fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<_> = walk(dir)
        .into_iter()
        .map(|p| {
            let rel = p.strip_prefix(dir).unwrap().to_string_lossy().into_owned();
            let bytes = std::fs::read(&p).unwrap();
            (rel, bytes)
        })
        .collect();
    out.sort();
    out
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            files.extend(walk(&p));
        } else {
            files.push(p);
        }
    }
    files
}

#[test]
fn a_browser_lists_and_downloads_the_outbox_without_changing_it() {
    let tmp = tempfile::tempdir().unwrap();
    let q = Queue::new(tmp.path().join("outbox"));
    std::fs::create_dir_all(q.dir.join("deckle-daily-2026-10-20.sdr")).unwrap();
    // Big enough to need several writes.
    let book: Vec<u8> = (0..300_000u32).map(|i| (i * 31 % 251) as u8).collect();
    std::fs::write(q.dir.join("deckle-daily-2026-10-20.kfx"), &book).unwrap();
    std::fs::write(
        q.dir.join("deckle-daily-2026-10-20.sdr/XRAY.entities.asc"),
        b"SIDECAR",
    )
    .unwrap();
    let before = snapshot(&q.dir);

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let served = q.clone();
    std::thread::spawn(move || serve::serve(listener, served, TOKEN.to_owned()));

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();
    let base = serve::url(addr, TOKEN);
    assert_eq!(base, format!("http://{addr}/t/{TOKEN}/"));

    let mut index = agent.get(&base).call().unwrap();
    assert_eq!(index.status().as_u16(), 200);
    let html = index.body_mut().read_to_string().unwrap();
    assert!(html.contains("deckle-daily-2026-10-20.kfx"), "{html}");
    assert!(!html.contains(".sdr"), "{html}");

    let mut file = agent
        .get(format!("{base}deckle-daily-2026-10-20.kfx"))
        .call()
        .unwrap();
    assert_eq!(file.status().as_u16(), 200);
    assert_eq!(
        file.headers()
            .get("content-length")
            .unwrap()
            .to_str()
            .unwrap(),
        book.len().to_string()
    );
    assert_eq!(
        file.headers()
            .get("content-disposition")
            .unwrap()
            .to_str()
            .unwrap(),
        "attachment; filename=\"deckle-daily-2026-10-20.kfx\""
    );
    let got = file
        .body_mut()
        .with_config()
        .limit(10 << 20)
        .read_to_vec()
        .unwrap();
    assert_eq!(got, book);

    for (url, status) in [
        (format!("http://{addr}/"), 403),
        (format!("http://{addr}/deckle-daily-2026-10-20.kfx"), 403),
        (
            format!("http://{addr}/t/ffffffffffffffffffffffffffffffff/deckle-daily-2026-10-20.kfx"),
            403,
        ),
        (
            format!("{base}%2e%2e%2foutbox%2fdeckle-daily-2026-10-20.kfx"),
            404,
        ),
        (
            format!("{base}deckle-daily-2026-10-20.sdr%2fXRAY.entities.asc"),
            404,
        ),
        (format!("{base}nope.kfx"), 404),
    ] {
        let r = agent.get(&url).call().unwrap();
        assert_eq!(r.status().as_u16(), status, "{url}");
    }

    // Serving changed nothing, and the outbox still delivers.
    assert_eq!(snapshot(&q.dir), before);
    let kindle = tmp.path().join("kindle");
    std::fs::create_dir_all(&kindle).unwrap();
    let report = q.flush(&Target::Folder(kindle.clone()), None).unwrap();
    assert_eq!(report.delivered.len(), 1);
    assert_eq!(
        std::fs::read(kindle.join("deckle-daily-2026-10-20.kfx")).unwrap(),
        book
    );
}
