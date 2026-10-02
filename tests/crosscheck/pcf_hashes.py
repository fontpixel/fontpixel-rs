#!/usr/bin/env python3
"""Verify every supplied Python bdftopcf SHA-256 golden, including hash-only fonts."""
import gzip,json,subprocess,sys,tempfile,warnings
from pathlib import Path
sys.dont_write_bytecode=True
sys.path.insert(0,str(Path(__file__).resolve().parent));import pcf
sys.path.insert(0,str(pcf.ROOT/'tools'));from gen_pcf_cases import reference_helpers
from bdfparser import Font
from bdfparser.pcf import write_pcf
warnings.simplefilter('ignore');helpers=reference_helpers();canon=pcf.canonicalizer()
base=pcf.ROOT.parent/'reference/bdfparser/tests/fonts';sources={p.stem:p for p in base.glob('*.bdf')}
synthetic={'syn-terminal':'SYN_TERMINAL','syn-proportional':'SYN_PROPORTIONAL','syn-unencoded-only':'SYN_UNENCODED_ONLY','syn-two-rows':'SYN_TWO_ROWS'}
results=[]
with tempfile.TemporaryDirectory(prefix='fontpixel-pcf-hashes-') as d:
 d=Path(d)
 for name,key in synthetic.items():p=d/(name+'.bdf');p.write_text(helpers[key]);sources[name]=p
 for line in (base/'golden/SHA256SUMS').read_text().splitlines():
  expected,name=line.split(); stem=name[:-4];key=next(k for k in sorted(sources,key=len,reverse=True) if stem==k or stem.startswith(k+'.'))
  tokens=stem[len(key):].lstrip('.').split('-');flags=[];options={}
  for t in tokens:
   if not t:continue
   if t in ('lsbbit','msbbit'):flags.append('-l' if t=='lsbbit' else '-m');options['bitorder']='lsb' if t=='lsbbit' else 'msb'
   elif t in ('lsbbyte','msbbyte'):flags.append('-L' if t=='lsbbyte' else '-M');options['byteorder']='lsb' if t=='lsbbyte' else 'msb'
   else:
    flags.append('-'+t)
    if t[0] in 'pu':options['glyphpad' if t[0]=='p' else 'scanunit']=int(t[1:])
  f=Font(sources[key]);oracle=write_pcf(f,**options)
  external=subprocess.run(['bdftopcf',*flags],input=canon(f),capture_output=True,check=True).stdout
  actual,_=pcf.convert(str(pcf.ROOT/'target/release/fontpixel'),sources[key],d/'out.pcf',**options)
  assert actual==oracle,(name,'Rust vs Python')
  assert expected in (pcf.digest(external),pcf.digest(actual)),(name,'supplied SHA drift')
  bug=actual!=external
  if bug:
   fmt=pcf.fmt_options(actual)
   assert fmt['byteorder']!=fmt['bitorder'] and fmt['scanunit']>1,(name,'unexpected difference')
  results.append(dict(golden=name,committed_sha256=expected,bdftopcf_sha256=pcf.digest(external),rust_sha256=pcf.digest(actual),python='byte-identical',compiler_bug_corrected=bug))
(pcf.ROOT/'tests/crosscheck/pcf-hashes-results.json').write_text(json.dumps(results,indent=2)+'\n')
print('PASS',len(results),'compiler hash goldens;',sum(x['compiler_bug_corrected'] for x in results),'known nondefault compiler bugs corrected')
