#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Offline schema and semantic verification for compatible-client state v1."""
import copy
import hashlib
import json
from pathlib import Path
from jsonschema import Draft202012Validator, FormatChecker
from referencing import Registry

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / ("fixtures" if (ROOT / "fixtures").exists() else "Fixtures") / "state"
MAX = 9007199254740991


def semantic_errors(snapshot):
    errors = []
    seen_keys, seen_dots = set(), set()
    for operation in snapshot["registers"]:
        key, value, dot, context = (operation[x] for x in ("key", "value", "dot", "context"))
        logical_key = json.dumps(key, sort_keys=True)
        causal_dot = (dot["writer_id"], dot["counter"])
        if logical_key in seen_keys or causal_dot in seen_dots:
            errors.append("duplicate key or dot")
        seen_keys.add(logical_key)
        seen_dots.add(causal_dot)
        if dot["writer_id"] != snapshot["writer_id"] or context.get(dot["writer_id"], 0) >= dot["counter"]:
            errors.append("invalid owner context")
        if value is None:
            continue
        expected = {"last_location": "anchor", "story_position": "anchor", "mark": "mark", "crossword_cell": "cell", "ladder": "ladder"}[key["kind"]]
        if value["kind"] != expected:
            errors.append("key/value mismatch")
            continue
        if expected == "anchor":
            anchor = value["anchor"]
            if key["kind"] == "story_position" and key["story_id"] != anchor["story_id"]:
                errors.append("story mismatch")
            texts = [anchor["quote"]]
        elif expected == "mark":
            mark = value["mark"]
            if mark["id"] != key["id"]:
                errors.append("mark mismatch")
            selection = mark["range"]
            if selection and selection["location"] + selection["length"] > MAX:
                errors.append("range overflow")
            texts = [mark["quote"], mark["note"]]
        else:
            texts = []
        if any(len(text.encode("utf-8")) > 16384 for text in texts):
            errors.append("text byte bound")
    return errors


def validate(data, validator):
    if len(data) > 2097152:
        return ["snapshot byte bound"]
    try:
        value = json.loads(data)
    except (ValueError, UnicodeError):
        return ["invalid JSON"]
    errors = [error.message for error in validator.iter_errors(value)]
    # Python/jsonschema considers integral floats integers; v1 requires integer tokens.
    def integral_tokens(item):
        if isinstance(item, float):
            return False
        if isinstance(item, dict):
            return all(integral_tokens(v) for v in item.values())
        if isinstance(item, list):
            return all(integral_tokens(v) for v in item)
        return True
    if not integral_tokens(value):
        errors.append("noninteger numeric token")
    return errors or semantic_errors(value)


def no_remote(uri):
    raise ValueError("Remote schema resolution forbidden: " + uri)


def main():
    schema = json.loads((ROOT / "docs/formats/deckle-state.schema.json").read_text())
    Draft202012Validator.check_schema(schema)
    validator = Draft202012Validator(schema, format_checker=FormatChecker(), registry=Registry(retrieve=no_remote))
    cases = json.loads((FIXTURES / "cases.json").read_text())
    for body_case in json.loads((FIXTURES / "body-hashes.json").read_text()):
        assert hashlib.sha256(body_case["body"]["text"].encode("utf-8")).hexdigest() == body_case["sha256"]
    for case in cases:
        errors = validate((FIXTURES / case["file"]).read_bytes(), validator)
        assert bool(errors) != case["valid"], (case, errors)
    for case in json.loads((FIXTURES / "subjects.json").read_text()):
        subject = case["subject"]
        identity = [subject["kind"]] + ([subject["paper_id"], subject["edition_id"]] if subject["kind"] == "fresh" else [subject["date"]]) + [subject["manifest_sha256"]]
        canonical = json.dumps(identity, separators=(",", ":"))
        assert canonical == case["canonical_utf8"]
        assert hashlib.sha256(canonical.encode()).hexdigest() == case["key"]
    assert validate(b" " * 2097153, validator)
    original = json.loads((FIXTURES / "snapshot.json").read_text())
    for field, value in [("version", True), ("version", 1.0), ("extra", 1)]:
        mutation = copy.deepcopy(original)
        mutation[field] = value
        assert validate(json.dumps(mutation).encode(), validator)
    print(f"Validated {len(cases)} state fixtures, 2 canonical subjects, 4 boundary mutations (offline)")


if __name__ == "__main__":
    main()
