#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Validate portable press fixtures, offline, including semantic contract invariants.

Install tools/requirements-press.txt in a venv first. The fixture matrix lists
schema-only cases; semantic mutation checks below cover relationships JSON Schema
cannot express. Schema resolution never accesses the network.
"""
import datetime as dt
import json
from pathlib import Path
import sys

from jsonschema import Draft202012Validator, FormatChecker
from referencing import Registry

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "fixtures/press"


def timestamp(value):
    return dt.datetime.fromisoformat(value.replace("Z", "+00:00"))


def semantic_errors(value):
    """Extra invariants after schema validation, independent of local clock/binding."""
    if value["format"] == "deckle-press-request":
        if timestamp(value["expires_at"]) - timestamp(value["created_at"]) != dt.timedelta(hours=24):
            return ["request TTL must be exactly 24 hours"]
    if value["format"] == "deckle-press-status" and "result" in value:
        result = value["result"]
        if result["bundle"] != f'.deckle-press/editions/{result["edition_id"]}.deckle':
            return ["result path disagrees with edition identity"]
    return []


def reject_remote(uri):
    raise ValueError(f"Remote schema resolution forbidden: {uri}")


def main():
    assert "date-time" in FormatChecker.checkers, "Install the pinned requirements for timestamp checks"
    cases = json.loads((FIXTURES / "cases.json").read_text())
    for case in cases:
        schema = json.loads((ROOT / case["schema"]).read_text())
        # Select the public definition directly; removing oneOf means errors
        # are attributed to this control's exact shape.
        if "definition" in case:
            schema.pop("oneOf")
            schema["$ref"] = f'#/$defs/{case["definition"]}'
        Draft202012Validator.check_schema(schema)
        validator = Draft202012Validator(schema, format_checker=FormatChecker(), registry=Registry(retrieve=reject_remote))
        value = json.loads((FIXTURES / case["file"]).read_text())
        errors = [e.message for e in validator.iter_errors(value)]
        if not errors:
            errors = semantic_errors(value)
        if (not errors) != case["valid"]:
            raise AssertionError(f'{case["file"]}: expected valid={case["valid"]}; {errors}')
    request = json.loads((FIXTURES / "request.json").read_text())
    request["expires_at"] = "2026-10-28T15:59:59Z"
    assert semantic_errors(request), "arbitrary request lifetime accepted"
    status = json.loads((FIXTURES / "status-published.json").read_text())
    status["result"]["edition_id"] = status["paper_id"]
    assert semantic_errors(status), "mismatched result ID accepted"
    print(f"Validated {len(cases)} schema fixtures and 2 semantic rejection cases (offline)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
