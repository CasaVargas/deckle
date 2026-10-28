#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Run the isolated, fake-source worker exchange and validate its actual output offline."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

from jsonschema import Draft202012Validator, FormatChecker
from referencing import Registry

ROOT = Path(__file__).resolve().parent.parent


def validate(output):
    def read(name):
        return json.loads((output / name).read_text())

    press = Draft202012Validator(json.loads((ROOT / 'docs/formats/deckle-press.schema.json').read_text()),
                                format_checker=FormatChecker(), registry=Registry())
    for name in ['manifest.json', 'request.json', 'request-second.json', 'status-queued.json',
                 'status-building.json', 'status-published.json', 'status-second.json']:
        assert (output / name).stat().st_size <= 16_384, name
        press.validate(read(name))
    request, second = read('request.json'), read('request-second.json')
    manifest = read('manifest.json')
    assert request['reader_id'] != second['reader_id']
    assert request['request_id'] != second['request_id']
    statuses = [read(f'status-{state}.json') for state in ['queued', 'building', 'published']]
    for status, state in zip(statuses, ['queued', 'building', 'published']):
        assert status['state'] == state
        assert status['request_id'] == request['request_id']
        assert status['paper_id'] == request['paper_id'] == manifest['paper_id']
        assert status['producer_id'] == request['producer_id'] == manifest['producer_id']
    assert statuses[0]['revision'] < statuses[1]['revision'] < statuses[2]['revision']
    published = statuses[2]
    assert read('status-second.json')['request_id'] == second['request_id']
    assert read('status-second.json')['result'] == published['result']
    assert read('status-second.json')['job_id'] == published['job_id']
    result = published['result']
    assert result['bundle'] == f".deckle-press/editions/{result['edition_id']}.deckle"
    bundle = output / result['bundle']
    manifest_bytes = (bundle / 'edition.json').read_bytes()
    assert hashlib.sha256(manifest_bytes).hexdigest() == result['manifest_sha256']
    edition = json.loads(manifest_bytes)
    Draft202012Validator(json.loads((ROOT / 'docs/formats/edition-bundle.schema.json').read_text()),
                         format_checker=FormatChecker(), registry=Registry()).validate(edition)
    assert edition['edition']['id'] == result['edition_id']
    assert edition['edition']['paper_id'] == request['paper_id']
    assert edition['edition']['kind'] == 'fresh'
    assert edition['files'], 'exercise actual content, not an empty manifest'
    for asset in edition['files']:
        path = bundle / asset['path']
        assert path.resolve().is_relative_to(bundle.resolve()), asset['path']
        data = path.read_bytes()
        assert len(data) == asset['size'], asset['path']
        assert hashlib.sha256(data).hexdigest() == asset['sha256'], asset['path']
    for name in ['scheduled', 'marks']:
        assert (output / f'{name}-before.json').read_bytes() == (output / f'{name}-after.json').read_bytes()
    print(f"Validated real worker exchange: 2 requests, 3 phases, 1 coalesced result, {len(edition['files'])} hashed assets; scheduled bundle/marks unchanged")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, help='New directory for exchange output (must not exist)')
    parser.add_argument('--target-dir', type=Path, help='Isolated Cargo target directory; defaults to a new temporary directory')
    parser.add_argument('--validate-only', type=Path, help='Validate an existing exchange without running Cargo')
    args = parser.parse_args()
    if args.validate_only:
        validate(args.validate_only.resolve())
        return
    temporary = Path(tempfile.mkdtemp(prefix='deckle-press-smoke-'))
    output = (args.output or temporary / 'exchange').resolve()
    if output.exists():
        parser.error(f'output already exists: {output}')
    output.parent.mkdir(parents=True, exist_ok=True)
    target = (args.target_dir or temporary / 'target').resolve()
    env = os.environ.copy()
    env['CARGO_TARGET_DIR'] = str(target)
    env['DECKLE_PRESS_SMOKE_OUTPUT'] = str(output)
    print(f'Checkout: {ROOT}\nCargo target: {target}\nExchange: {output}', flush=True)
    subprocess.run(['cargo', 'test', '-p', 'deckle-cli', '--test', 'it',
                    'press_worker::press_worker_protocol_smoke_exchange', '--', '--exact'],
                   cwd=ROOT, env=env, check=True, timeout=900)
    validate(output)


if __name__ == '__main__':
    main()
