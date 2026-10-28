// SPDX-License-Identifier: GPL-3.0-or-later
use chrono::{DateTime, Duration, Utc};
use deckle_digest::press::{
    PressBinding, PressErrorCode, decode_manifest, decode_request, decode_status,
};
use serde_json::{Value, json};
use std::path::PathBuf;

const REQUEST: &str = "78c08b13-6bed-46c2-aab7-73376423bc9d";
const READER: &str = "37824cbc-561b-4a90-8823-33f83bc0c1d6";
const PRODUCER: &str = "ba9e81c1-d5e3-43cb-bca1-acaf771caf79";
const PAPER: &str = "ea249ae5-771c-4d2b-9f11-5119fbe3feda";
const EDITION: &str = "1429eb21-019d-4da9-815d-f710561e08a8";
fn now() -> DateTime<Utc> {
    "2026-10-27T16:00:00Z".parse().unwrap()
}
fn binding() -> PressBinding {
    PressBinding {
        root: PathBuf::from("unused-local-root"),
        producer_id: PRODUCER.parse().unwrap(),
        paper_id: PAPER.parse().unwrap(),
    }
}
fn path() -> PathBuf {
    PathBuf::from(".deckle-press")
        .join("requests")
        .join(READER)
        .join(format!("{REQUEST}.json"))
}
fn request() -> Value {
    json!({"format":"deckle-press-request","version":1,"request_id":REQUEST,"reader_id":READER,"producer_id":PRODUCER,"paper_id":PAPER,"operation":"fresh_full","created_at":"2026-10-27T16:00:00Z","expires_at":"2026-10-28T16:00:00Z"})
}
fn bytes(v: &Value) -> Vec<u8> {
    serde_json::to_vec(v).unwrap()
}

#[test]
fn press_protocol_accepts_exact_limit_and_unknown_fields() {
    let mut v = request();
    v["future_optional"] = json!({"opaque":true});
    let mut b = bytes(&v);
    b.resize(16_384, b' ');
    let r = decode_request(&b, &path(), &binding(), now()).unwrap();
    assert_eq!(r.expires_at - r.created_at, Duration::hours(24));
    assert_eq!(
        decode_request(&vec![b' '; 16_385], &path(), &binding(), now())
            .unwrap_err()
            .code,
        PressErrorCode::Unsupported
    );
}
#[test]
fn press_protocol_rejects_untrusted_routing_and_versions() {
    for (key, value) in [
        ("version", json!(2)),
        ("request_id", json!("invalid")),
        ("paper_id", json!(PRODUCER)),
        ("producer_id", json!(PAPER)),
        ("operation", json!("run_shell")),
        ("format", json!("other")),
    ] {
        let mut v = request();
        v[key] = value;
        assert!(
            decode_request(&bytes(&v), &path(), &binding(), now()).is_err(),
            "{key}"
        );
    }
    for p in [
        format!(".deckle-press/requests/{READER}/wrong.json"),
        format!(".deckle-press/requests/{PRODUCER}/{REQUEST}.json"),
        format!("/.deckle-press/requests/{READER}/{REQUEST}.json"),
        format!(".deckle-press/requests/{READER}/../{REQUEST}.json"),
        format!(".deckle-press/requests/{READER}/./{REQUEST}.json"),
        format!(".deckle-press/requests/{READER}/{REQUEST} (conflict).json"),
    ] {
        assert!(
            decode_request(&bytes(&request()), &PathBuf::from(&p), &binding(), now()).is_err(),
            "{p}"
        );
    }
    assert!(decode_request(b"{", &path(), &binding(), now()).is_err());
}
#[test]
fn press_protocol_enforces_clock_boundaries_and_fixed_ttl() {
    let mut v = request();
    v["expires_at"] = json!("2026-10-28T15:59:59Z");
    assert_eq!(
        decode_request(&bytes(&v), &path(), &binding(), now())
            .unwrap_err()
            .code,
        PressErrorCode::ClockMismatch
    );
    assert!(
        decode_request(
            &bytes(&request()),
            &path(),
            &binding(),
            now() - Duration::minutes(5)
        )
        .is_ok()
    );
    assert_eq!(
        decode_request(
            &bytes(&request()),
            &path(),
            &binding(),
            now() - Duration::minutes(5) - Duration::seconds(1)
        )
        .unwrap_err()
        .code,
        PressErrorCode::ClockMismatch
    );
    assert_eq!(
        decode_request(
            &bytes(&request()),
            &path(),
            &binding(),
            now() + Duration::hours(24)
        )
        .unwrap_err()
        .code,
        PressErrorCode::Expired
    );
}
fn status() -> Value {
    json!({"format":"deckle-press-status","version":1,"request_id":REQUEST,"producer_id":PRODUCER,"paper_id":PAPER,"revision":1,"state":"published","updated_at":"2026-10-27T16:01:00Z","result":{"edition_id":EDITION,"bundle":format!(".deckle-press/editions/{EDITION}.deckle"),"manifest_sha256":"a".repeat(64)}})
}
#[test]
fn press_protocol_status_binds_result_path_hash_and_state() {
    let p = PathBuf::from(format!(".deckle-press/status/{REQUEST}.json"));
    assert!(decode_status(&bytes(&status()), &p, &binding()).is_ok());
    for (key, value) in [
        ("bundle", json!(format!("other/{EDITION}.deckle"))),
        ("edition_id", json!(PAPER)),
        ("manifest_sha256", json!("bad")),
    ] {
        let mut v = status();
        v["result"][key] = value;
        assert!(decode_status(&bytes(&v), &p, &binding()).is_err(), "{key}");
    }
    for state in ["queued", "building", "failed", "expired"] {
        let mut v = status();
        v["state"] = json!(state);
        assert!(
            decode_status(&bytes(&v), &p, &binding()).is_err(),
            "unexpected result: {state}"
        );
    }
    for state in ["published", "result_removed"] {
        let mut v = status();
        v["state"] = json!(state);
        v.as_object_mut().unwrap().remove("result");
        assert!(decode_status(&bytes(&v), &p, &binding()).is_err());
    }
    let mut v = status();
    v["state"] = json!("result_removed");
    assert!(decode_status(&bytes(&v), &p, &binding()).is_err());
    v["removed_at"] = json!("2026-10-30T16:00:00Z");
    v["message"] = json!("Fresh edition is no longer available; print another");
    assert!(decode_status(&bytes(&v), &p, &binding()).is_ok());
    v["paper_id"] = json!(PRODUCER);
    assert!(decode_status(&bytes(&v), &p, &binding()).is_err());
}
#[test]
fn press_protocol_manifest_is_bounded_and_bound() {
    let mut v = json!({"format":"deckle-press","version":1,"producer_id":PRODUCER,"paper_id":PAPER,"paper_name":"Daily","enabled":true,"operations":["fresh_full"],"updated_at":"2026-10-27T16:00:00Z"});
    assert!(decode_manifest(&bytes(&v), &binding()).is_ok());
    v["version"] = json!(2);
    assert!(decode_manifest(&bytes(&v), &binding()).is_err());
    v["version"] = json!(1);
    v["producer_id"] = json!(PAPER);
    assert!(decode_manifest(&bytes(&v), &binding()).is_err());
    assert!(decode_manifest(&vec![b' '; 16_385], &binding()).is_err());
}

#[test]
fn press_protocol_schemas_preserve_legacy_and_require_fresh_identity() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cases: Value =
        serde_json::from_slice(&std::fs::read(root.join("fixtures/press/cases.json")).unwrap())
            .unwrap();
    for case in cases.as_array().unwrap() {
        let schema: Value = serde_json::from_slice(
            &std::fs::read(root.join(case["schema"].as_str().unwrap())).unwrap(),
        )
        .unwrap();
        let mut schema = schema;
        if let Some(def) = case["definition"].as_str() {
            schema["$ref"] = json!(format!("#/$defs/{def}"));
        }
        let validator = jsonschema::draft202012::options()
            .should_validate_formats(true)
            .build(&schema)
            .unwrap();
        let value: Value = serde_json::from_slice(
            &std::fs::read(
                root.join("fixtures/press")
                    .join(case["file"].as_str().unwrap()),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            validator.is_valid(&value),
            case["valid"].as_bool().unwrap(),
            "{}",
            case["file"]
        );
    }
}

#[test]
fn press_protocol_optional_fields_must_have_their_declared_types() {
    let p = PathBuf::from(format!(".deckle-press/status/{REQUEST}.json"));
    let mut v = status();
    v["state"] = json!("queued");
    v["result"] = Value::Null;
    assert!(
        decode_status(&bytes(&v), &p, &binding()).is_err(),
        "a present result is forbidden for queued status even when null"
    );
    v.as_object_mut().unwrap().remove("result");
    v["job_id"] = Value::Null;
    assert!(
        decode_status(&bytes(&v), &p, &binding()).is_err(),
        "present optional fields must have the declared type"
    );
}

#[test]
fn press_protocol_rejects_noncanonical_uuid_spellings() {
    fn spellings(id: &str) -> [String; 4] {
        [
            id.to_uppercase(),
            id.replace('-', ""),
            format!("{{{id}}}"),
            format!("urn:uuid:{id}"),
        ]
    }
    let status_path = PathBuf::from(format!(".deckle-press/status/{REQUEST}.json"));
    for key in ["request_id", "reader_id", "producer_id", "paper_id"] {
        for spelling in spellings(request()[key].as_str().unwrap()) {
            let mut candidate = request();
            candidate[key] = json!(spelling);
            assert!(
                decode_request(&bytes(&candidate), &path(), &binding(), now()).is_err(),
                "request {key}: {spelling}"
            );
        }
    }
    for key in ["producer_id", "paper_id"] {
        let manifest: Value =
            serde_json::from_str(include_str!("../../../../fixtures/press/manifest.json")).unwrap();
        for spelling in spellings(manifest[key].as_str().unwrap()) {
            let mut candidate = manifest.clone();
            candidate[key] = json!(spelling);
            assert!(
                decode_manifest(&bytes(&candidate), &binding()).is_err(),
                "manifest {key}: {spelling}"
            );
        }
    }
    for (key, id) in [
        ("request_id", REQUEST),
        ("producer_id", PRODUCER),
        ("paper_id", PAPER),
        ("job_id", PRODUCER),
    ] {
        for spelling in spellings(id) {
            let mut candidate = status();
            candidate[key] = json!(spelling);
            assert!(
                decode_status(&bytes(&candidate), &status_path, &binding()).is_err(),
                "status {key}: {spelling}"
            );
        }
    }
    for spelling in spellings(EDITION) {
        let mut candidate = status();
        candidate["result"]["edition_id"] = json!(spelling);
        assert!(
            decode_status(&bytes(&candidate), &status_path, &binding()).is_err(),
            "result: {spelling}"
        );
    }
}
