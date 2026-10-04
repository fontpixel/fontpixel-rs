#!/usr/bin/env python3
"""Compile only our original FNT/pixels using the public SDK compiler."""
import argparse,hashlib,json,shutil,subprocess
from pathlib import Path
ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--sdk',type=Path,required=True);ap.add_argument('--output-dir',type=Path,required=True);a=ap.parse_args()
root=Path(__file__).resolve().parents[3];fixture=root/'crates/fontpixel/tests/fixtures/playdate';out=a.output_dir.resolve();source=out/'source';source.mkdir(parents=True,exist_ok=True)
shutil.copyfile(fixture/'original.fnt',source/'original.fnt')
(source/'main.lua').write_text('function playdate.update() end\n')
(source/'pdxinfo').write_text('name=Original fontpixel fixtures\nauthor=fontpixel\nbundleID=org.fontpixel.originalfixture\nversion=1.0\nbuildNumber=1\n')
commands=[]
for flavor,extra in [('uncompressed',['-u']),('compressed',[])]:
    destination=out/(flavor+'.pdx');cmd=[str(a.sdk.resolve()/'bin/pdc'),'-sdkpath',str(a.sdk.resolve()),*extra,str(source),str(destination)]
    p=subprocess.run(cmd,capture_output=True);record={'command':cmd,'exit':p.returncode,'stdout':p.stdout.decode(errors='replace'),'stderr':p.stderr.decode(errors='replace')};commands.append(record)
    if p.returncode:raise RuntimeError(record)
    generated=(destination/'original.pft').read_bytes();checked=(fixture/('sdk-3.1.2-'+flavor+'.pft')).read_bytes();assert generated==checked,flavor
    record['sha256']=hashlib.sha256(generated).hexdigest()
(out/'build.json').write_text(json.dumps(commands,indent=2)+'\n')
print('Both original SDK fixtures reproduced byte-for-byte')
