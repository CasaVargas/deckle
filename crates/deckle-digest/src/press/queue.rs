// SPDX-License-Identifier: GPL-3.0-or-later
//! Deterministic admission and generation-fenced lifecycle, with injected UTC time.
use super::*;
use chrono::Duration;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobLease {
    pub job_id: Uuid,
    pub edition_id: Uuid,
    pub producer_id: Uuid,
    pub paper_id: Uuid,
    pub root: PathBuf,
    pub generation: u64,
    pub attempt: u8,
    pub started_at: DateTime<Utc>,
    pub deadline: DateTime<Utc>,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Binding {
    root: PathBuf,
    producer_id: Uuid,
    paper_id: Uuid,
    enabled: bool,
    stopped: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Entry {
    #[serde(default)]
    root: PathBuf,
    request: PressRequest,
    hash: String,
    first_seen: DateTime<Utc>,
    status: PressStatus,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Job {
    id: Uuid,
    edition: Uuid,
    producer: Uuid,
    paper: Uuid,
    root: PathBuf,
    first_seen: DateTime<Utc>,
    lease: Option<JobLease>,
    terminal: bool,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct JournalData {
    bindings: Vec<Binding>,
    entries: Vec<Entry>,
    jobs: Vec<Job>,
    #[serde(default)]
    outputs: Vec<OutputRecord>,
}
pub(crate) fn validate_data(data: &JournalData) -> bool {
    use std::collections::{HashMap, HashSet};
    let mut roots = HashSet::new();
    let mut bindings = HashSet::new();
    for b in &data.bindings {
        if !b.root.is_absolute()
            || !roots.insert(&b.root)
            || !bindings.insert((b.producer_id, b.paper_id, &b.root))
        {
            return false;
        }
    }
    let mut jobs = HashMap::new();
    let mut pending_jobs = HashSet::new();
    let mut active = 0;
    for j in &data.jobs {
        if jobs.insert(j.id, j).is_some()
            || !data
                .bindings
                .iter()
                .any(|b| b.root == j.root && b.producer_id == j.producer && b.paper_id == j.paper)
        {
            return false;
        }
        if let Some(l) = &j.lease {
            if l.job_id != j.id
                || l.edition_id != j.edition
                || l.root != j.root
                || l.producer_id != j.producer
                || l.paper_id != j.paper
                || !(1..=3).contains(&l.attempt)
                || l.generation != u64::from(l.attempt)
                || l.deadline != l.started_at + Duration::minutes(15)
            {
                return false;
            }
            if !j.terminal {
                active += 1;
            }
        } else if !j.terminal && !pending_jobs.insert((j.producer, j.paper)) {
            return false;
        }
    }
    if active > 1 {
        return false;
    }
    let mut requests = HashSet::new();
    let mut pending_counts = HashMap::new();
    let mut live_jobs = HashSet::new();
    for e in &data.entries {
        if !requests.insert(e.request.request_id) {
            return false;
        }
        let Some(b) = data.bindings.iter().find(|b| {
            b.producer_id == e.request.producer_id
                && b.paper_id == e.request.paper_id
                && b.root == e.root
        }) else {
            return false;
        };
        let binding = PressBinding {
            root: b.root.clone(),
            producer_id: b.producer_id,
            paper_id: b.paper_id,
        };
        if e.status.request_id != e.request.request_id {
            return false;
        }
        let Ok(bytes) = serde_json::to_vec(&e.request) else {
            return false;
        };
        if let Err(error) = decode_request(
            &bytes,
            Path::new(&format!(
                ".deckle-press/requests/{}/{}.json",
                e.request.reader_id, e.request.request_id
            )),
            &binding,
            e.first_seen,
        ) && (e.status.job_id.is_some()
            || !matches!(
                error.code,
                PressErrorCode::Expired | PressErrorCode::ClockMismatch
            )
            || e.status.error_code != Some(error.code))
        {
            return false;
        }
        let Ok(bytes) = serde_json::to_vec(&e.status) else {
            return false;
        };
        if decode_status(
            &bytes,
            Path::new(&format!(
                ".deckle-press/status/{}.json",
                e.request.request_id
            )),
            &binding,
        )
        .is_err()
        {
            return false;
        }
        let Some(job_id) = e.status.job_id else {
            if !matches!(e.status.state, PressState::Failed | PressState::Expired)
                || e.status.started_at.is_some()
                || !e.status.error_code.is_some_and(rejection_code)
            {
                return false;
            }
            continue;
        };
        let Some(j) = jobs.get(&job_id) else {
            return false;
        };
        if j.producer != binding.producer_id
            || j.paper != binding.paper_id
            || j.root != binding.root
        {
            return false;
        }
        if e.status
            .result
            .as_ref()
            .is_some_and(|r| r.edition_id != j.edition)
        {
            return false;
        }
        if e.status.started_at != j.lease.as_ref().map(|l| l.started_at)
            && e.status.state != PressState::Expired
            && e.status.state != PressState::Failed
        {
            return false;
        }
        if !terminal(e.status.state) {
            if j.terminal || (e.status.state == PressState::Building) != j.lease.is_some() {
                return false;
            }
            live_jobs.insert(j.id);
            let count = pending_counts.entry(e.request.paper_id).or_insert(0);
            *count += 1;
            if *count > 100 {
                return false;
            }
        } else if matches!(
            e.status.state,
            PressState::Published | PressState::ResultRemoved
        ) && (!j.terminal || j.lease.is_none())
        {
            return false;
        }
    }
    let mut editions = HashSet::new();
    for o in &data.outputs {
        for auth in [&o.shared_deletion, &o.archive_deletion]
            .into_iter()
            .flatten()
        {
            let mut paths = HashSet::new();
            if !auth
                .members
                .iter()
                .any(|m| m.relative.is_empty() && m.directory)
                || (o.published_at.is_some() && o.removed_at.is_none())
                || auth.members.iter().any(|m| {
                    !paths.insert(&m.relative)
                        || (!m.relative.is_empty()
                            && (m.relative.contains('\\')
                                || m.relative
                                    .split('/')
                                    .any(|p| p.is_empty() || p == "." || p == "..")
                                || std::path::Path::new(&m.relative)
                                    .components()
                                    .any(|p| !matches!(p, std::path::Component::Normal(_)))))
                })
            {
                return false;
            }
        }

        if o.stages.iter().any(|s| {
            s.generation == 0
                || s.generation > 3
                || !s.name.starts_with(&format!(".{}-", o.result.edition_id))
                || !s.name.ends_with(".tmp")
                || s.name
                    .get(38..s.name.len().saturating_sub(4))
                    .is_none_or(|id| Uuid::parse_str(id).is_err())
        }) {
            return false;
        }
        if !editions.insert(o.result.edition_id)
            || validate_result(&o.lease, &o.result).is_err()
            || !data.jobs.iter().any(|j| {
                j.id == o.lease.job_id
                    && j.edition == o.lease.edition_id
                    && j.root == o.lease.root
                    && j.producer == o.lease.producer_id
                    && j.paper == o.lease.paper_id
            })
            || (o.removed_at.is_some() && o.published_at.is_none())
            || (o.deleted && (o.removed_at.is_none() || o.deletion_completed_at.is_none()))
            || (!o.deleted && o.deletion_completed_at.is_some())
        {
            return false;
        }
    }
    data.jobs
        .iter()
        .all(|j| j.terminal || live_jobs.contains(&j.id))
}
fn conflict() -> PressError {
    PressError::new(PressErrorCode::ProtocolConflict)
}
fn terminal(state: PressState) -> bool {
    !matches!(state, PressState::Queued | PressState::Building)
}
fn policy(b: &Binding) -> Result<(), PressError> {
    if !b.enabled {
        return Err(PressError::new(PressErrorCode::Disabled));
    }
    if b.stopped {
        return Err(PressError::new(PressErrorCode::PressesStopped));
    }
    Ok(())
}
impl PressJournal {
    /// Register routing from validated local setup; request bytes cannot supply paths.
    pub fn register_binding(
        &mut self,
        binding: PressBinding,
        enabled: bool,
        stopped: bool,
    ) -> Result<(), PressError> {
        self.ensure_healthy()?;
        let root = std::fs::canonicalize(binding.root).map_err(|_| journal::storage())?;
        let mut data = self.data.clone();
        if data.bindings.iter().any(|b| {
            b.root == root
                && (b.producer_id != binding.producer_id || b.paper_id != binding.paper_id)
        }) {
            return Err(conflict());
        }
        if let Some(b) = data.bindings.iter_mut().find(|b| {
            b.producer_id == binding.producer_id && b.paper_id == binding.paper_id && b.root == root
        }) {
            b.enabled = enabled;
            b.stopped = stopped;
        } else {
            data.bindings.push(Binding {
                root,
                producer_id: binding.producer_id,
                paper_id: binding.paper_id,
                enabled,
                stopped,
            });
        }
        self.commit(data)
    }
    pub fn accept(
        &mut self,
        request: PressRequest,
        content_hash: &str,
        now: DateTime<Utc>,
    ) -> Result<PressStatus, PressError> {
        let binding = self.unique_binding(&request)?;
        self.accept_in(&binding, request, content_hash, now)
    }
    /// Accept within a validated local folder. Identical IDs in another root conflict;
    /// request bytes never select routing, and jobs coalesce only within this binding.
    pub fn accept_in(
        &mut self,
        binding: &PressBinding,
        request: PressRequest,
        content_hash: &str,
        now: DateTime<Utc>,
    ) -> Result<PressStatus, PressError> {
        self.ensure_healthy()?;
        if let Some(e) = self
            .data
            .entries
            .iter()
            .find(|e| e.request.request_id == request.request_id)
        {
            if e.hash != content_hash
                || e.request != request
                || e.root != binding.root
                || binding.producer_id != request.producer_id
                || binding.paper_id != request.paper_id
            {
                return Err(conflict());
            }
            let mut data = self.data.clone();
            if expire_queued(&mut data, now)? {
                self.commit(data)?;
            }
            return self.status(request.request_id).ok_or_else(conflict);
        }
        let b = self
            .data
            .bindings
            .iter()
            .find(|b| {
                b.producer_id == request.producer_id
                    && b.paper_id == request.paper_id
                    && b.root == binding.root
                    && b.producer_id == binding.producer_id
                    && b.paper_id == binding.paper_id
            })
            .ok_or_else(conflict)?;
        policy(b)?;
        let binding = PressBinding {
            root: b.root.clone(),
            producer_id: b.producer_id,
            paper_id: b.paper_id,
        };
        let bytes = serde_json::to_vec(&request).map_err(|_| conflict())?;
        decode_request(
            &bytes,
            Path::new(&format!(
                ".deckle-press/requests/{}/{}.json",
                request.reader_id, request.request_id
            )),
            &binding,
            now,
        )?;
        let mut data = self.data.clone();
        expire_queued(&mut data, now)?;
        if data
            .entries
            .iter()
            .filter(|e| e.request.paper_id == request.paper_id && !terminal(e.status.state))
            .count()
            >= 100
        {
            return Err(PressError::new(PressErrorCode::Busy));
        }
        let idx = if let Some(idx) = data.jobs.iter().position(|j| {
            j.paper == b.paper_id && j.producer == b.producer_id && j.root == b.root && !j.terminal
        }) {
            idx
        } else {
            if data
                .jobs
                .iter()
                .any(|j| j.paper == b.paper_id && !j.terminal && j.lease.is_none())
            {
                return Err(PressError::new(PressErrorCode::Busy));
            }
            data.jobs.push(Job {
                id: Uuid::new_v4(),
                edition: Uuid::new_v4(),
                producer: b.producer_id,
                paper: b.paper_id,
                root: b.root.clone(),
                first_seen: now,
                lease: None,
                terminal: false,
            });
            data.jobs.len() - 1
        };
        let j = &data.jobs[idx];
        let status = PressStatus {
            format: "deckle-press-status".into(),
            version: 1,
            request_id: request.request_id,
            producer_id: request.producer_id,
            paper_id: request.paper_id,
            revision: 1,
            state: if j.lease.is_some() {
                PressState::Building
            } else {
                PressState::Queued
            },
            updated_at: now,
            job_id: Some(j.id),
            started_at: j.lease.as_ref().map(|l| l.started_at),
            error_code: None,
            result: None,
            removed_at: None,
            message: None,
        };
        data.entries.push(Entry {
            root: binding.root.clone(),
            request,
            hash: content_hash.to_owned(),
            first_seen: now,
            status: status.clone(),
        });
        self.commit(data)?;
        Ok(status)
    }
    /// Caller must acquire the existing build run_lock before this call. Waiting does
    /// not start the deadline. This supervisor lock must cover all papers locally.
    pub fn admit(&mut self, now: DateTime<Utc>) -> Result<Option<JobLease>, PressError> {
        self.ensure_healthy()?;
        let mut data = self.data.clone();
        let changed = expire_queued(&mut data, now)?;
        if data.jobs.iter().any(|j| j.lease.is_some() && !j.terminal) {
            if changed {
                self.commit(data)?;
            }
            return Ok(None);
        }
        let selected = data
            .jobs
            .iter()
            .filter(|j| !j.terminal && j.lease.is_none())
            .filter(|j| {
                !data.jobs.iter().any(|previous| {
                    previous.paper == j.paper
                        && previous
                            .lease
                            .as_ref()
                            .is_some_and(|l| now < l.started_at + Duration::minutes(5))
                })
            })
            .min_by_key(|j| (j.first_seen, j.id))
            .map(|j| j.id);
        let lease = selected.map(|id| {
            let j = data.jobs.iter_mut().find(|j| j.id == id).unwrap();
            let lease = JobLease {
                job_id: id,
                edition_id: j.edition,
                producer_id: j.producer,
                paper_id: j.paper,
                root: j.root.clone(),
                generation: 1,
                attempt: 1,
                started_at: now,
                deadline: now + Duration::minutes(15),
            };
            j.lease = Some(lease.clone());
            for e in &mut data.entries {
                if e.status.job_id == Some(id) && e.status.state == PressState::Queued {
                    e.status.state = PressState::Building;
                    e.status.started_at = Some(now);
                    e.status.updated_at = now;
                    e.status.revision += 1;
                }
            }
            lease
        });
        if lease.is_some() || changed {
            self.commit(data)?;
        }
        Ok(lease)
    }
    pub fn status(&self, request_id: Uuid) -> Option<PressStatus> {
        self.data
            .entries
            .iter()
            .find(|e| e.request.request_id == request_id)
            .map(|e| e.status.clone())
    }
    pub fn active(&self) -> Option<JobLease> {
        self.data
            .jobs
            .iter()
            .find(|j| !j.terminal && j.lease.is_some())
            .and_then(|j| j.lease.clone())
    }
    pub fn validate_lease(&self, lease: &JobLease, now: DateTime<Utc>) -> Result<(), PressError> {
        self.check_generation(lease)?;
        if now >= lease.deadline {
            return Err(PressError::new(PressErrorCode::TimedOut));
        }
        Ok(())
    }
    /// Final filesystem transaction executes while this journal owns the supervisor
    /// lock. Verify expensive assets/binding first, then sample `now` immediately
    /// before calling. The finalizer must check the deadline again before rename if
    /// it performs appreciable work. It cannot release this journal's local lock.
    pub fn publish(
        &mut self,
        lease: &JobLease,
        result: PressResult,
        now: DateTime<Utc>,
        finalize: impl FnOnce() -> Result<(), PressError>,
    ) -> Result<(), PressError> {
        self.validate_lease(lease, now)?;
        validate_result(lease, &result)?;
        finalize()?;
        self.commit_result(lease, result, now)
    }
}

impl PressJournal {
    /// Regenerate shared status after a crash; callers write only committed snapshots.
    pub fn statuses(&self) -> Vec<PressStatus> {
        self.data.entries.iter().map(|e| e.status.clone()).collect()
    }
    /// Recover only after confirming the previous worker has exited or been terminated.
    /// Keep the build run_lock held throughout recovery. Never extend the first deadline.
    pub fn recover(
        &mut self,
        previous: &JobLease,
        now: DateTime<Utc>,
    ) -> Result<JobLease, PressError> {
        self.check_generation(previous)?;
        let error = if now >= previous.deadline {
            Some(PressErrorCode::TimedOut)
        } else if previous.attempt >= 3 {
            Some(PressErrorCode::BuildFailed)
        } else {
            None
        };
        if let Some(code) = error {
            self.fail(previous, code, now)?;
            return Err(PressError::new(code));
        }
        let mut data = self.data.clone();
        let mut lease = previous.clone();
        lease.attempt += 1;
        lease.generation += 1;
        data.jobs
            .iter_mut()
            .find(|j| j.id == lease.job_id)
            .unwrap()
            .lease = Some(lease.clone());
        for e in &mut data.entries {
            if e.status.job_id == Some(lease.job_id) && !terminal(e.status.state) {
                e.status.updated_at = now;
                e.status.revision += 1;
            }
        }
        self.commit(data)?;
        Ok(lease)
    }
    /// Fences the generation before failure statuses can be written. Caller must stop
    /// the previous worker before releasing the build slot or admitting another job.
    pub fn fail(
        &mut self,
        lease: &JobLease,
        code: PressErrorCode,
        now: DateTime<Utc>,
    ) -> Result<(), PressError> {
        self.check_generation(lease)?;
        if !matches!(
            code,
            PressErrorCode::SourceFailure
                | PressErrorCode::BuildFailed
                | PressErrorCode::TimedOut
                | PressErrorCode::StorageUnavailable
        ) {
            return Err(conflict());
        }
        let mut data = self.data.clone();
        data.jobs
            .iter_mut()
            .find(|j| j.id == lease.job_id)
            .unwrap()
            .terminal = true;
        for e in &mut data.entries {
            if e.status.job_id == Some(lease.job_id) && !terminal(e.status.state) {
                e.status.state = PressState::Failed;
                e.status.error_code = Some(code);
                e.status.updated_at = now;
                e.status.revision += 1;
            }
        }
        self.commit(data)
    }
    fn check_generation(&self, lease: &JobLease) -> Result<(), PressError> {
        self.ensure_healthy()?;
        if self
            .data
            .jobs
            .iter()
            .any(|j| !j.terminal && j.lease.as_ref() == Some(lease))
        {
            Ok(())
        } else {
            Err(conflict())
        }
    }
}

fn validate_result(lease: &JobLease, result: &PressResult) -> Result<(), PressError> {
    if result.edition_id != lease.edition_id
        || result.bundle != format!(".deckle-press/editions/{}.deckle", lease.edition_id)
        || result.manifest_sha256.len() != 64
        || !result
            .manifest_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(conflict());
    }
    Ok(())
}
impl PressJournal {
    /// Reconcile only a fully verified immutable final output that already exists.
    /// This recovery path never creates/renames an output and runs before deciding
    /// timeout. Verification must check job identity, binding, manifest and assets.
    pub fn reconcile_published(
        &mut self,
        lease: &JobLease,
        result: PressResult,
        now: DateTime<Utc>,
        verify_committed: impl FnOnce() -> Result<(), PressError>,
    ) -> Result<(), PressError> {
        self.check_generation(lease)?;
        validate_result(lease, &result)?;
        verify_committed()?;
        self.commit_result(lease, result, now)
    }
    pub fn binding_for_request(&self, request_id: Uuid) -> Option<PressBinding> {
        let entry = self
            .data
            .entries
            .iter()
            .find(|e| e.request.request_id == request_id)?;
        let b = self.data.bindings.iter().find(|b| {
            b.producer_id == entry.request.producer_id
                && b.paper_id == entry.request.paper_id
                && b.root == entry.root
        })?;
        Some(PressBinding {
            root: b.root.clone(),
            producer_id: b.producer_id,
            paper_id: b.paper_id,
        })
    }

    fn commit_result(
        &mut self,
        lease: &JobLease,
        result: PressResult,
        now: DateTime<Utc>,
    ) -> Result<(), PressError> {
        let mut data = self.data.clone();
        data.jobs
            .iter_mut()
            .find(|j| j.id == lease.job_id)
            .unwrap()
            .terminal = true;
        for e in &mut data.entries {
            if e.status.job_id == Some(lease.job_id) && !terminal(e.status.state) {
                e.status.state = PressState::Published;
                e.status.result = Some(result.clone());
                e.status.updated_at = now;
                e.status.revision += 1;
            }
        }
        if let Some(o) = data
            .outputs
            .iter_mut()
            .find(|o| o.lease.job_id == lease.job_id)
        {
            o.published_at = Some(now);
        }
        self.commit(data)
    }
}

fn expire_queued(data: &mut JournalData, now: DateTime<Utc>) -> Result<bool, PressError> {
    let mut changed = false;
    for e in &mut data.entries {
        if e.status.state == PressState::Queued {
            let b = data
                .bindings
                .iter()
                .find(|b| {
                    b.paper_id == e.request.paper_id
                        && b.producer_id == e.request.producer_id
                        && b.root == e.root
                })
                .ok_or_else(conflict)?;
            let error = if now >= e.request.expires_at {
                Some(PressErrorCode::Expired)
            } else {
                policy(b).err().map(|e| e.code)
            };
            if let Some(code) = error {
                e.status.state = if code == PressErrorCode::Expired {
                    PressState::Expired
                } else {
                    PressState::Failed
                };
                e.status.error_code = Some(code);
                e.status.revision += 1;
                e.status.updated_at = now;
                changed = true;
            }
        }
    }
    for j in &mut data.jobs {
        if !j.terminal
            && !data
                .entries
                .iter()
                .any(|e| e.status.job_id == Some(j.id) && !terminal(e.status.state))
        {
            j.terminal = true;
            changed = true;
        }
    }

    Ok(changed)
}

fn rejection_code(code: PressErrorCode) -> bool {
    matches!(
        code,
        PressErrorCode::Busy
            | PressErrorCode::Disabled
            | PressErrorCode::PressesStopped
            | PressErrorCode::Expired
            | PressErrorCode::ClockMismatch
    )
}
impl PressJournal {
    /// Durably refuse a safely bound, typed request. Its immutable ID remains terminal;
    /// a later explicit retry needs a new ID. Malformed/unbound controls are diagnostics.
    /// Existing accepted or rejected IDs return their original committed status.
    pub fn reject(
        &mut self,
        request: PressRequest,
        content_hash: &str,
        code: PressErrorCode,
        now: DateTime<Utc>,
    ) -> Result<PressStatus, PressError> {
        let binding = self.unique_binding(&request)?;
        self.reject_in(&binding, request, content_hash, code, now)
    }
    pub fn reject_in(
        &mut self,
        binding: &PressBinding,
        request: PressRequest,
        content_hash: &str,
        code: PressErrorCode,
        now: DateTime<Utc>,
    ) -> Result<PressStatus, PressError> {
        self.ensure_healthy()?;
        if self
            .data
            .entries
            .iter()
            .any(|e| e.request.request_id == request.request_id)
        {
            return self.accept_in(binding, request, content_hash, now);
        }
        if !rejection_code(code) {
            return Err(conflict());
        }
        let b = self
            .data
            .bindings
            .iter()
            .find(|b| {
                b.producer_id == request.producer_id
                    && b.paper_id == request.paper_id
                    && b.root == binding.root
                    && b.producer_id == binding.producer_id
                    && b.paper_id == binding.paper_id
            })
            .ok_or_else(conflict)?;
        let binding = PressBinding {
            root: b.root.clone(),
            producer_id: b.producer_id,
            paper_id: b.paper_id,
        };
        let bytes = serde_json::to_vec(&request).map_err(|_| conflict())?;
        match decode_request(
            &bytes,
            Path::new(&format!(
                ".deckle-press/requests/{}/{}.json",
                request.reader_id, request.request_id
            )),
            &binding,
            now,
        ) {
            Ok(_)
                if matches!(
                    code,
                    PressErrorCode::Expired | PressErrorCode::ClockMismatch
                ) =>
            {
                return Err(conflict());
            }
            Err(error)
                if error.code == code
                    && matches!(
                        code,
                        PressErrorCode::Expired | PressErrorCode::ClockMismatch
                    ) => {}
            Err(error) => return Err(error),
            Ok(_) => (),
        }
        let status = PressStatus {
            format: "deckle-press-status".into(),
            version: 1,
            request_id: request.request_id,
            producer_id: request.producer_id,
            paper_id: request.paper_id,
            revision: 1,
            state: if code == PressErrorCode::Expired {
                PressState::Expired
            } else {
                PressState::Failed
            },
            updated_at: now,
            job_id: None,
            started_at: None,
            error_code: Some(code),
            result: None,
            removed_at: None,
            message: None,
        };
        // Already-known IDs returned above retain their full reconciliation history.
        // Unknown controls at the terminal horizon do not renew their journal lifetime.
        if code == PressErrorCode::Expired && now - request.expires_at >= Duration::days(30) {
            return Ok(status);
        }
        let mut data = self.data.clone();
        data.entries.push(Entry {
            root: binding.root.clone(),
            request,
            hash: content_hash.to_owned(),
            first_seen: now,
            status: status.clone(),
        });
        self.commit(data)?;
        Ok(status)
    }
}

/// Paths are always derived from the binding and stable IDs, never deserialized paths.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct OutputRecord {
    pub lease: JobLease,
    pub result: PressResult,
    pub published_at: Option<DateTime<Utc>>,
    pub removed_at: Option<DateTime<Utc>>,
    pub deleted: bool,
    #[serde(default)]
    pub deletion_completed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub stages: Vec<StageRecord>,
    #[serde(default)]
    pub shared_deletion: Option<DeletionAuthorization>,
    #[serde(default)]
    pub archive_deletion: Option<DeletionAuthorization>,
}
impl PressJournal {
    pub(crate) fn record_intent(
        &mut self,
        lease: &JobLease,
        result: PressResult,
        now: DateTime<Utc>,
    ) -> Result<(), PressError> {
        self.validate_lease(lease, now)?;
        validate_result(lease, &result)?;
        let mut data = self.data.clone();
        if let Some(old) = data
            .outputs
            .iter_mut()
            .find(|o| o.lease.job_id == lease.job_id)
        {
            if old.published_at.is_some() || old.removed_at.is_some() {
                return Err(conflict());
            }
            old.result = result;
            old.lease = lease.clone();
        } else {
            data.outputs.push(OutputRecord {
                lease: lease.clone(),
                result,
                published_at: None,
                removed_at: None,
                deleted: false,
                deletion_completed_at: None,
                stages: Vec::new(),
                shared_deletion: None,
                archive_deletion: None,
            });
        }
        self.commit(data)
    }
    pub(crate) fn outputs(&self) -> Vec<OutputRecord> {
        self.data.outputs.clone()
    }
    pub(crate) fn job_status(&self, job: Uuid) -> Option<PressStatus> {
        self.data
            .entries
            .iter()
            .find(|e| {
                e.status.job_id == Some(job)
                    && matches!(
                        e.status.state,
                        PressState::Published | PressState::ResultRemoved
                    )
            })
            .map(|e| e.status.clone())
    }
    pub(crate) fn mark_removed(
        &mut self,
        edition: Uuid,
        now: DateTime<Utc>,
    ) -> Result<(), PressError> {
        self.ensure_healthy()?;
        let mut data = self.data.clone();
        let o = data
            .outputs
            .iter_mut()
            .find(|o| o.result.edition_id == edition)
            .ok_or_else(conflict)?;
        if o.removed_at.is_some() {
            return Ok(());
        }
        if o.published_at.is_none()
            || data
                .jobs
                .iter()
                .any(|j| j.id == o.lease.job_id && !j.terminal)
        {
            return Err(conflict());
        }
        o.removed_at = Some(now);
        for e in &mut data.entries {
            if e.status
                .result
                .as_ref()
                .is_some_and(|r| r.edition_id == edition)
            {
                e.status.state = PressState::ResultRemoved;
                e.status.removed_at = Some(now);
                e.status.message = Some(RESULT_REMOVED_MESSAGE.into());
                e.status.revision += 1;
                e.status.updated_at = now;
            }
        }
        self.commit(data)
    }
    pub(crate) fn mark_deleted(
        &mut self,
        edition: Uuid,
        now: DateTime<Utc>,
    ) -> Result<(), PressError> {
        let mut data = self.data.clone();
        let output = data
            .outputs
            .iter_mut()
            .find(|o| o.result.edition_id == edition)
            .ok_or_else(conflict)?;
        output.deleted = true;
        output.deletion_completed_at = Some(now);
        self.commit(data)
    }
    pub(crate) fn forget_old(
        &mut self,
        now: DateTime<Utc>,
        mut remove_status: impl FnMut(&PressBinding, Uuid) -> Result<(), PressError>,
    ) -> Result<(), PressError> {
        self.ensure_healthy()?;
        let mut data = self.data.clone();
        let old: std::collections::HashSet<_> = data
            .entries
            .iter()
            .filter(|e| terminal(e.status.state) && now - e.status.updated_at >= Duration::days(30))
            .filter(|e| {
                !data.outputs.iter().any(|o| {
                    e.status.job_id == Some(o.lease.job_id)
                        && (!o.deleted
                            || o.deletion_completed_at
                                .is_some_and(|at| now - at < Duration::days(30)))
                })
            })
            .map(|e| e.request.request_id)
            .collect();
        let removed: Vec<_> = data
            .entries
            .iter()
            .filter(|e| old.contains(&e.request.request_id))
            .filter_map(|e| {
                self.binding_for_request(e.request.request_id)
                    .map(|b| (b, e.request.request_id))
            })
            .collect();
        let mut error = None;
        let mut completed = std::collections::HashSet::new();
        for (binding, id) in removed {
            match remove_status(&binding, id) {
                Ok(()) => {
                    completed.insert(id);
                }
                Err(e) => {
                    error.get_or_insert(e);
                }
            }
        }
        data.entries
            .retain(|e| !completed.contains(&e.request.request_id));
        data.outputs.retain(|o| {
            data.entries
                .iter()
                .any(|e| e.status.job_id == Some(o.lease.job_id))
                || !o.deleted
                || o.deletion_completed_at
                    .is_none_or(|at| now - at < Duration::days(30))
        });
        data.jobs.retain(|j| {
            !j.terminal
                || data.entries.iter().any(|e| e.status.job_id == Some(j.id))
                || data.outputs.iter().any(|o| o.lease.job_id == j.id)
        });
        self.commit(data)?;
        error.map_or(Ok(()), Err)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct StageRecord {
    pub name: String,
    pub local: bool,
    pub generation: u64,
}
impl PressJournal {
    pub(crate) fn record_stage(
        &mut self,
        lease: &JobLease,
        name: String,
        local: bool,
    ) -> Result<(), PressError> {
        self.check_generation(lease)?;
        let mut data = self.data.clone();
        data.outputs
            .iter_mut()
            .find(|o| o.lease.job_id == lease.job_id)
            .ok_or_else(conflict)?
            .stages
            .push(StageRecord {
                name,
                local,
                generation: lease.generation,
            });
        self.commit(data)
    }
    pub(crate) fn job_terminal(&self, job: Uuid) -> bool {
        self.data.jobs.iter().any(|j| j.id == job && j.terminal)
    }
}

impl PressJournal {
    pub(crate) fn forget_intent(&mut self, job: Uuid) -> Result<(), PressError> {
        self.ensure_healthy()?;
        let mut data = self.data.clone();
        if !self.job_terminal(job)
            || data
                .outputs
                .iter()
                .any(|o| o.lease.job_id == job && o.published_at.is_some())
        {
            return Err(conflict());
        }
        data.outputs.retain(|o| o.lease.job_id != job);
        self.commit(data)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ObjectIdentity {
    pub volume: u64,
    pub file: u64,
    pub created: Option<(u64, u32)>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DeletionMember {
    pub relative: String,
    pub directory: bool,
    pub identity: ObjectIdentity,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DeletionAuthorization {
    pub members: Vec<DeletionMember>,
}
impl PressJournal {
    pub(crate) fn authorize_deletion(
        &mut self,
        edition: Uuid,
        local: bool,
        authorization: DeletionAuthorization,
    ) -> Result<(), PressError> {
        self.ensure_healthy()?;
        let mut data = self.data.clone();
        let o = data
            .outputs
            .iter_mut()
            .find(|o| o.result.edition_id == edition)
            .ok_or_else(conflict)?;
        if !self.job_terminal(o.lease.job_id)
            || (o.published_at.is_some() && o.removed_at.is_none())
        {
            return Err(conflict());
        }
        let saved = if local {
            &mut o.archive_deletion
        } else {
            &mut o.shared_deletion
        };
        if let Some(old) = saved {
            if old != &authorization {
                return Err(conflict());
            }
            return Ok(());
        }
        *saved = Some(authorization);
        self.commit(data)
    }
}

/// Old snapshots had exactly one binding per producer/paper. Resolve absent entry roots
/// before validation, and refuse ambiguous migrations rather than reroute rejected IDs.
pub(crate) fn migrate_bindings(data: &mut JournalData) -> bool {
    for entry in &mut data.entries {
        if entry.root.as_os_str().is_empty() {
            let mut matches = data.bindings.iter().filter(|b| {
                b.producer_id == entry.request.producer_id && b.paper_id == entry.request.paper_id
            });
            let Some(binding) = matches.next() else {
                return false;
            };
            if matches.next().is_some() {
                return false;
            }
            entry.root = binding.root.clone();
        }
    }
    true
}
impl PressJournal {
    fn unique_binding(&self, request: &PressRequest) -> Result<PressBinding, PressError> {
        self.ensure_healthy()?;
        if let Some(binding) = self.binding_for_request(request.request_id) {
            return Ok(binding);
        }
        let mut matches = self
            .data
            .bindings
            .iter()
            .filter(|b| b.producer_id == request.producer_id && b.paper_id == request.paper_id);
        let b = matches.next().ok_or_else(conflict)?;
        if matches.next().is_some() {
            return Err(conflict());
        }
        Ok(PressBinding {
            root: b.root.clone(),
            producer_id: b.producer_id,
            paper_id: b.paper_id,
        })
    }
    /// Known durable roots, including disabled/offline roots. These identities are local.
    pub fn bindings(&self) -> Vec<PressBinding> {
        self.data
            .bindings
            .iter()
            .map(|b| PressBinding {
                root: b.root.clone(),
                producer_id: b.producer_id,
                paper_id: b.paper_id,
            })
            .collect()
    }
    /// Change policy for an already registered exact root without accessing that folder.
    /// This lets local opt-out remain effective while its sync provider is unavailable.
    pub fn set_binding_policy(
        &mut self,
        binding: &PressBinding,
        enabled: bool,
        stopped: bool,
    ) -> Result<(), PressError> {
        self.ensure_healthy()?;
        let mut data = self.data.clone();
        let b = data
            .bindings
            .iter_mut()
            .find(|b| {
                b.root == binding.root
                    && b.paper_id == binding.paper_id
                    && b.producer_id == binding.producer_id
            })
            .ok_or_else(conflict)?;
        if b.enabled == enabled && b.stopped == stopped {
            return Ok(());
        }
        b.enabled = enabled;
        b.stopped = stopped;
        self.commit(data)
    }
}
