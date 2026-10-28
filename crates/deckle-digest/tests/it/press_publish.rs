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
