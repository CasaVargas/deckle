// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded, file-based fresh edition controls. Decoders do no filesystem I/O;
//! callers must separately reject symlinks and non-regular files before reading.
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::path::{Path, PathBuf};
pub use uuid::Uuid;

pub const MAX_CONTROL_BYTES: usize = 16_384;
/// Local routing context, never written into shared controls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PressBinding {
    pub root: PathBuf,
    pub producer_id: Uuid,
    pub paper_id: Uuid,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FreshIdentity {
    pub paper_id: Uuid,
    pub edition_id: Uuid,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PressErrorCode {
    Busy,
    ClockMismatch,
    Expired,
    Disabled,
    PressesStopped,
    Unsupported,
    SourceFailure,
    BuildFailed,
    TimedOut,
    StorageUnavailable,
    ProtocolConflict,
}
/// Safe to expose to compatible clients; never contains provider or OS error text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PressError {
    pub code: PressErrorCode,
}
impl PressError {
    pub fn new(code: PressErrorCode) -> Self {
        Self { code }
    }
}
impl std::fmt::Display for PressError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self.code {
            PressErrorCode::Busy => "The press is busy; try again later",
            PressErrorCode::ClockMismatch => "Check the device clock and try again",
            PressErrorCode::Expired => "The request has expired",
            PressErrorCode::Disabled => "Fresh-paper requests are disabled",
            PressErrorCode::PressesStopped => "The presses are stopped",
            PressErrorCode::Unsupported => "This control file is unsupported or incomplete",
            PressErrorCode::SourceFailure => "Sources could not be loaded",
            PressErrorCode::BuildFailed => "The fresh edition could not be built",
            PressErrorCode::TimedOut => "The fresh edition took too long to build",
            PressErrorCode::StorageUnavailable => "The shared folder is unavailable",
            PressErrorCode::ProtocolConflict => "The request conflicts with its folder binding",
        })
    }
}
impl std::error::Error for PressError {}
fn unsupported() -> PressError {
    PressError::new(PressErrorCode::Unsupported)
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PressOperation {
    #[default]
    FreshFull,
    ReadingEdition,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProducerManifest {
    pub format: String,
    pub version: u32,
    #[serde(deserialize_with = "canonical_uuid")]
    pub producer_id: Uuid,
    #[serde(deserialize_with = "canonical_uuid")]
    pub paper_id: Uuid,
    pub paper_name: String,
    pub enabled: bool,
    pub operations: Vec<PressOperation>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reading_editions: bool,
    pub updated_at: DateTime<Utc>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PressRequest {
    pub format: String,
    pub version: u32,
    #[serde(deserialize_with = "canonical_uuid")]
    pub request_id: Uuid,
    #[serde(deserialize_with = "canonical_uuid")]
    pub reader_id: Uuid,
    #[serde(deserialize_with = "canonical_uuid")]
    pub producer_id: Uuid,
    #[serde(deserialize_with = "canonical_uuid")]
    pub paper_id: Uuid,
    pub operation: PressOperation,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub articles: Option<Vec<String>>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PressState {
    Queued,
    Building,
    Published,
    Failed,
    Expired,
    ResultRemoved,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PressResult {
    #[serde(deserialize_with = "canonical_uuid")]
    pub edition_id: Uuid,
    pub bundle: String,
    pub manifest_sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PressStatus {
    pub format: String,
    pub version: u32,
    #[serde(deserialize_with = "canonical_uuid")]
    pub request_id: Uuid,
    #[serde(deserialize_with = "canonical_uuid")]
    pub producer_id: Uuid,
    #[serde(deserialize_with = "canonical_uuid")]
    pub paper_id: Uuid,
    pub revision: u64,
    pub state: PressState,
    pub updated_at: DateTime<Utc>,
    #[serde(
        default,
        deserialize_with = "present_canonical_uuid",
        skip_serializing_if = "Option::is_none"
    )]
    pub job_id: Option<Uuid>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub error_code: Option<PressErrorCode>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub result: Option<PressResult>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub removed_at: Option<DateTime<Utc>>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub message: Option<String>,
}
// Public controls use the exact lowercase, hyphenated spelling from the schema.
fn canonical_uuid<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Uuid, D::Error> {
    let value = String::deserialize(deserializer)?;
    let id = Uuid::parse_str(&value).map_err(serde::de::Error::custom)?;
    if value != id.to_string() {
        return Err(serde::de::Error::custom("noncanonical UUID"));
    }
    Ok(id)
}
fn present_canonical_uuid<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Uuid>, D::Error> {
    canonical_uuid(deserializer).map(Some)
}
// Missing optional fields are accepted; a present JSON null is not the declared type.
fn present_optional<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}
pub const RESULT_REMOVED_MESSAGE: &str = "Fresh edition is no longer available; print another";
fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, PressError> {
    if bytes.len() > MAX_CONTROL_BYTES {
        return Err(unsupported());
    }
    serde_json::from_slice(bytes).map_err(|_| unsupported())
}
fn bound(producer: Uuid, paper: Uuid, binding: &PressBinding) -> Result<(), PressError> {
    if producer != binding.producer_id || paper != binding.paper_id {
        return Err(PressError::new(PressErrorCode::ProtocolConflict));
    }
    Ok(())
}
// Compare raw path spelling too: Path equality normalizes interior `.` components.
fn exact_path(relative: &Path, expected: String) -> Result<(), PressError> {
    let spelling = relative.to_str().ok_or_else(unsupported)?;
    // Native Windows paths use backslashes; JSON result paths always use '/'.
    #[cfg(windows)]
    let spelling = spelling.replace('\\', "/");
    if spelling != expected {
        return Err(unsupported());
    }
    Ok(())
}
pub fn decode_request(
    bytes: &[u8],
    relative: &Path,
    binding: &PressBinding,
    now: DateTime<Utc>,
) -> Result<PressRequest, PressError> {
    let r: PressRequest = decode(bytes)?;
    if r.format != "deckle-press-request" || r.version != 1 {
        return Err(unsupported());
    }
    validate_input(r.operation, r.articles.as_deref())?;
    bound(r.producer_id, r.paper_id, binding)?;
    exact_path(
        relative,
        format!(
            ".deckle-press/requests/{}/{}.json",
            r.reader_id, r.request_id
        ),
    )?;
    if r.expires_at - r.created_at != Duration::hours(24)
        || r.created_at > now + Duration::minutes(5)
    {
        return Err(PressError::new(PressErrorCode::ClockMismatch));
    }
    if r.expires_at <= now {
        return Err(PressError::new(PressErrorCode::Expired));
    }
    Ok(r)
}
pub fn decode_manifest(
    bytes: &[u8],
    binding: &PressBinding,
) -> Result<ProducerManifest, PressError> {
    let m: ProducerManifest = decode(bytes)?;
    if m.format != "deckle-press" || m.version != 1 || m.operations != [PressOperation::FreshFull] {
        return Err(unsupported());
    }
    bound(m.producer_id, m.paper_id, binding)?;
    Ok(m)
}
pub fn decode_status(
    bytes: &[u8],
    relative: &Path,
    binding: &PressBinding,
) -> Result<PressStatus, PressError> {
    let s: PressStatus = decode(bytes)?;
    if s.format != "deckle-press-status" || s.version != 1 || s.revision == 0 {
        return Err(unsupported());
    }
    bound(s.producer_id, s.paper_id, binding)?;
    exact_path(
        relative,
        format!(".deckle-press/status/{}.json", s.request_id),
    )?;
    let needs_result = matches!(s.state, PressState::Published | PressState::ResultRemoved);
    if s.result.is_some() != needs_result {
        return Err(unsupported());
    }
    if let Some(r) = &s.result
        && (r.bundle != format!(".deckle-press/editions/{}.deckle", r.edition_id)
            || r.manifest_sha256.len() != 64
            || !r
                .manifest_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    {
        return Err(unsupported());
    }
    if s.state == PressState::ResultRemoved {
        if s.removed_at.is_none() || s.message.as_deref() != Some(RESULT_REMOVED_MESSAGE) {
            return Err(unsupported());
        }
    } else if s.removed_at.is_some() || s.message.is_some() {
        return Err(unsupported());
    }
    Ok(s)
}

/// Validate the immutable ordered selection independently of any filesystem routing.
pub fn validate_input(
    operation: PressOperation,
    articles: Option<&[String]>,
) -> Result<(), PressError> {
    match (operation, articles) {
        (PressOperation::FreshFull, None) => Ok(()),
        (PressOperation::ReadingEdition, Some(urls)) if (1..=20).contains(&urls.len()) => {
            let mut seen = std::collections::HashSet::new();
            for value in urls {
                if value.len() > 2048 || !seen.insert(normalize_article_url(value)?) {
                    return Err(unsupported());
                }
            }
            Ok(())
        }
        _ => Err(unsupported()),
    }
}
/// Strip tracking and fragment identity without rewriting retained signed query bytes.
pub fn normalize_article_url(value: &str) -> Result<String, PressError> {
    if value.is_empty() || value.chars().any(char::is_control) || value.trim() != value {
        return Err(unsupported());
    }
    let mut url = crate::url::Url::parse(value).map_err(|_| unsupported())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || value
            .split("://")
            .nth(1)
            .is_some_and(|s| s.split(['/', '?', '#']).next().unwrap_or("").contains('@'))
    {
        return Err(unsupported());
    }
    url.set_fragment(None);
    if let Some(query) = url.query() {
        let retained = query
            .split('&')
            .filter(|pair| {
                let key = pair.split('=').next().unwrap_or("");
                !crate::url::form_urlencoded::parse(key.as_bytes())
                    .next()
                    .is_some_and(|(k, _)| k.starts_with("utm_"))
            })
            .collect::<Vec<_>>()
            .join("&");
        url.set_query(if retained.is_empty() {
            None
        } else {
            Some(&retained)
        });
    }
    Ok(url.to_string())
}
