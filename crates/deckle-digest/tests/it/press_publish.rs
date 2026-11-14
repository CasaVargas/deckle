// SPDX-License-Identifier: GPL-3.0-or-later
use chrono::{Duration, TimeZone, Utc};
use deckle_digest::press::*;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 27, 12, 0, 0).unwrap()
}
fn setup(root: &Path) -> (PressJournal, PressBinding) {
    let export = root.join("export");
    fs::create_dir(&export).unwrap();
    let b = PressBinding {
        root: fs::canonicalize(export).unwrap(),
        producer_id: Uuid::new_v4(),
        paper_id: Uuid::new_v4(),
    };
    setup::write_manifest(
        &b,
        &ProducerManifest {
            format: "deckle-press".into(),
            version: 1,
            producer_id: b.producer_id,
            paper_id: b.paper_id,
            paper_name: "Test".into(),
            enabled: true,
            operations: vec![PressOperation::FreshFull],
            reading_editions: true,
            updated_at: now(),
        },
    )
    .unwrap();
    let mut j = PressJournal::open(&root.join("journal")).unwrap();
    j.register_binding(b.clone(), true, false).unwrap();
    (j, b)
}
fn admit(j: &mut PressJournal, b: &PressBinding, at: chrono::DateTime<Utc>) -> (JobLease, Uuid) {
    let r = PressRequest {
        format: "deckle-press-request".into(),
        version: 1,
        request_id: Uuid::new_v4(),
        reader_id: Uuid::new_v4(),
        producer_id: b.producer_id,
        paper_id: b.paper_id,
        operation: PressOperation::FreshFull,
        articles: None,
        created_at: at,
        expires_at: at + Duration::hours(24),
    };
    let id = r.request_id;
    j.accept(r, "hash", at).unwrap();
    (j.admit(at).unwrap().unwrap(), id)
}
fn artifact(root: &Path, l: &JobLease) -> FreshArtifact {
    let bundle = root.join(format!("build-{}", l.edition_id));
    fs::create_dir(&bundle).unwrap();
    fs::write(bundle.join("cover.jpg"), b"image").unwrap();
    let mut v: serde_json::Value =
        serde_json::from_str(include_str!("../../../../fixtures/press/bundle-fresh.json")).unwrap();
    v["edition"]["id"] = l.edition_id.to_string().into();
    v["edition"]["paper_id"] = l.paper_id.to_string().into();
    v["files"] = serde_json::json!([{"path":"cover.jpg","size":5,"sha256":hash(b"image")}]);
    let bytes = serde_json::to_vec(&v).unwrap();
    fs::write(bundle.join("edition.json"), &bytes).unwrap();
    FreshArtifact {
        bundle,
        manifest_sha256: hash(&bytes),
    }
}
#[test]
fn press_publish_real_bundle_status_archive_and_restart_are_idempotent() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, b) = setup(d.path());
    let (l, id) = admit(&mut j, &b, now());
    let a = artifact(d.path(), &l);
    let s = publish_result(&mut j, &l, &a, now).unwrap();
    assert_eq!(s.state, PressState::Published);
    assert!(
        b.root
            .join(&s.result.as_ref().unwrap().bundle)
            .join("edition.json")
            .exists()
    );
    assert!(
        d.path()
            .join(format!(
                "journal/archive/{}/{}.deckle/edition.json",
                l.paper_id, l.edition_id
            ))
            .exists()
    );
    drop(j);
    let mut j = PressJournal::open(&d.path().join("journal")).unwrap();
    flush_statuses(&mut j).unwrap();
    let status: PressStatus = serde_json::from_slice(
        &fs::read(b.root.join(format!(".deckle-press/status/{id}.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(status, s);
    assert_eq!(
        fs::read_dir(b.root.join(".deckle-press/editions"))
            .unwrap()
            .filter(|e| !e
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with('.'))
            .count(),
        1
    );
}
#[test]
fn press_publish_checks_assets_clock_and_stale_generations() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, b) = setup(d.path());
    let (l, _) = admit(&mut j, &b, now());
    let a = artifact(d.path(), &l);
    fs::write(a.bundle.join("cover.jpg"), b"wrong").unwrap();
    assert!(publish_result(&mut j, &l, &a, now).is_err());
    assert!(
        !b.root
            .join(format!(".deckle-press/editions/{}.deckle", l.edition_id))
            .exists()
    );
    fs::write(a.bundle.join("cover.jpg"), b"image").unwrap();
    let newer = j.recover(&l, now()).unwrap();
    assert!(publish_result(&mut j, &l, &a, now).is_err());
    let mut calls = 0;
    let e = publish_result(&mut j, &newer, &a, || {
        calls += 1;
        if calls < 3 { now() } else { l.deadline }
    })
    .unwrap_err();
    assert_eq!(e.code, PressErrorCode::TimedOut);
    assert!(
        !b.root
            .join(format!(".deckle-press/editions/{}.deckle", l.edition_id))
            .exists()
    );
}
#[test]
fn press_publish_recovers_final_rename_before_journal_commit_even_after_deadline() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, b) = setup(d.path());
    let (l, id) = admit(&mut j, &b, now());
    let a = artifact(d.path(), &l);
    let mut calls = 0;
    let blocked = d.path().join("journal/journal.backup.json.tmp");
    let e = publish_result(&mut j, &l, &a, || {
        calls += 1;
        if calls == 3 {
            fs::create_dir(&blocked).unwrap();
        }
        now()
    })
    .unwrap_err();
    assert_eq!(e.code, PressErrorCode::StorageUnavailable);
    assert!(
        b.root
            .join(format!(".deckle-press/editions/{}.deckle", l.edition_id))
            .exists()
    );
    assert!(j.validate_lease(&l, now()).is_err());
    assert!(flush_statuses(&mut j).is_err());
    drop(j);
    fs::remove_dir(blocked).unwrap();
    let mut j = PressJournal::open(&d.path().join("journal")).unwrap();
    let s = recover_result(&mut j, &l, l.deadline + Duration::hours(1))
        .unwrap()
        .unwrap();
    assert_eq!(s.request_id, id);
    assert_eq!(s.state, PressState::Published);
}
#[test]
fn press_publish_status_failure_retries_without_source_or_archive() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, b) = setup(d.path());
    let (l, id) = admit(&mut j, &b, now());
    let a = artifact(d.path(), &l);
    let status = b.root.join(".deckle-press/status");
    fs::write(&status, b"blocked").unwrap();
    assert_eq!(
        publish_result(&mut j, &l, &a, now).unwrap_err().code,
        PressErrorCode::ProtocolConflict
    );
    assert_eq!(j.status(id).unwrap().state, PressState::Published);
    fs::remove_dir_all(a.bundle).unwrap();
    fs::remove_file(status).unwrap();
    flush_statuses(&mut j).unwrap();
    assert!(
        b.root
            .join(format!(".deckle-press/status/{id}.json"))
            .exists()
    );
}
#[test]
fn press_publish_retains_48_hours_then_tombstones_before_delete_and_keeps_seven() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, b) = setup(d.path());
    let mut outputs = Vec::new();
    fs::write(b.root.join("2026-10-27.deckle"), b"legacy").unwrap();
    fs::create_dir_all(b.root.join(".deckle-press/requests/foreign")).unwrap();
    fs::write(
        b.root.join(".deckle-press/requests/foreign/old.json"),
        b"request",
    )
    .unwrap();
    for i in 0..8 {
        let at = now() + Duration::minutes(5 * i);
        let (l, id) = admit(&mut j, &b, at);
        let a = artifact(d.path(), &l);
        publish_result(&mut j, &l, &a, || at).unwrap();
        outputs.push((l, id));
    }
    prune_results(&mut j, now() + Duration::hours(47)).unwrap();
    for (l, _) in &outputs {
        assert!(
            b.root
                .join(format!(".deckle-press/editions/{}.deckle", l.edition_id))
                .exists()
        );
    }
    let unrelated = b.root.join(".deckle-press/editions/unrelated.deckle");
    fs::create_dir(&unrelated).unwrap();
    fs::write(unrelated.join("keep"), b"unrelated").unwrap();
    let (first, id) = &outputs[0];
    let output = b.root.join(format!(
        ".deckle-press/editions/{}.deckle",
        first.edition_id
    ));
    let status = b.root.join(format!(".deckle-press/status/{id}.json"));
    fs::remove_file(&status).unwrap();
    fs::create_dir(&status).unwrap();
    assert!(prune_results(&mut j, now() + Duration::hours(49)).is_err());
    assert!(output.exists());
    assert_eq!(j.status(*id).unwrap().state, PressState::ResultRemoved);
    fs::remove_dir(&status).unwrap();
    prune_results(&mut j, now() + Duration::hours(49)).unwrap();
    assert!(!output.exists());
    let s: PressStatus = serde_json::from_slice(&fs::read(status).unwrap()).unwrap();
    assert_eq!(s.state, PressState::ResultRemoved);
    assert!(
        !d.path()
            .join(format!(
                "journal/archive/{}/{}.deckle",
                first.paper_id, first.edition_id
            ))
            .exists()
    );
    assert_eq!(fs::read(unrelated.join("keep")).unwrap(), b"unrelated");
    assert_eq!(
        fs::read(b.root.join("2026-10-27.deckle")).unwrap(),
        b"legacy"
    );
    assert_eq!(
        fs::read(b.root.join(".deckle-press/requests/foreign/old.json")).unwrap(),
        b"request"
    );
    prune_results(&mut j, now() + Duration::days(31)).unwrap();
    assert!(
        j.status(*id).is_some(),
        "tombstone age is counted from removal"
    );
    prune_results(&mut j, now() + Duration::days(33)).unwrap();
    assert!(j.status(*id).is_none());
    let replay = PressRequest {
        format: "deckle-press-request".into(),
        version: 1,
        request_id: *id,
        reader_id: Uuid::new_v4(),
        producer_id: b.producer_id,
        paper_id: b.paper_id,
        operation: PressOperation::FreshFull,
        articles: None,
        created_at: now(),
        expires_at: now() + Duration::hours(24),
    };
    assert_eq!(
        j.accept(replay, "old", now() + Duration::days(33))
            .unwrap_err()
            .code,
        PressErrorCode::Expired
    );
    assert_eq!(j.status(outputs[7].1).unwrap().state, PressState::Published);
}
#[test]
fn press_publish_missing_owner_manifest_blocks_retention_and_archive_failure_is_safe() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, b) = setup(d.path());
    let (l, id) = admit(&mut j, &b, now());
    let a = artifact(d.path(), &l);
    fs::write(d.path().join("journal/archive"), b"blocked").unwrap();
    assert!(publish_result(&mut j, &l, &a, now).is_err());
    assert_eq!(j.status(id).unwrap().state, PressState::Building);
    fs::remove_file(d.path().join("journal/archive")).unwrap();
    publish_result(&mut j, &l, &a, now).unwrap();
    fs::remove_file(b.root.join(".deckle-press/producer.json")).unwrap();
    assert!(prune_results(&mut j, now() + Duration::days(4)).is_err());
    assert_eq!(j.status(id).unwrap().state, PressState::Published);
}
#[test]
fn press_publish_archive_recovery_and_final_only_recovery_validate_intent() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, b) = setup(d.path());
    let (l, _) = admit(&mut j, &b, now());
    let a = artifact(d.path(), &l);
    assert!(recover_result(&mut j, &l, now()).unwrap().is_none());
    let mut calls = 0;
    assert!(
        publish_result(&mut j, &l, &a, || {
            calls += 1;
            if calls < 3 { now() } else { l.deadline }
        })
        .is_err()
    );
    let archived = archived_artifact(&j, &l).unwrap().unwrap();
    fs::remove_dir_all(a.bundle).unwrap();
    let new = j.recover(&l, now() + Duration::minutes(1)).unwrap();
    publish_result(&mut j, &new, &archived, now).unwrap();
    let final_dir = b
        .root
        .join(format!(".deckle-press/editions/{}.deckle", l.edition_id));
    assert!(final_dir.exists());
    prune_results(&mut j, now() + Duration::hours(1)).unwrap();
    assert_eq!(
        fs::read_dir(b.root.join(".deckle-press/editions"))
            .unwrap()
            .count(),
        1,
        "recorded staging is cleaned"
    );
}
#[test]
fn press_publish_corrupt_committed_result_cannot_reconcile_after_deadline() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, b) = setup(d.path());
    let (l, _) = admit(&mut j, &b, now());
    let a = artifact(d.path(), &l);
    let blocked = d.path().join("journal/journal.backup.json.tmp");
    let mut calls = 0;
    assert!(
        publish_result(&mut j, &l, &a, || {
            calls += 1;
            if calls == 3 {
                fs::create_dir(&blocked).unwrap();
            }
            now()
        })
        .is_err()
    );
    drop(j);
    fs::remove_dir(blocked).unwrap();
    let mut j = PressJournal::open(&d.path().join("journal")).unwrap();
    fs::write(
        b.root.join(format!(
            ".deckle-press/editions/{}.deckle/cover.jpg",
            l.edition_id
        )),
        b"wrong",
    )
    .unwrap();
    assert!(recover_result(&mut j, &l, l.deadline).is_err());
    assert!(j.active().is_some());
}
#[test]
fn press_publish_finalizer_rechecks_clock_and_terminal_intents_are_cleaned() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, b) = setup(d.path());
    let (l, id) = admit(&mut j, &b, now());
    let a = artifact(d.path(), &l);
    let mut calls = 0;
    let e = publish_result(&mut j, &l, &a, || {
        calls += 1;
        if calls < 4 { now() } else { l.deadline }
    })
    .unwrap_err();
    assert_eq!(e.code, PressErrorCode::TimedOut);
    assert!(archived_artifact(&j, &l).unwrap().is_some());
    prune_results(&mut j, now()).unwrap();
    assert!(
        archived_artifact(&j, &l).unwrap().is_some(),
        "nonterminal references protect archive"
    );
    j.fail(&l, PressErrorCode::TimedOut, l.deadline).unwrap();
    prune_results(&mut j, l.deadline).unwrap();
    assert!(archived_artifact(&j, &l).unwrap().is_none());
    assert_eq!(j.status(id).unwrap().state, PressState::Failed);
    assert_eq!(
        fs::read_dir(b.root.join(".deckle-press/editions"))
            .unwrap()
            .count(),
        0
    );
}
#[cfg(unix)]
#[test]
fn press_publish_refuses_symlink_assets_and_changed_producer() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, b) = setup(d.path());
    let (l, _) = admit(&mut j, &b, now());
    let a = artifact(d.path(), &l);
    let foreign = d.path().join("outside");
    fs::write(&foreign, b"image").unwrap();
    fs::remove_file(a.bundle.join("cover.jpg")).unwrap();
    std::os::unix::fs::symlink(&foreign, a.bundle.join("cover.jpg")).unwrap();
    assert!(publish_result(&mut j, &l, &a, now).is_err());
    fs::remove_file(a.bundle.join("cover.jpg")).unwrap();
    fs::write(a.bundle.join("cover.jpg"), b"image").unwrap();
    let path = b.root.join(".deckle-press/producer.json");
    let mut v: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    v["producer_id"] = Uuid::new_v4().to_string().into();
    let bytes = serde_json::to_vec(&v).unwrap();
    fs::write(&path, &bytes).unwrap();
    assert_eq!(
        publish_result(&mut j, &l, &a, now).unwrap_err().code,
        PressErrorCode::ProtocolConflict
    );
    assert_eq!(fs::read(path).unwrap(), bytes);
    assert_eq!(fs::read(foreign).unwrap(), b"image");
}
#[test]
fn press_publish_tombstones_live_thirty_days_after_delayed_physical_removal() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, b) = setup(d.path());
    let mut oldest = None;
    for i in 0..8 {
        let at = now() + Duration::minutes(5 * i);
        let (l, id) = admit(&mut j, &b, at);
        let a = artifact(d.path(), &l);
        publish_result(&mut j, &l, &a, || at).unwrap();
        if i == 0 {
            oldest = Some(id);
        }
    }
    let id = oldest.unwrap();
    let status = b.root.join(format!(".deckle-press/status/{id}.json"));
    fs::remove_file(&status).unwrap();
    fs::create_dir(&status).unwrap();
    assert!(prune_results(&mut j, now() + Duration::days(3)).is_err());
    fs::remove_dir(status).unwrap();
    prune_results(&mut j, now() + Duration::days(10)).unwrap();
    prune_results(&mut j, now() + Duration::days(34)).unwrap();
    assert!(j.status(id).is_some());
    prune_results(&mut j, now() + Duration::days(41)).unwrap();
    assert!(j.status(id).is_none());
}
#[test]
fn press_publish_returns_live_coalesced_status_when_first_queued_request_expired() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, b) = setup(d.path());
    let request = |created_at| PressRequest {
        format: "deckle-press-request".into(),
        version: 1,
        request_id: Uuid::new_v4(),
        reader_id: Uuid::new_v4(),
        producer_id: b.producer_id,
        paper_id: b.paper_id,
        operation: PressOperation::FreshFull,
        articles: None,
        created_at,
        expires_at: created_at + Duration::hours(24),
    };
    let first = request(now());
    let second = request(now() + Duration::minutes(5));
    let second_id = second.request_id;
    j.accept(first, "first", now()).unwrap();
    j.accept(second, "second", now() + Duration::minutes(5))
        .unwrap();
    let at = now() + Duration::hours(24);
    let lease = j.admit(at).unwrap().unwrap();
    let a = artifact(d.path(), &lease);
    let s = publish_result(&mut j, &lease, &a, || at).unwrap();
    assert_eq!(s.state, PressState::Published);
    assert_eq!(s.request_id, second_id);
}
#[test]
fn press_publish_unavailable_old_binding_does_not_block_other_statuses() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, b) = setup(d.path());
    let (_, old_id) = admit(&mut j, &b, now());
    let other = d.path().join("other");
    fs::create_dir(&other).unwrap();
    let other = PressBinding {
        root: other.canonicalize().unwrap(),
        paper_id: Uuid::new_v4(),
        ..b.clone()
    };
    setup::write_manifest(
        &other,
        &ProducerManifest {
            format: "deckle-press".into(),
            version: 1,
            producer_id: other.producer_id,
            paper_id: other.paper_id,
            paper_name: "Other".into(),
            enabled: true,
            operations: vec![PressOperation::FreshFull],
            reading_editions: true,
            updated_at: now(),
        },
    )
    .unwrap();
    j.register_binding(other.clone(), true, false).unwrap();
    let request = PressRequest {
        format: "deckle-press-request".into(),
        version: 1,
        request_id: Uuid::new_v4(),
        reader_id: Uuid::new_v4(),
        producer_id: other.producer_id,
        paper_id: other.paper_id,
        operation: PressOperation::FreshFull,
        articles: None,
        created_at: now(),
        expires_at: now() + Duration::hours(24),
    };
    let id = request.request_id;
    j.accept(request, "other", now()).unwrap();
    fs::rename(&b.root, d.path().join("offline")).unwrap();
    assert!(flush_statuses(&mut j).is_err());
    assert!(
        other
            .root
            .join(format!(".deckle-press/status/{id}.json"))
            .exists()
    );
    assert_eq!(j.binding_for_request(old_id).unwrap(), b);
}

fn request_at(
    b: &PressBinding,
    created_at: chrono::DateTime<Utc>,
    expires_at: chrono::DateTime<Utc>,
) -> PressRequest {
    PressRequest {
        format: "deckle-press-request".into(),
        version: 1,
        request_id: Uuid::new_v4(),
        reader_id: Uuid::new_v4(),
        producer_id: b.producer_id,
        paper_id: b.paper_id,
        operation: PressOperation::FreshFull,
        articles: None,
        created_at,
        expires_at,
    }
}
fn status_path(b: &PressBinding, id: Uuid) -> std::path::PathBuf {
    b.root.join(format!(".deckle-press/status/{id}.json"))
}
#[test]
fn press_retention_forgets_a_refusal_after_three_days_and_keeps_its_status_file() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, b) = setup(d.path());
    let refused = request_at(&b, now(), now() + Duration::hours(24));
    j.reject_in(&b, refused.clone(), "busy", PressErrorCode::Busy, now())
        .unwrap();
    flush_statuses(&mut j).unwrap();
    prune_results(&mut j, now() + Duration::hours(71)).unwrap();
    assert!(j.status(refused.request_id).is_some());
    prune_results(&mut j, now() + Duration::hours(73)).unwrap();
    assert!(j.status(refused.request_id).is_none());
    assert!(status_path(&b, refused.request_id).exists());
}
#[test]
fn press_retention_forgets_a_clock_mismatch_three_days_after_first_seen() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, b) = setup(d.path());
    let far = Utc.with_ymd_and_hms(9999, 1, 1, 0, 0, 0).unwrap();
    let refused = request_at(&b, now(), far);
    j.reject_in(
        &b,
        refused.clone(),
        "far",
        PressErrorCode::ClockMismatch,
        now(),
    )
    .unwrap();
    flush_statuses(&mut j).unwrap();
    prune_results(&mut j, now() + Duration::hours(71)).unwrap();
    assert!(j.status(refused.request_id).is_some());
    prune_results(&mut j, now() + Duration::hours(72)).unwrap();
    assert!(j.status(refused.request_id).is_none());
    assert!(status_path(&b, refused.request_id).exists());
}
#[test]
fn press_retention_keeps_a_refusal_from_a_clock_ahead_until_48_hours_past_its_expiry() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, b) = setup(d.path());
    let ahead = now() + Duration::hours(60);
    let refused = request_at(&b, ahead, ahead + Duration::hours(24));
    j.reject_in(
        &b,
        refused.clone(),
        "ahead",
        PressErrorCode::ClockMismatch,
        now(),
    )
    .unwrap();
    // 73 hours on, the request reads as valid; its refusal must still be the answer.
    let later = now() + Duration::hours(73);
    prune_results(&mut j, later).unwrap();
    let status = j.accept_in(&b, refused.clone(), "ahead", later).unwrap();
    assert_eq!(status.error_code, Some(PressErrorCode::ClockMismatch));
    prune_results(&mut j, ahead + Duration::hours(24 + 48)).unwrap();
    assert!(j.status(refused.request_id).is_none());
}
#[test]
fn press_queue_busy_refusal_is_not_queued_once_the_queue_drains() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, b) = setup(d.path());
    let (lease, _) = admit(&mut j, &b, now());
    let mut waiting = request_at(&b, now(), now() + Duration::hours(24));
    waiting.operation = PressOperation::ReadingEdition;
    waiting.articles = Some(vec!["https://example.com/a".into()]);
    j.accept_in(&b, waiting, "waiting", now()).unwrap();
    // A second, different selection while one is already waiting is refused as busy.
    let mut reading = request_at(&b, now(), now() + Duration::hours(24));
    reading.operation = PressOperation::ReadingEdition;
    reading.articles = Some(vec!["https://example.com/b".into()]);
    assert_eq!(
        j.accept_in(&b, reading.clone(), "reading", now())
            .unwrap_err()
            .code,
        PressErrorCode::Busy
    );
    j.reject_in(&b, reading.clone(), "reading", PressErrorCode::Busy, now())
        .unwrap();
    j.fail(&lease, PressErrorCode::BuildFailed, now()).unwrap();
    let pending = j.admit(now() + Duration::minutes(6)).unwrap().unwrap();
    j.fail(&pending, PressErrorCode::BuildFailed, now())
        .unwrap();
    let again = j
        .accept_in(&b, reading.clone(), "reading", now() + Duration::minutes(7))
        .unwrap();
    assert_eq!(again.state, PressState::Failed);
    assert_eq!(again.error_code, Some(PressErrorCode::Busy));
    assert!(j.admit(now() + Duration::minutes(20)).unwrap().is_none());
}
#[test]
fn press_retention_holds_at_most_three_days_of_refusals() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, b) = setup(d.path());
    // 600 refusals, 30 a day for 20 days, retention after each day. (2,000 take a minute
    // here, nearly all of it syncing the journal; the bound is the same.)
    for day in 0..20 {
        for i in 0..30 {
            let at = now() + Duration::days(day) + Duration::minutes(i * 45);
            let refused = request_at(&b, at, at + Duration::hours(24));
            j.reject_in(&b, refused, "busy", PressErrorCode::Busy, at)
                .unwrap();
        }
        prune_results(&mut j, now() + Duration::days(day + 1)).unwrap();
        assert!(j.statuses().len() <= 90, "{}", j.statuses().len());
    }
}
#[test]
fn press_status_flush_writes_only_changed_statuses_while_a_folder_is_offline() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, b) = setup(d.path());
    let other_root = d.path().join("other");
    fs::create_dir(&other_root).unwrap();
    let other = PressBinding {
        root: fs::canonicalize(&other_root).unwrap(),
        producer_id: b.producer_id,
        paper_id: Uuid::new_v4(),
    };
    setup::write_manifest(
        &other,
        &ProducerManifest {
            format: "deckle-press".into(),
            version: 1,
            producer_id: other.producer_id,
            paper_id: other.paper_id,
            paper_name: "Other".into(),
            enabled: true,
            operations: vec![PressOperation::FreshFull],
            reading_editions: true,
            updated_at: now(),
        },
    )
    .unwrap();
    j.register_binding(other.clone(), true, false).unwrap();
    let here = request_at(&b, now(), now() + Duration::hours(24));
    let away = request_at(&other, now(), now() + Duration::hours(24));
    j.reject_in(&b, here.clone(), "here", PressErrorCode::Busy, now())
        .unwrap();
    j.reject_in(&other, away.clone(), "away", PressErrorCode::Busy, now())
        .unwrap();
    let mut flushed = std::collections::HashMap::new();
    fs::rename(&other.root, d.path().join("offline")).unwrap();
    assert!(flush_changed(&mut j, &mut flushed, &Default::default()).is_err());
    assert!(status_path(&b, here.request_id).exists());
    // A file removed after a successful write is not rewritten by an incremental flush:
    // each of 100 ticks writes only the offline folder's unwritten status.
    fs::remove_file(status_path(&b, here.request_id)).unwrap();
    for _ in 0..100 {
        assert!(flush_changed(&mut j, &mut flushed, &Default::default()).is_err());
    }
    assert!(!status_path(&b, here.request_id).exists());
    fs::rename(d.path().join("offline"), &other.root).unwrap();
    flush_changed(&mut j, &mut flushed, &Default::default()).unwrap();
    assert!(status_path(&other, away.request_id).exists());
    assert!(!status_path(&b, here.request_id).exists());
    // A full flush (an empty record of what was written) writes it again.
    flushed.clear();
    flush_changed(&mut j, &mut flushed, &Default::default()).unwrap();
    assert!(status_path(&b, here.request_id).exists());
}
#[test]
fn press_late_request_gets_one_expired_status_and_no_journal_entry() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, b) = setup(d.path());
    let created = now() - Duration::days(4);
    let late = request_at(&b, created, created + Duration::hours(24));
    let status = j
        .reject_in(&b, late.clone(), "late", PressErrorCode::Expired, now())
        .unwrap();
    assert_eq!(status.state, PressState::Expired);
    assert!(j.status(late.request_id).is_none());
    assert!(!status_written(&b, late.request_id));
    write_unjournaled_status(&b, &status).unwrap();
    assert!(status_written(&b, late.request_id));
    let written: PressStatus =
        serde_json::from_slice(&fs::read(status_path(&b, late.request_id)).unwrap()).unwrap();
    assert_eq!(written, status);
    // Never replaces a status file that is already there.
    assert!(write_unjournaled_status(&b, &status).is_err());
    // Under 48 hours past expiry, an expired request is journaled as before.
    let recent = request_at(&b, now() - Duration::hours(71), now() - Duration::hours(47));
    j.reject_in(&b, recent.clone(), "recent", PressErrorCode::Expired, now())
        .unwrap();
    assert!(j.status(recent.request_id).is_some());
}

fn manifest(b: &PressBinding) -> ProducerManifest {
    ProducerManifest {
        format: "deckle-press".into(),
        version: 1,
        producer_id: b.producer_id,
        paper_id: b.paper_id,
        paper_name: "Test".into(),
        enabled: true,
        operations: vec![PressOperation::FreshFull],
        reading_editions: true,
        updated_at: now(),
    }
}
/// Publish `count` editions for `b`, six minutes apart, and return their bundle paths.
fn publish_editions(
    d: &Path,
    j: &mut PressJournal,
    b: &PressBinding,
    count: i64,
) -> Vec<(std::path::PathBuf, std::path::PathBuf, Uuid)> {
    (0..count)
        .map(|i| {
            let at = now() + Duration::minutes(6 * i);
            let (l, id) = admit(j, b, at);
            let a = artifact(d, &l);
            let s = publish_result(j, &l, &a, || at).unwrap();
            let archive = d.join(format!(
                "journal/archive/{}/{}.deckle",
                l.paper_id, l.edition_id
            ));
            (b.root.join(s.result.unwrap().bundle), archive, id)
        })
        .collect()
}
/// Stop configuring paper A in this folder and connect it to paper B instead.
fn reconnect(j: &mut PressJournal, a: &PressBinding) -> PressBinding {
    j.set_binding_policy(a, false, false).unwrap();
    let b = PressBinding {
        paper_id: Uuid::new_v4(),
        ..a.clone()
    };
    setup::disconnect_manifest(&a.root, &manifest(a)).unwrap();
    setup::write_manifest(&b, &manifest(&b)).unwrap();
    b
}
#[test]
fn press_replace_binding_removes_the_old_papers_outputs_and_statuses() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, a) = setup(d.path());
    // Ten editions in the last hour: newer than 48 hours and more than seven.
    let published = publish_editions(d.path(), &mut j, &a, 10);
    let b = reconnect(&mut j, &a);
    j.register_binding(b.clone(), true, false).unwrap();
    for (bundle, archive, id) in &published {
        assert!(!bundle.exists(), "{}", bundle.display());
        assert!(!archive.exists(), "{}", archive.display());
        assert!(!status_path(&a, *id).exists());
        assert!(j.status(*id).is_none());
    }
    assert_eq!(j.bindings(), vec![b.clone()]);
    drop(j);
    let mut j = PressJournal::open(&d.path().join("journal")).unwrap();
    let later = now() + Duration::hours(2);
    let request = request_at(&b, later, later + Duration::hours(24));
    j.accept_in(&b, request.clone(), "b", later).unwrap();
    let lease = j.admit(later).unwrap().unwrap();
    assert_eq!(lease.paper_id, b.paper_id);
}
#[test]
fn press_replace_binding_waits_for_the_old_papers_pending_requests() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, a) = setup(d.path());
    let pending = request_at(&a, now(), now() + Duration::hours(24));
    j.accept_in(&a, pending.clone(), "pending", now()).unwrap();
    let b = reconnect(&mut j, &a);
    assert_eq!(
        j.register_binding(b.clone(), true, false).unwrap_err().code,
        PressErrorCode::ProtocolConflict
    );
    assert_eq!(j.bindings(), vec![a.clone()]);
    assert!(j.status(pending.request_id).is_some());
}
#[test]
fn press_replace_binding_needs_the_folder_to_name_the_new_paper() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, a) = setup(d.path());
    let published = publish_editions(d.path(), &mut j, &a, 1);
    j.set_binding_policy(&a, false, false).unwrap();
    let b = PressBinding {
        paper_id: Uuid::new_v4(),
        ..a.clone()
    };
    // The folder still names paper A.
    assert_eq!(
        j.register_binding(b, true, false).unwrap_err().code,
        PressErrorCode::ProtocolConflict
    );
    assert!(published[0].0.exists());
}
#[test]
fn press_replace_binding_refuses_an_unverifiable_bundle_and_names_it() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, a) = setup(d.path());
    let published = publish_editions(d.path(), &mut j, &a, 2);
    let b = reconnect(&mut j, &a);
    let extra = published[1].0.join(".DS_Store");
    fs::write(&extra, b"finder").unwrap();
    let error = j.register_binding(b.clone(), true, false).unwrap_err();
    assert_eq!(error.code, PressErrorCode::ProtocolConflict);
    assert!(
        error
            .to_string()
            .contains(&published[1].0.display().to_string()),
        "{error}"
    );
    assert_eq!(fs::read(&extra).unwrap(), b"finder");
    // A crash here leaves a journal that opens, still bound to A.
    drop(j);
    let mut j = PressJournal::open(&d.path().join("journal")).unwrap();
    assert_eq!(j.bindings(), vec![a.clone()]);
    // Once the owner removes the stray file, the next attempt finishes the move.
    fs::remove_file(&extra).unwrap();
    j.register_binding(b.clone(), true, false).unwrap();
    assert!(!published[1].0.exists());
    assert_eq!(j.bindings(), vec![b]);
}
#[test]
fn press_replace_binding_removes_unpublished_intents_and_stages() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, a) = setup(d.path());
    let (l, _) = admit(&mut j, &a, now());
    let built = artifact(d.path(), &l);
    // The deadline passes just before the final rename: an archived copy, a recorded
    // intent and a shared staging folder are left behind.
    let mut calls = 0;
    assert!(
        publish_result(&mut j, &l, &built, || {
            calls += 1;
            if calls == 3 { l.deadline } else { now() }
        })
        .is_err()
    );
    j.fail(&l, PressErrorCode::TimedOut, now()).unwrap();
    let archived = d.path().join(format!(
        "journal/archive/{}/{}.deckle",
        l.paper_id, l.edition_id
    ));
    assert!(archived.exists());
    let editions = a.root.join(".deckle-press/editions");
    assert_eq!(
        fs::read_dir(&editions).unwrap().count(),
        1,
        "one staging folder"
    );
    let b = reconnect(&mut j, &a);
    j.register_binding(b.clone(), true, false).unwrap();
    assert!(!archived.exists());
    assert_eq!(fs::read_dir(&editions).unwrap().count(), 0);
    drop(j);
    let j = PressJournal::open(&d.path().join("journal")).unwrap();
    assert_eq!(j.bindings(), vec![b]);
}
#[test]
fn press_status_files_never_carry_an_error_detail() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, a) = setup(d.path());
    let published = publish_editions(d.path(), &mut j, &a, 1);
    let refused = request_at(&a, now(), now() + Duration::hours(24));
    j.reject_in(&a, refused, "busy", PressErrorCode::Busy, now())
        .unwrap();
    let created = now() - Duration::days(4);
    let late = request_at(&a, created, created + Duration::hours(24));
    let status = j
        .reject_in(&a, late, "late", PressErrorCode::Expired, now())
        .unwrap();
    write_unjournaled_status(&a, &status).unwrap();
    let mut flushed = std::collections::HashMap::new();
    flush_changed(&mut j, &mut flushed, &Default::default()).unwrap();
    flush_statuses(&mut j).unwrap();
    // An error with a local explanation naming a path in this folder.
    let b = reconnect(&mut j, &a);
    fs::write(published[0].0.join(".DS_Store"), b"finder").unwrap();
    let error = j.register_binding(b, true, false).unwrap_err();
    let detail = error.detail.clone().expect("a local explanation");
    assert_eq!(error.to_string(), detail);
    let local = a.root.to_string_lossy().into_owned();
    assert!(detail.contains(&local));
    let statuses: Vec<_> = fs::read_dir(a.root.join(".deckle-press/status"))
        .unwrap()
        .map(|e| fs::read_to_string(e.unwrap().path()).unwrap())
        .collect();
    assert_eq!(statuses.len(), 3);
    for text in statuses {
        assert!(!text.contains(&detail), "{text}");
        assert!(!text.contains(&local), "{text}");
    }
}
fn sequence(d: &Path) -> u64 {
    let snapshot: serde_json::Value =
        serde_json::from_slice(&fs::read(d.join("journal/journal.json")).unwrap()).unwrap();
    snapshot["sequence"].as_u64().unwrap()
}
#[test]
fn press_publish_succeeds_while_another_folders_status_cannot_be_written() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, b) = setup(d.path());
    let other_root = d.path().join("other");
    fs::create_dir(&other_root).unwrap();
    let other = PressBinding {
        root: fs::canonicalize(&other_root).unwrap(),
        producer_id: b.producer_id,
        paper_id: Uuid::new_v4(),
    };
    setup::write_manifest(&other, &manifest(&other)).unwrap();
    j.register_binding(other.clone(), true, false).unwrap();
    let waiting = request_at(&other, now(), now() + Duration::hours(24));
    j.reject_in(&other, waiting, "other", PressErrorCode::Busy, now())
        .unwrap();
    fs::rename(&other.root, d.path().join("offline")).unwrap();
    let (l, id) = admit(&mut j, &b, now());
    let a = artifact(d.path(), &l);
    let s = publish_result(&mut j, &l, &a, now).unwrap();
    assert_eq!(s.state, PressState::Published);
    assert!(status_path(&b, id).exists());
}
#[test]
fn press_retention_with_nothing_to_forget_does_not_rewrite_the_journal() {
    let d = tempfile::tempdir().unwrap();
    let (mut j, b) = setup(d.path());
    let refused = request_at(&b, now(), now() + Duration::hours(24));
    j.reject_in(&b, refused, "busy", PressErrorCode::Busy, now())
        .unwrap();
    let before = sequence(d.path());
    for minutes in 0..10 {
        prune_results(&mut j, now() + Duration::minutes(minutes)).unwrap();
    }
    assert_eq!(sequence(d.path()), before);
    prune_results(&mut j, now() + Duration::hours(73)).unwrap();
    assert_eq!(sequence(d.path()), before + 1);
}
