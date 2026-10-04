#!/usr/bin/env python3
"""Build an external oracle with byte-identical official Rockbox sources.

Supply the downloaded font.c/font.h via --source-dir. Only original adapters and
platform stubs live in this repository; no Rockbox implementation is vendored.
"""
import argparse, hashlib, json, shutil, subprocess
from pathlib import Path
ap=argparse.ArgumentParser(description=__doc__)
ap.add_argument('--source-dir',type=Path,required=True)
ap.add_argument('--output-dir',type=Path,required=True)
a=ap.parse_args();out=a.output_dir.resolve();out.mkdir(parents=True,exist_ok=True)
for name,digest in [('font.c','56c0591ac7646be826ea5e178e830a6319b4adc5aa8a68fa3a2f5a5009727097'),('font.h','7fee3ef7921b68f77b14df4d657afb94f619378e4a18e62f8f9cc58d7614ea1e')]:
    source=a.source_dir/name
    assert hashlib.sha256(source.read_bytes()).hexdigest()==digest,source
    if source.resolve()!=out/name:shutil.copyfile(source,out/name)
here=Path(__file__).resolve().parent
shutil.copyfile(here/'rockbox_adapter.c',out/'main.c')
shutil.copyfile(here/'rockbox_stubs.h',out/'oracle_stubs.h')
for name in ['config','core_alloc','debug','diacritic','file','font_cache','kernel','lcd','logf','lru','panic','rbpaths','rbunicode','string-extra','sysfont','system']:
    (out/(name+'.h')).write_text('#include "oracle_stubs.h"\n')
cmd=['cc','-std=gnu11','-O2','-ffunction-sections','-fdata-sections','-I',str(out),str(out/'main.c'),'-Wl,--gc-sections','-o',str(out/'rockbox-read')]
p=subprocess.run(cmd,capture_output=True)
report={'command':cmd,'exit':p.returncode,'stdout':p.stdout.decode(errors='replace'),'stderr':p.stderr.decode(errors='replace')}
if p.returncode==0:report['binary_sha256']=hashlib.sha256((out/'rockbox-read').read_bytes()).hexdigest()
(out/'build.json').write_text(json.dumps(report,indent=2)+'\n')
raise SystemExit(p.returncode)
