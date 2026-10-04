#!/usr/bin/env python3
"""Build the unchanged official SDL SFont reader outside the source tree."""
import argparse,hashlib,json,subprocess,urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
DEB='libsdl2-dev_2.30.0+dfsg-1ubuntu3.1_amd64.deb'
DEB_SHA='8d78c93699aafdb33205ef7712baeefc8d8aa806862fac4c2554f3f8db234da1'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def obtain(url,path,expected):
    if not path.exists():
        path.parent.mkdir(parents=True,exist_ok=True)
        with urllib.request.urlopen(url)as r:path.write_bytes(r.read())
    assert sha(path)==expected,(str(path),sha(path),expected)
def main():
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--work',type=Path,required=True);ap.add_argument('--report',type=Path,required=True);ap.add_argument('--sdl-library',type=Path,default=Path('/lib/x86_64-linux-gnu/libSDL2-2.0.so.0'));a=ap.parse_args();a.work.mkdir(parents=True,exist_ok=True)
    evidence=json.loads((ROOT/'reports/w5.e2-image/sources.json').read_text())
    for name in ['SFont.c','SFont.h']:
        e=next(e for e in evidence if e['name']==name);obtain(e['url'],a.work/name,e['sha256'])
    deb=a.work/DEB;obtain('http://archive.ubuntu.com/ubuntu/pool/universe/libs/libsdl2/'+DEB,deb,DEB_SHA)
    commands=[]
    for cmd in [['dpkg-deb','-x',str(deb),str(a.work/'headers')],['cc','-O2','-I'+str(a.work/'headers/usr/include/SDL2'),'-I'+str(a.work/'headers/usr/include/x86_64-linux-gnu'),'-I'+str(a.work),str(ROOT/'reports/w5.e2-image/oracles/sfont_adapter.c'),str(a.work/'SFont.c'),str(a.sdl_library),'-o',str(a.work/'sfont-reader')]]:
        p=subprocess.run(cmd,capture_output=True);commands.append(dict(argv=cmd,exit=p.returncode,stdout=p.stdout.decode(errors='replace'),stderr=p.stderr.decode(errors='replace')));assert p.returncode==0,commands[-1]
    report=dict(commands=commands,inputs={str(p):sha(p)for p in [a.work/'SFont.c',a.work/'SFont.h',deb,a.sdl_library,ROOT/'reports/w5.e2-image/oracles/sfont_adapter.c']},output_sha256=sha(a.work/'sfont-reader'))
    a.report.parent.mkdir(parents=True,exist_ok=True);a.report.write_text(json.dumps(report,indent=2)+'\n')
if __name__=='__main__':main()
