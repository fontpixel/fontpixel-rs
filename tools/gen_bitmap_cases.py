#!/usr/bin/env python3
"""Deterministic bitmap-only SPEC-v3 oracle. Never writes into the reference tree."""
import argparse
import hashlib
import json
from pathlib import Path
import random
import sys
import warnings

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--reference', type=Path, default=ROOT.parent/'reference'/'bdfparser')
parser.add_argument('--check', action='store_true')
args = parser.parse_args()
sys.path.insert(0, str(args.reference/'src'))
from bdfparser import Bitmap
warnings.simplefilter('ignore')
rng = random.Random(0xBDF301)
cases = []

def record(rows, op, values=(), **extra):
    bm = Bitmap(list(rows))
    case = dict(id=len(cases), rows=rows, op=op, args=list(values), **extra)
    if op in ('paste','overlay'):
        result = getattr(bm, op)(Bitmap(extra['other']), *values)
    elif op == 'concatall':
        result = Bitmap.concatall([Bitmap(r) for r in extra['bitmaps']], *values, extra['gaps'])
    elif op == 'replace':
        result = bm.replace(extra['old'], extra['new'])
        if result.bindata and any(len(r) != len(result.bindata[0]) for r in result.bindata):
            return  # Core rejects ragged output, tested separately.
    else:
        result = getattr(bm, op)(*values)
    case['expected'] = result.bindata
    case['bbox'] = result.bbox()
    cases.append(case)

def rows(w=None,h=None):
    w=rng.randrange(0,34) if w is None else w
    h=rng.randrange(0,12) if h is None else h
    return [''.join(rng.choice('000001122') for _ in range(w)) for _ in range(h)]

for i in range(160):
    source=rows()
    for op in ('fliph','flipv','invert','trim'):
        record(source,op)
    record(source,'crop',(rng.randrange(35),rng.randrange(15),rng.randrange(-40,40),rng.randrange(-15,15)))
    record(source,'shift',(rng.randrange(-40,40),rng.randrange(-15,15)))
    record(source,'rotate',(rng.choice([-720,-270,-180,-90,0,90,180,270,720]),))
    record(source,'bold',(rng.randrange(15),))
    record(source,'italic',(rng.randrange(1,7),rng.randrange(-15,15)))
    record(source,'shadow',(rng.randrange(-5,6),rng.randrange(-5,6)))
    record(source,'glow',(rng.randrange(2),))
    record(source,'bytepad',(rng.choice([1,2,3,4,8,16]),))
    record(source,'enlarge',(rng.randrange(-2,5),rng.randrange(-2,5)))
    record(source,'paste',(rng.randrange(-40,40),rng.randrange(-15,15)),other=rows())
    record(source,'overlay',other=rows())
    record(source,'replace',old=rng.choice(['0','1','2','12','']),new=rng.choice(['0','1','2','12','']))
    group=[rows() for _ in range(rng.randrange(0,6))]
    gaps=[rng.randrange(-40,12) for _ in range(max(0,len(group)-1))]
    for direction in [1,2,0,-1]:
        for align in [0,1]:
            record([], 'concatall',(direction,align),bitmaps=group,gaps=gaps)

# Simple hand-inspectable asymmetric label cases and byte-boundary widths.
for w in [1,7,8,9,15,16,17,31,32,33,63,64,65,127,128,129]:
    source=rows(w,3)
    record(source,'shadow',(-3,2))
    record(source,'glow',(1,))
    record(source,'bold',(9,))

source=args.reference/'src'/'bdfparser'/'bitmap.py'
doc={'generator':'tools/gen_bitmap_cases.py','seed':0xBDF301,'reference_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'cases':cases}
serialized=json.dumps(doc,ensure_ascii=False,separators=(',',':'))+'\n'
output=ROOT/'crates'/'fontpixel'/'tests'/'data'/'bitmap_reference.json'
if args.check:
    if output.read_text() != serialized:
        raise SystemExit('bitmap reference fixture differs')
else:
    output.write_text(serialized)
print(f'{len(cases)} reference cases verified; {len(serialized.encode())} bytes')
