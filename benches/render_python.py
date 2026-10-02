#!/usr/bin/env python3
"""Same resident font, inputs, warmups and five single calls as render_timing.rs."""
import json,sys,time,warnings
from pathlib import Path
sys.dont_write_bytecode=True
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'reference/bdfparser/src'))
from bdfparser import Font
warnings.simplefilter('ignore')
f=Font(sys.argv[1]);text='永'*2000;opts=dict(linelimit=80*f.headers['fbbx'],mode=0)
screen=f.draw(text,**opts);all_=f.drawall()
def sample(fn):
    out=[]
    for _ in range(5):
        start=time.perf_counter();value=fn();out.append((time.perf_counter()-start)*1000)
        del value
    return out
print(json.dumps(dict(python=sys.version,path=sys.argv[1],glyphs=len(f),screen_characters=2000,screen_width=screen.width(),screen_height=screen.height(),drawall_width=all_.width(),drawall_height=all_.height(),screen_ms=sample(lambda:f.draw(text,**opts)),drawall_ms=sample(f.drawall))))
