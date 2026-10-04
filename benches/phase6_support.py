#!/usr/bin/env python3
"""Shared bounded orchestration helpers. All child diagnostic bytes are retained."""
import hashlib
import codecs
import json
import os
from pathlib import Path
import subprocess


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def batch_header(path, expected_tasks, limit=65536):
    """Decode only small top-level fields before the potentially huge tasks list.

    This is not validation of the whole report. The complete stdout is separately
    retained and hashed; process exit and this transaction summary gate success.
    """
    with Path(path).open('rb') as source:
        text = codecs.getincrementaldecoder('utf-8')('strict').decode(source.read(limit), final=False)
    decoder = json.JSONDecoder(); pos = 0; header = {}
    def skip(index):
        while index < len(text) and text[index].isspace(): index += 1
        return index
    pos = skip(pos)
    if pos >= len(text) or text[pos] != '{': raise ValueError('batch report is not an object')
    pos += 1
    while True:
        pos = skip(pos)
        key, pos = decoder.raw_decode(text, pos)
        if not isinstance(key, str) or key in header: raise ValueError('invalid/duplicate batch metadata key')
        pos = skip(pos)
        if pos >= len(text) or text[pos] != ':': raise ValueError('invalid batch metadata separator')
        pos = skip(pos + 1)
        if key == 'tasks':
            if pos >= len(text) or text[pos] != '[': raise ValueError('batch tasks list missing')
            break
        value, pos = decoder.raw_decode(text, pos); header[key] = value
        pos = skip(pos)
        if pos >= len(text) or text[pos] != ',': raise ValueError('batch header truncated or tasks missing')
        pos += 1
    expected = dict(total=expected_tasks, succeeded=expected_tasks, failed=0, committed=expected_tasks)
    if header.get('version') != 1 or header.get('success') is not True or header.get('exit_code') != 0 or header.get('summary') != expected:
        raise ValueError({'unsuccessful_batch': header, 'expected': expected})
    return header


def invoke(command, stem, *, env=None):
    stdout, stderr = Path(str(stem) + '.stdout'), Path(str(stem) + '.stderr')
    for path in [stdout, stderr]:
        if os.path.lexists(path): raise FileExistsError(path)
    stdout.parent.mkdir(parents=True, exist_ok=True)
    with stdout.open('xb') as out, stderr.open('xb') as err:
        process = subprocess.run(list(map(str, command)), stdout=out, stderr=err, env=env)
    record = {'command': list(map(str, command)), 'exit': process.returncode,
              'stdout': str(stdout), 'stderr': str(stderr)}
    if process.returncode: raise RuntimeError(record)
    return record


def inventory(root):
    return {str(p.relative_to(root)): {'bytes': p.stat().st_size, 'sha256': digest(p)}
            for p in sorted(Path(root).rglob('*')) if p.is_file()}
