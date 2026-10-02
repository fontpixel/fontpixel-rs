#!/usr/bin/env python3
"""Regenerate small self-contained MIT reference goldens; never writes reference/.

Requires the supplied reference trees or BDFPARSER_PY/BDFPARSER_JS overrides.
The checked-in Rust tests run without Python or reference trees.
"""
import ast
import hashlib
import json
import os
from pathlib import Path
import sys
sys.dont_write_bytecode = True
import warnings

ROOT = Path(__file__).resolve().parents[1]
PYSRC = Path(os.environ.get('BDFPARSER_PY', ROOT.parent / 'reference/bdfparser/src'))
sys.path.insert(0, str(PYSRC))
sys.path.insert(0, str(ROOT / 'tests/parity'))
from bdfparser import Font
import gen_cases
from gen_cases import Codes, random_case
gen_cases.AVOID.add("ragged")
from probes import cases as probe_cases
import run_py
warnings.simplefilter('ignore')
OUT = ROOT / 'tests/data/compat'
OUT.mkdir(exist_ok=True)

def constant(path, name):
    tree = ast.parse(path.read_text())
    for node in tree.body:
        if isinstance(node, ast.Assign) and any(isinstance(t, ast.Name) and t.id == name for t in node.targets):
            return ast.literal_eval(node.value)
    raise RuntimeError(name)

mini = constant(PYSRC.parent / 'tests/info.py', 'mini_bdf')
(OUT / 'mini.bdf').write_text(mini)
(OUT / 'mini.svg').write_text(constant(PYSRC.parent / 'tests/test_svg.py', 'EXPECTED_MINI_SVGFONT'))
font = Font().load_string(mini)
manifest = []
for shape in ['square', 'circle']:
    for dotsize in [1.0, 0.7]:
        for metadata in [True, False]:
            opts = dict(shape=shape, dotsize=dotsize, metadata=metadata, ppu=64, timestamp=0)
            name = f'mini-{shape}-{dotsize}-{int(metadata)}'
            data = font.tottf(**opts)
            (OUT / (name + '.ttf')).write_bytes(data)
            rt = Font().load_bytes(data, pixelsize=8).tobdf()
            (OUT / (name + '.bdf')).write_text(rt)
            manifest.append(dict(name=name, options=opts, sha256=hashlib.sha256(data).hexdigest()))
(OUT / 'ttf.json').write_text(json.dumps(manifest, indent=2) + '\n')
# Synthetic probes and random cases preserve the original operation scripts.
# K12 is invalid ragged input, tested separately with the Rust invariant test.
cases = [c for c in probe_cases() if c['id'] != 'K12-ragged-bitmap']
cases += [random_case(i, 1, Codes(None)) for i in range(64)]
with (OUT / 'cases.jsonl').open('w') as f, (OUT / 'expected.jsonl').open('w') as g:
    for c in cases:
        f.write(json.dumps(c) + '\n')
        g.write(json.dumps(dict(id=c['id'], res=run_py.run_case(c))) + '\n')
files = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(OUT.iterdir()) if p.name != 'SOURCES.json'}
(OUT / 'SOURCES.json').write_text(json.dumps({'license':'MIT, Tom Chen; LICENSES/bdfparser-MIT.txt', 'source':'Supplied bdfparser tests/info.py mini_bdf; tests/test_svg.py EXPECTED_MINI_SVGFONT; Python oracle on the migrated parity synthetic generators; no external font artwork.', 'files':files},indent=2)+'\n')
