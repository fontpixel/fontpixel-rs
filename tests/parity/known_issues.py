"""
Known (analysed) Python <-> Rust divergences.

K11 is deliberately not exempt in Rust: numeric rows retain arbitrary precision.

compare.py labels every divergence with the id of the first matching entry
(or NEW). Matchers see the operation (dict), the case, and both raw results
(`py`, `js`: canonical result text or error message).

Keep entries specific: a matcher should only match the documented root cause.
See README.md ("Findings") for the minimal reproducers and spec references.

Findings K01-K10 and K13-K16 of the first harness run were fixed by SPEC-v3
§11 (both libraries); their matchers were removed so that any recurrence is
reported as NEW (probes.py keeps a regression probe for each). What remains
are the two permanent language differences of SPEC-v3 §11.
"""

from __future__ import annotations

import re


def _err(x):
    return isinstance(x, str) and re.match(r'^[A-Za-z]*(Error|Exception)\b', x) is not None


def _has(case, t):
    return t in (case.get('tags') or [])


KNOWN = [
    # id, short description, matcher(op, case, py, js, pyerr, jserr)
    (
        'K12-ragged-bitmap',
        'Operations on "ragged" bitmaps (rows of different lengths, e.g. after replace("10", "2"); invalid input, '
        'SPEC-v3 §1.3): Python permits ragged intermediate lists; Rust rejects the invalid input transactionally',
        lambda op, c, py, js, pe, je: (
            op.get('op') == 'bset' and je and not pe
            and 'rectangular' in js and len({len(r) for r in op.get('rows', [])}) > 1
        ) or (_has(c, 'ragged') and je and ('ragged' in js or 'rectangular' in js)
              and op.get('op') in ('bop', 'htransform', 'transform')),
    ),
]


def label(op, case, py, js, pyerr, jserr, ctx=None):
    for kid, _desc, fn in KNOWN:
        try:
            if fn(op or {}, case, py or '', js or '', pyerr, jserr):
                return kid
        except Exception:  # noqa: BLE001
            continue
    return 'NEW'


DESCRIPTIONS = {kid: desc for kid, desc, _ in KNOWN}
