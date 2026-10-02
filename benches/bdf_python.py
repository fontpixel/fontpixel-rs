#!/usr/bin/env python3
"""Five BDF wall-clock reference samples, same resident-input/output contract."""
import gc,json,statistics,sys,time,warnings
from pathlib import Path
sys.dont_write_bytecode=True
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT.parent/"reference/bdfparser/src"))
from bdfparser import Font
warnings.simplefilter("ignore")
path=Path(sys.argv[1]);data=path.read_bytes();font=Font(data);font.tobdf()
reads=[];writes=[]
for _ in range(5):
    gc.collect();start=time.perf_counter();parsed=Font(data);reads.append((time.perf_counter()-start)*1000);del parsed
for _ in range(5):
    gc.collect();start=time.perf_counter();written=font.tobdf().encode();writes.append((time.perf_counter()-start)*1000);del written
print(json.dumps(dict(path=str(path),python=sys.version,input_bytes=len(data),glyphs=len(font.glyphs)+len(font.unencoded),read_ms=reads,write_ms=writes)))
