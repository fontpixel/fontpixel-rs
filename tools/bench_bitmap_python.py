#!/usr/bin/env python3
"""Matching Python bitmap operations, five wall-clock batches; excludes input setup."""
import argparse
import json
from pathlib import Path
import statistics
import sys
from time import perf_counter_ns
sys.dont_write_bytecode = True
root=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--reference', type=Path, default=root.parent/'reference'/'bdfparser')
args=p.parse_args()
sys.path.insert(0,str(args.reference/'src'))
from bdfparser import Bitmap

def pattern(w,h):
    return Bitmap([''.join('1' if (x*13+y*7)%11<4 else '0' for x in range(w)) for y in range(h)])
def measure(name,count,action):
    for _ in range(min(count,10)): action()
    samples=[]
    for _ in range(5):
        start=perf_counter_ns()
        for _ in range(count): action()
        samples.append((perf_counter_ns()-start)/count)
    return dict(name=name,iterations_per_batch=count,samples_ns=samples,median_ns=statistics.median(samples))
small=pattern(64,64)
glyph=pattern(14,14)
canvas=Bitmap.blank(1120,350)
def screen():
    for y in range(25):
        for x in range(80):
            canvas.paste(glyph,x*14,y*14)
result=[
    measure('crop_64x64_to_31x31',10000,lambda:small.clone().crop(31,31,3,5)),
    measure('concat_two_64x64',1000,lambda:Bitmap.concatall([small,small])),
    measure('concat_80_glyphs',100,lambda:Bitmap.concatall([glyph]*80)),
    measure('paste_screen_80x25',3,screen),
    measure('bold_64x64_radius8',100,lambda:small.clone().bold(8)),
    measure('rotate_64x64',1000,lambda:small.clone().rotate(90)),
]
print(json.dumps({'python':sys.version,'results':result},indent=2))
