#!/usr/bin/env python3
"""Five single-threaded resident-input PCF calls; the reference tree is read-only."""
import gc,json,sys,time,warnings,hashlib
from pathlib import Path
sys.dont_write_bytecode=True
ROOT=Path(__file__).resolve().parents[1];sys.path.insert(0,str(ROOT.parent/'reference/bdfparser/src'))
from bdfparser import Font
from bdfparser.pcf import write_pcf,read_pcf
warnings.simplefilter('ignore');path=Path(sys.argv[1]);data=path.read_bytes();font=Font().load_bytes(data);wire=write_pcf(font)
def sample(fn):
    result=[]
    for _ in range(5):
        gc.collect();start=time.perf_counter();value=fn();result.append((time.perf_counter()-start)*1000);del value
    return result
print(json.dumps(dict(path=str(path),python=sys.version,glyphs=len(font.glyphs)+len(font.unencoded),input_bytes=len(data),pcf_bytes=len(wire),sha256=hashlib.sha256(wire).hexdigest(),bdf_parse_ms=sample(lambda:Font().load_bytes(data)),pcf_write_ms=sample(lambda:write_pcf(font)),pcf_read_ms=sample(lambda:read_pcf(wire)),bdf_to_pcf_ms=sample(lambda:write_pcf(Font().load_bytes(data))))))
