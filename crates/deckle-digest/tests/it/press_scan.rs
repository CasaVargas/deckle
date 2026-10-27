// SPDX-License-Identifier: GPL-3.0-or-later
use chrono::{Duration, Utc};
use deckle_digest::press::scan::{ScanCursor, ScanEvent, scan_requests};
use deckle_digest::press::{PressBinding, PressOperation, PressRequest, Uuid};
fn fixture() -> (tempfile::TempDir, PressBinding, PressRequest) {
    let dir = tempfile::tempdir().unwrap();
    let binding = PressBinding {
        root: dir.path().canonicalize().unwrap(),
        producer_id: Uuid::new_v4(),
        paper_id: Uuid::new_v4(),
    };
    let r = PressRequest {
        format: "deckle-press-request".into(),
        version: 1,
        request_id: Uuid::new_v4(),
        reader_id: Uuid::new_v4(),
        producer_id: binding.producer_id,
        paper_id: binding.paper_id,
        operation: PressOperation::FreshFull,
        created_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(24),
    };
    (
        dir,
        binding,
        PressRequest {
            expires_at: r.created_at + Duration::hours(24),
            ..r
        },
    )
}
fn put(b: &PressBinding, r: &PressRequest, bytes: &[u8]) -> std::path::PathBuf {
    let p = b.root.join(format!(
        ".deckle-press/requests/{}/{}.json",
        r.reader_id, r.request_id
    ));
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, bytes).unwrap();
    p
}
#[test]
fn press_scan_bounds_entries_and_eventually_reaches_new_request() {
    let (_d, b, r) = fixture();
    for _ in 0..350 {
        let mut old = r.clone();
        old.request_id = Uuid::new_v4();
        old.created_at -= Duration::days(2);
        old.expires_at -= Duration::days(2);
        put(&b, &old, &serde_json::to_vec(&old).unwrap());
    }
    put(&b, &r, &serde_json::to_vec(&r).unwrap());
    let mut c = ScanCursor::default();
    let mut found = false;
    for _ in 0..8 {
        let events = scan_requests(&b, &mut c, r.created_at);
        assert!(c.examined() <= 100);
        assert!(events.len() <= 100);
        found |= events.iter().any(
            |e| matches!(e,ScanEvent::Request{request,..} if request.request_id==r.request_id),
        );
    }
    assert!(found);
}
#[test]
fn press_scan_malformed_grace_tracks_exact_changed_bytes() {
    let (_d, b, r) = fixture();
    let p = put(&b, &r, b"{");
    let mut c = ScanCursor::default();
    assert!(scan_requests(&b, &mut c, r.created_at).is_empty());
    assert!(scan_requests(&b, &mut c, r.created_at + Duration::seconds(59)).is_empty());
    assert!(
        scan_requests(&b, &mut c, r.created_at + Duration::seconds(60))
            .iter()
            .any(|e| matches!(e, ScanEvent::Diagnostic { .. }))
    );
    assert!(
        scan_requests(&b, &mut c, r.created_at + Duration::seconds(61)).is_empty(),
        "unchanged diagnostic is emitted once"
    );
    std::fs::write(p, serde_json::to_vec(&r).unwrap()).unwrap();
    assert!(
        scan_requests(&b, &mut c, r.created_at + Duration::seconds(61))
            .iter()
            .any(|e| matches!(e, ScanEvent::Request { .. }))
    );
}
#[test]
#[cfg(unix)]
fn press_scan_rejects_file_and_ancestor_symlinks() {
    let (_d, b, r) = fixture();
    let p = put(&b, &r, &serde_json::to_vec(&r).unwrap());
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("request");
    std::fs::rename(&p, &target).unwrap();
    std::os::unix::fs::symlink(&target, &p).unwrap();
    let mut c = ScanCursor::default();
    assert!(
        !scan_requests(&b, &mut c, r.created_at)
            .iter()
            .any(|e| matches!(e, ScanEvent::Request { .. }))
    );
    std::fs::remove_file(p).unwrap();
    let parent = b.root.join(".deckle-press/requests");
    std::fs::rename(&parent, outside.path().join("requests")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("requests"), parent).unwrap();
    assert!(
        !scan_requests(&b, &mut c, r.created_at)
            .iter()
            .any(|e| matches!(e, ScanEvent::Request { .. }))
    );
}
#[test]
#[cfg(unix)]
fn press_scan_open_race_never_reads_symlink_target_or_blocks_on_fifo() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let (_d, b, r) = fixture();
    let p = put(&b, &r, b"inside");
    let outside = tempfile::tempdir().unwrap();
    let secret = outside.path().join("secret");
    std::fs::write(&secret, b"outside").unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let done = stop.clone();
    let path = p.clone();
    let thread = std::thread::spawn(move || {
        while !done.load(Ordering::Relaxed) {
            let _ = std::fs::remove_file(&path);
            let _ = std::os::unix::fs::symlink(&secret, &path);
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::write(&path, b"inside");
        }
    });
    for _ in 0..1000 {
        if let Ok(bytes) =
            deckle_digest::press::scan::read_control(&b.root, p.strip_prefix(&b.root).unwrap())
        {
            assert!(bytes.is_empty() || bytes == b"inside");
        }
    }
    stop.store(true, Ordering::Relaxed);
    thread.join().unwrap();
    let _ = std::fs::remove_file(&p);
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(p.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    assert!(
        deckle_digest::press::scan::read_control(&b.root, p.strip_prefix(&b.root).unwrap())
            .is_err()
    );
}
