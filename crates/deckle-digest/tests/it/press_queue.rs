// SPDX-License-Identifier: GPL-3.0-or-later
use chrono::{Duration, TimeZone, Utc};
use deckle_digest::press::*;

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 27, 12, 0, 0).unwrap()
}
fn binding(root: &std::path::Path) -> PressBinding {
    PressBinding {
        root: std::fs::canonicalize(root).unwrap(),
        producer_id: Uuid::new_v4(),
        paper_id: Uuid::new_v4(),
    }
}
fn request(b: &PressBinding, at: chrono::DateTime<Utc>) -> PressRequest {
    PressRequest {
        format: "deckle-press-request".into(),
        version: 1,
        request_id: Uuid::new_v4(),
        reader_id: Uuid::new_v4(),
        producer_id: b.producer_id,
        paper_id: b.paper_id,
        operation: PressOperation::FreshFull,
        created_at: at,
        expires_at: at + Duration::hours(24),
    }
}
fn result(lease: &JobLease) -> PressResult {
    PressResult {
        edition_id: lease.edition_id,
        bundle: format!(".deckle-press/editions/{}.deckle", lease.edition_id),
        manifest_sha256: "a".repeat(64),
    }
}
#[test]
fn press_queue_coalesces_requests_before_and_during_build_then_queues_new_result() {
    let dir = tempfile::tempdir().unwrap();
    let b = binding(dir.path());
    let mut j = PressJournal::open(&dir.path().join("journal")).unwrap();
    j.register_binding(b.clone(), true, false).unwrap();
    let r = request(&b, now());
    let a = j.accept(r.clone(), "hash-a", now()).unwrap();
    let second = j.accept(request(&b, now()), "hash-b", now()).unwrap();
    assert_eq!(a.job_id, second.job_id);
    assert_eq!(j.accept(r.clone(), "hash-a", now()).unwrap(), a);
    assert_eq!(
        j.accept(r, "changed", now()).unwrap_err().code,
        PressErrorCode::ProtocolConflict
    );
    let lease = j.admit(now()).unwrap().unwrap();
    assert_eq!(lease.started_at, now());
    assert_eq!(lease.deadline, now() + Duration::minutes(15));
    let during = j.accept(request(&b, now()), "hash-c", now()).unwrap();
    assert_eq!(during.job_id, a.job_id);
    assert_eq!(during.started_at, Some(now()));
    assert_eq!(during.state, PressState::Building);
    j.publish(&lease, result(&lease), now(), || Ok(())).unwrap();
    let next = j.accept(request(&b, now()), "hash-d", now()).unwrap();
    assert_ne!(next.job_id, a.job_id);
    assert!(j.admit(now() + Duration::minutes(4)).unwrap().is_none());
    assert!(j.admit(now() + Duration::minutes(5)).unwrap().is_some());
}

#[test]
fn press_queue_recovery_preserves_absolute_deadline_identity_and_fences_attempts() {
    let dir = tempfile::tempdir().unwrap();
    let b = binding(dir.path());
    let root = dir.path().join("journal");
    let mut j = PressJournal::open(&root).unwrap();
    j.register_binding(b.clone(), true, false).unwrap();
    let req = request(&b, now());
    let id = req.request_id;
    j.accept(req, "a", now()).unwrap();
    let first = j.admit(now()).unwrap().unwrap();
    drop(j);
    let mut j = PressJournal::open(&root).unwrap();
    assert_eq!(j.active(), Some(first.clone()));
    let second = j.recover(&first, now() + Duration::minutes(1)).unwrap();
    assert_eq!(second.edition_id, first.edition_id);
    assert_eq!(second.job_id, first.job_id);
    assert_eq!(second.deadline, first.started_at + Duration::minutes(15));
    assert_eq!((second.attempt, second.generation), (2, 2));
    let mut invoked = false;
    assert!(
        j.publish(&first, result(&first), now(), || {
            invoked = true;
            Ok(())
        })
        .is_err()
    );
    assert!(!invoked);
    let third = j.recover(&second, now() + Duration::minutes(2)).unwrap();
    assert_eq!((third.attempt, third.generation), (3, 3));
    assert_eq!(
        j.recover(&third, now() + Duration::minutes(3))
            .unwrap_err()
            .code,
        PressErrorCode::BuildFailed
    );
    assert!(j.active().is_none());
    assert_eq!(j.status(id).unwrap().state, PressState::Failed);
    assert!(j.validate_lease(&third, now()).is_err());
}

#[test]
fn press_queue_timeout_never_extends_on_restart_and_stale_worker_cannot_publish() {
    let dir = tempfile::tempdir().unwrap();
    let b = binding(dir.path());
    let root = dir.path().join("j");
    let mut j = PressJournal::open(&root).unwrap();
    j.register_binding(b.clone(), true, false).unwrap();
    let req = request(&b, now());
    let id = req.request_id;
    j.accept(req, "a", now()).unwrap();
    let lease = j.admit(now()).unwrap().unwrap();
    drop(j);
    let mut j = PressJournal::open(&root).unwrap();
    assert_eq!(
        j.recover(&lease, lease.deadline).unwrap_err().code,
        PressErrorCode::TimedOut
    );
    assert_eq!(
        j.status(id).unwrap().error_code,
        Some(PressErrorCode::TimedOut)
    );
    assert!(
        j.publish(&lease, result(&lease), lease.deadline, || panic!(
            "fenced worker called"
        ))
        .is_err()
    );
}

#[test]
fn press_queue_one_active_globally_and_one_hundred_pending_per_paper() {
    let dir = tempfile::tempdir().unwrap();
    let b = binding(dir.path());
    let other_dir = tempfile::tempdir().unwrap();
    let other = binding(other_dir.path());
    let mut j = PressJournal::open(&dir.path().join("j")).unwrap();
    j.register_binding(b.clone(), true, false).unwrap();
    j.register_binding(other.clone(), true, false).unwrap();
    for i in 0..100 {
        j.accept(request(&b, now()), &format!("hash-{i}"), now())
            .unwrap();
    }
    assert_eq!(
        j.accept(request(&b, now()), "excess", now())
            .unwrap_err()
            .code,
        PressErrorCode::Busy
    );
    j.accept(
        request(&other, now()),
        "other",
        now() + Duration::seconds(1),
    )
    .unwrap();
    let first = j.admit(now()).unwrap().unwrap();
    assert_eq!(first.paper_id, b.paper_id);
    assert!(j.admit(now() + Duration::minutes(6)).unwrap().is_none());
    j.fail(
        &first,
        PressErrorCode::BuildFailed,
        now() + Duration::minutes(1),
    )
    .unwrap();
    let next = j.admit(now() + Duration::minutes(1)).unwrap().unwrap();
    assert_eq!(next.paper_id, other.paper_id);
}

#[test]
fn press_queue_expiry_and_policy_are_rechecked_after_waiting_for_build_lock() {
    let dir = tempfile::tempdir().unwrap();
    let b = binding(dir.path());
    let mut j = PressJournal::open(&dir.path().join("j")).unwrap();
    j.register_binding(b.clone(), true, false).unwrap();
    let r = request(&b, now());
    let id = r.request_id;
    j.accept(r, "a", now()).unwrap();
    assert!(j.admit(now() + Duration::hours(24)).unwrap().is_none());
    assert_eq!(j.status(id).unwrap().state, PressState::Expired);
    let r = request(&b, now() + Duration::hours(25));
    let id = r.request_id;
    j.accept(r, "b", now() + Duration::hours(25)).unwrap();
    j.register_binding(b.clone(), true, true).unwrap();
    assert!(j.admit(now() + Duration::hours(25)).unwrap().is_none());
    assert_eq!(
        j.status(id).unwrap().error_code,
        Some(PressErrorCode::PressesStopped)
    );
    j.register_binding(b.clone(), true, false).unwrap();
    j.accept(
        request(&b, now() + Duration::hours(26)),
        "c",
        now() + Duration::hours(26),
    )
    .unwrap();
    let lease = j.admit(now() + Duration::hours(26)).unwrap().unwrap();
    j.register_binding(b.clone(), false, false).unwrap();
    j.publish(&lease, result(&lease), lease.started_at, || Ok(()))
        .unwrap();
    assert_eq!(
        j.accept(request(&b, now()), "d", now()).unwrap_err().code,
        PressErrorCode::Disabled
    );
}

#[test]
fn press_queue_future_clock_limit_and_replay_after_records_disappear() {
    let dir = tempfile::tempdir().unwrap();
    let b = binding(dir.path());
    let mut j = PressJournal::open(&dir.path().join("j")).unwrap();
    j.register_binding(b.clone(), true, false).unwrap();
    j.accept(request(&b, now() + Duration::minutes(5)), "allowed", now())
        .unwrap();
    assert_eq!(
        j.accept(
            request(&b, now() + Duration::minutes(5) + Duration::seconds(1)),
            "future",
            now()
        )
        .unwrap_err()
        .code,
        PressErrorCode::ClockMismatch
    );
    let old = request(&b, now());
    j.accept(old.clone(), "old", now()).unwrap();
    let lease = j.admit(now()).unwrap().unwrap();
    j.publish(&lease, result(&lease), now(), || Ok(())).unwrap();
    let id = old.request_id;
    assert_eq!(
        j.accept(old.clone(), "old", now() + Duration::days(31))
            .unwrap()
            .state,
        PressState::Published
    );
    // Simulate terminal record retention having elapsed without implementing pruning here.
    let mut empty = PressJournal::open(&dir.path().join("after-retention")).unwrap();
    empty.register_binding(b, true, false).unwrap();
    assert_eq!(
        empty
            .accept(old, "old", now() + Duration::days(31))
            .unwrap_err()
            .code,
        PressErrorCode::Expired
    );
    assert!(empty.status(id).is_none());
}

#[test]
fn press_queue_durable_snapshots_recover_corruption_and_fail_closed_without_valid_copy() {
    let dir = tempfile::tempdir().unwrap();
    let b = binding(dir.path());
    let root = dir.path().join("j");
    let mut j = PressJournal::open(&root).unwrap();
    j.register_binding(b.clone(), true, false).unwrap();
    let r = request(&b, now());
    let id = r.request_id;
    let expected = j.accept(r, "a", now()).unwrap();
    assert_eq!(
        PressJournal::open(&root).err().unwrap().code,
        PressErrorCode::Busy
    );
    drop(j);
    std::fs::write(root.join("journal.json"), b"{torn").unwrap();
    let j = PressJournal::open(&root).unwrap();
    assert_eq!(j.status(id).unwrap(), expected);
    drop(j);
    std::fs::write(root.join("journal.backup.json"), b"bad").unwrap();
    assert_eq!(
        PressJournal::open(&root).err().unwrap().code,
        PressErrorCode::StorageUnavailable
    );
}

#[test]
fn press_queue_failed_finalization_preserves_building_and_status_revisions_are_replayable() {
    let dir = tempfile::tempdir().unwrap();
    let b = binding(dir.path());
    let root = dir.path().join("j");
    let mut j = PressJournal::open(&root).unwrap();
    j.register_binding(b.clone(), true, false).unwrap();
    let r = request(&b, now());
    let id = r.request_id;
    j.accept(r.clone(), "a", now()).unwrap();
    let lease = j.admit(now()).unwrap().unwrap();
    assert!(
        j.publish(&lease, result(&lease), now(), || Err(PressError::new(
            PressErrorCode::StorageUnavailable
        )))
        .is_err()
    );
    assert_eq!(j.status(id).unwrap().state, PressState::Building);
    j.publish(&lease, result(&lease), now(), || Ok(())).unwrap();
    let expected = j.status(id).unwrap();
    assert_eq!(expected.revision, 3);
    drop(j);
    let j = PressJournal::open(&root).unwrap();
    assert_eq!(j.statuses(), vec![expected]);
}

#[test]
fn press_queue_committed_output_recovery_wins_over_elapsed_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let b = binding(dir.path());
    let root = dir.path().join("j");
    let mut j = PressJournal::open(&root).unwrap();
    j.register_binding(b.clone(), true, false).unwrap();
    let r = request(&b, now());
    let id = r.request_id;
    j.accept(r, "a", now()).unwrap();
    let lease = j.admit(now()).unwrap().unwrap();
    drop(j);
    let mut j = PressJournal::open(&root).unwrap();
    assert!(
        j.publish(&lease, result(&lease), lease.deadline, || panic!(
            "expired publication ran"
        ))
        .is_err()
    );
    let mut verified = false;
    j.reconcile_published(
        &lease,
        result(&lease),
        lease.deadline + Duration::minutes(1),
        || {
            verified = true;
            Ok(())
        },
    )
    .unwrap();
    assert!(verified);
    assert_eq!(j.status(id).unwrap().state, PressState::Published);
    assert_eq!(j.binding_for_request(id).unwrap(), b);
    assert!(j.recover(&lease, lease.deadline).is_err());
}

#[test]
fn press_queue_expired_pending_ids_do_not_consume_the_pending_limit_or_old_job() {
    let dir = tempfile::tempdir().unwrap();
    let b = binding(dir.path());
    let mut j = PressJournal::open(&dir.path().join("j")).unwrap();
    j.register_binding(b.clone(), true, false).unwrap();
    let mut old_job = None;
    for i in 0..100 {
        old_job = j
            .accept(request(&b, now()), &format!("{i}"), now())
            .unwrap()
            .job_id;
    }
    let at = now() + Duration::hours(24);
    let status = j.accept(request(&b, at), "new", at).unwrap();
    assert_ne!(status.job_id, old_job);
    assert_eq!(
        j.statuses()
            .iter()
            .filter(|s| s.state == PressState::Expired)
            .count(),
        100
    );
}

#[test]
fn press_queue_storage_failure_poisoning_rejects_further_mutation_until_reopened() {
    let dir = tempfile::tempdir().unwrap();
    let b = binding(dir.path());
    let root = dir.path().join("j");
    let mut j = PressJournal::open(&root).unwrap();
    j.register_binding(b.clone(), true, false).unwrap();
    // A directory at our temporary filename forces a real local commit failure.
    std::fs::create_dir(root.join("journal.backup.json.tmp")).unwrap();
    assert_eq!(
        j.accept(request(&b, now()), "a", now()).unwrap_err().code,
        PressErrorCode::StorageUnavailable
    );
    std::fs::remove_dir(root.join("journal.backup.json.tmp")).unwrap();
    assert_eq!(
        j.accept(request(&b, now()), "b", now()).unwrap_err().code,
        PressErrorCode::StorageUnavailable
    );
    drop(j);
    let mut j = PressJournal::open(&root).unwrap();
    assert!(j.statuses().is_empty());
    assert!(j.accept(request(&b, now()), "c", now()).is_ok());
}

#[test]
fn press_queue_semantically_corrupt_checksums_fail_closed() {
    use sha2::{Digest, Sha256};
    let dir = tempfile::tempdir().unwrap();
    let b = binding(dir.path());
    let root = dir.path().join("j");
    let mut j = PressJournal::open(&root).unwrap();
    j.register_binding(b.clone(), true, false).unwrap();
    j.accept(request(&b, now()), "a", now()).unwrap();
    drop(j);
    for name in ["journal.json", "journal.backup.json"] {
        let path = root.join(name);
        let mut outer: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let mut payload: serde_json::Value =
            serde_json::from_str(outer["payload"].as_str().unwrap()).unwrap();
        payload["entries"][0]["status"]["job_id"] = serde_json::json!(Uuid::new_v4());
        let body = serde_json::to_string(&payload).unwrap();
        outer["sha256"] = serde_json::json!(
            Sha256::digest(body.as_bytes())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        outer["payload"] = serde_json::json!(body);
        std::fs::write(path, serde_json::to_vec(&outer).unwrap()).unwrap();
    }
    assert_eq!(
        PressJournal::open(&root).err().unwrap().code,
        PressErrorCode::StorageUnavailable
    );
}

#[test]
fn press_queue_recovery_selects_newer_backup_after_crash_between_snapshot_renames() {
    let dir = tempfile::tempdir().unwrap();
    let b = binding(dir.path());
    let root = dir.path().join("j");
    let mut j = PressJournal::open(&root).unwrap();
    j.register_binding(b.clone(), true, false).unwrap();
    let r = request(&b, now());
    let id = r.request_id;
    j.accept(r, "a", now()).unwrap();
    let older = std::fs::read(root.join("journal.json")).unwrap();
    let lease = j.admit(now()).unwrap().unwrap();
    drop(j);
    // Backup already contains the building lease; the primary rename did not finish.
    std::fs::write(root.join("journal.json"), older).unwrap();
    let j = PressJournal::open(&root).unwrap();
    assert_eq!(j.active(), Some(lease));
    assert_eq!(j.status(id).unwrap().revision, 2);
}

#[test]
fn press_queue_durable_rejection_never_becomes_a_different_equal_revision_status() {
    let dir = tempfile::tempdir().unwrap();
    let b = binding(dir.path());
    let root = dir.path().join("j");
    let mut j = PressJournal::open(&root).unwrap();
    j.register_binding(b.clone(), true, false).unwrap();
    let r = request(&b, now());
    let id = r.request_id;
    let refused = j
        .reject(r.clone(), "same", PressErrorCode::Busy, now())
        .unwrap();
    assert_eq!(refused.state, PressState::Failed);
    assert_eq!(refused.error_code, Some(PressErrorCode::Busy));
    assert_eq!(refused.revision, 1);
    assert!(refused.job_id.is_none());
    assert_eq!(j.accept(r.clone(), "same", now()).unwrap(), refused);
    assert_eq!(
        j.reject(r.clone(), "changed", PressErrorCode::Disabled, now())
            .unwrap_err()
            .code,
        PressErrorCode::ProtocolConflict
    );
    assert_eq!(j.binding_for_request(id).unwrap(), b);
    drop(j);
    let mut j = PressJournal::open(&root).unwrap();
    assert_eq!(j.status(id).unwrap(), refused);
    assert_eq!(j.accept(r, "same", now()).unwrap(), refused);
    assert!(j.admit(now()).unwrap().is_none());
    let accepted = request(&b, now());
    let accepted_id = accepted.request_id;
    j.accept(accepted.clone(), "accepted", now()).unwrap();
    assert_eq!(
        j.reject(accepted, "accepted", PressErrorCode::Busy, now())
            .unwrap()
            .state,
        PressState::Queued
    );
    assert_eq!(j.status(accepted_id).unwrap().revision, 1);
}

#[test]
fn press_queue_reject_only_accepts_bound_valid_controls_or_explicit_temporal_rejections() {
    let dir = tempfile::tempdir().unwrap();
    let b = binding(dir.path());
    let mut j = PressJournal::open(&dir.path().join("j")).unwrap();
    j.register_binding(b.clone(), true, false).unwrap();
    let expired = request(&b, now() - Duration::days(2));
    let status = j
        .reject(expired, "expired", PressErrorCode::Expired, now())
        .unwrap();
    assert_eq!(status.state, PressState::Expired);
    let future = request(&b, now() + Duration::minutes(6));
    j.reject(future, "future", PressErrorCode::ClockMismatch, now())
        .unwrap();
    let mut unsupported = request(&b, now());
    unsupported.version = 99;
    assert!(
        j.reject(unsupported, "unsupported", PressErrorCode::Busy, now())
            .is_err()
    );
    assert!(
        j.reject(
            request(&b, now()),
            "invalid-code",
            PressErrorCode::TimedOut,
            now()
        )
        .is_err()
    );
    drop(j);
    assert!(PressJournal::open(&dir.path().join("j")).is_ok());
}

#[test]
fn press_queue_replaying_expired_queued_request_expires_it_while_build_lock_is_busy() {
    let dir = tempfile::tempdir().unwrap();
    let b = binding(dir.path());
    let mut j = PressJournal::open(&dir.path().join("j")).unwrap();
    j.register_binding(b.clone(), true, false).unwrap();
    let r = request(&b, now());
    j.accept(r.clone(), "a", now()).unwrap();
    let expired = j.accept(r.clone(), "a", r.expires_at).unwrap();
    assert_eq!(expired.state, PressState::Expired);
    assert_eq!(expired.revision, 2);
    assert_eq!(
        j.accept(r, "a", now() + Duration::days(2)).unwrap(),
        expired
    );
    assert!(j.admit(now() + Duration::days(2)).unwrap().is_none());
}
