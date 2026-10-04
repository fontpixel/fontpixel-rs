#!/usr/bin/env python3
"""Copy supplied reference sources and report prerequisites; never install/download.

The copy operation requires a fresh WORK/reference destination. Protected source
files are read only; source/copy bytes are independently hashed. Dependency or
compiler availability is reported honestly and does not authorize a workload.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT=Path(__file__).resolve().parents[1]
PACKAGES=['fonttools','freetype-py','uharfbuzz','opentype-sanitizer','brotli','Pillow','monobit','CairoSVG','unicorn']


def sha(path):
    with path.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()


def required_versions(path):
    """Read the supplied exact direct pins without importing oracle packages."""
    pins={}
    for line in path.read_text().splitlines():
        line=line.partition('#')[0].strip()
        if not line:continue
        name,separator,version=line.partition('==')
        if not separator or not name or not version or any(c.isspace() for c in name+version):
            raise ValueError('expected an exact name==version oracle pin: '+line)
        if name.lower() in {key.lower() for key in pins}:raise ValueError('duplicate oracle pin: '+name)
        pins[name]=version
    if {name.lower() for name in pins}!={name.lower() for name in PACKAGES}:
        raise ValueError('oracle requirements must enumerate the complete nine-package contract')
    return pins


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--work',type=Path,required=True)
    p.add_argument('--package',type=Path,default=ROOT.parent)
    p.add_argument('--copy-references',action='store_true')
    p.add_argument('--python',type=Path,default=ROOT.parent/'local-work/oracles/bin/python')
    p.add_argument('--requirements',type=Path,help='exact handoff nine-package pins; default PACKAGE/requirements-oracles.txt')
    p.add_argument('--output',type=Path,required=True)
    a=p.parse_args();a.package=a.package.resolve();a.work=a.work.resolve();a.output=a.output.resolve()
    for folder in ['reference','samples','validation-inputs','original-task']:
        protected=a.package/folder
        if a.work.is_relative_to(protected) or a.output.is_relative_to(protected):
            raise ValueError('work/output must be outside protected source trees')
    a.work.mkdir(parents=True,exist_ok=True)
    if a.output.exists():raise FileExistsError(a.output)
    copied=[]
    if a.copy_references:
        source=(a.package/'reference').resolve();target=a.work.resolve()/'reference'
        if target.exists():raise FileExistsError('reference copy must use a fresh destination')
        protected_files=sorted(p for p in source.rglob('*') if p.is_file())
        before={str(p.relative_to(source)):{'bytes':p.stat().st_size,'sha256':sha(p)} for p in protected_files}
        shutil.copytree(source,target,ignore=shutil.ignore_patterns('__pycache__','node_modules','.git'))
        for name,identity in before.items():
            if any(part in ['__pycache__','node_modules','.git'] for part in Path(name).parts):continue
            assert {'bytes':(target/name).stat().st_size,'sha256':sha(target/name)}==identity,(name,'copied bytes')
            assert {'bytes':(source/name).stat().st_size,'sha256':sha(source/name)}==identity,(name,'source changed')
            copied.append({'file':name,**identity})
    requirements=a.requirements.resolve() if a.requirements else a.package/'requirements-oracles.txt'
    pins=required_versions(requirements) if requirements.is_file() else {}
    code="import importlib.metadata as m,json,sys,sysconfig;names="+repr(PACKAGES)+";result={};\nfor n in names:\n try:result[n]=m.version(n)\n except m.PackageNotFoundError:result[n]=None\nprint(json.dumps({'python':sys.version,'python_version':list(sys.version_info[:3]),'executable':sys.executable,'prefix':sys.prefix,'purelib':sysconfig.get_path('purelib'),'platlib':sysconfig.get_path('platlib'),'packages':result}))"
    oracle=None
    if a.python.is_file():oracle=json.loads(subprocess.check_output([str(a.python),'-c',code],text=True,env={**os.environ,'PYTHONDONTWRITEBYTECODE':'1'}))
    tools={name:shutil.which(name) for name in ['cargo','rustc','node','bdftopcf','fc-query']}
    files={'reference':a.work/'reference','js_bundle':a.work/'bdfparser.mjs',
           'js_lock':a.work/'js-tools/package-lock.json',
           'requirements':requirements,
           'esbuild':a.work/'js-tools/node_modules/.bin/esbuild'}
    missing=[name for name,path in tools.items() if not path]
    missing.extend(name for name,path in files.items() if not path.exists())
    if oracle is None:missing.append('selected oracle Python environment')
    else:
        missing.extend(name for name,version in oracle['packages'].items() if version is None)
        if tuple(oracle['python_version'])<(3,12):missing.append('Python >= 3.12')
    mismatches={name:{'expected':version,'actual':oracle['packages'].get(name)} for name,version in pins.items()
                if oracle is not None and oracle['packages'].get(name)!=version}
    missing.extend('pinned version mismatch: '+name for name in mismatches)
    result={'status':'prerequisite inventory only','workloads_run':False,'copied_reference_files':copied,
            'tools':tools,'selected_oracle_python':str(a.python),'oracle':oracle,
            'inputs':{name:str(path) for name,path in files.items()},'missing':missing,
            'required_versions':pins,'version_mismatches':mismatches,
            'requirements_sha256':sha(requirements) if requirements.is_file() else None,
            'ready_to_attempt_validation':not missing,
            'limits':'Exact direct pins are compared. Presence/version reporting does not establish native-library identities, successful Rust build, oracle correctness or timing quiescence. OTS and HarfBuzz Python packages supply consumers; PATH ots-sanitize/hb-shape are not required.'}
    a.output.parent.mkdir(parents=True,exist_ok=True)
    with a.output.open('x') as f:json.dump(result,f,indent=2);f.write('\n')
    print(json.dumps({'missing':missing,'copied_files':len(copied),'workloads_run':False}))

if __name__=='__main__':main()
