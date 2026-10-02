#!/usr/bin/env python3
"""Replay all 800 supplied TypeScript random PCF hash goldens with Rust.
Needs an existing esbuild executable and the reference bundle used in benchmarks.
Extracts only the MIT generator/constants to temporary files; no package fetches.
"""
import argparse,json,subprocess,sys,tempfile,warnings
from pathlib import Path
sys.dont_write_bytecode=True
sys.path.insert(0,str(Path(__file__).resolve().parent));import pcf
from bdfparser import Font
from bdfparser.pcf import write_pcf
p=argparse.ArgumentParser();p.add_argument('--esbuild',required=True);p.add_argument('--bundle',required=True);args=p.parse_args()
root=pcf.ROOT.parent/'reference/bdfparser-js';source=(root/'test/pcf.test.ts').read_text();manifest=json.loads((root/'test/fixtures/pcf/bdftopcf-sha256.json').read_text());warnings.simplefilter('ignore');results=[]
with tempfile.TemporaryDirectory(prefix='fontpixel-js-pcf-') as temp:
 temp=Path(temp);ts=temp/'random.ts';js=temp/'random.mjs'
 ts.write_text(source[source.index('const FLAG_SETS:'):source.index('const TERMINAL_BDF =')]+ '\nexport {randomBdf,FLAG_SETS};\n')
 subprocess.run([args.esbuild,str(ts),'--format=esm','--outfile='+str(js)],check=True,capture_output=True)
 driver='''import {randomBdf,FLAG_SETS} from GEN;
import {Font,setWarningHandler} from BUNDLE;
import {createHash} from 'node:crypto';
setWarningHandler(null);const hash=b=>createHash('sha256').update(b).digest('hex');
const variants=FLAG_SETS.filter(([,o])=>o!==null);let cases=[];
for(let seed=1;seed<=400;seed++){const source=randomBdf(seed),font=new Font().load_bytes(new TextEncoder().encode(source));const [flags,options]=variants[seed%variants.length];cases.push({seed,source,variants:[{key:`random-${seed}`,flags:[],options:{},sha256:hash(font.topcf())},{key:`random-${seed}${flags.join('')}`,flags,options,sha256:hash(font.topcf(options))}]});}
console.log(JSON.stringify(cases));'''.replace('from GEN','from '+json.dumps(js.as_uri())).replace('from BUNDLE','from '+json.dumps(Path(args.bundle).resolve().as_uri()))
 cases=json.loads(subprocess.run(['node','--input-type=module','--eval',driver],check=True,capture_output=True).stdout)
 for case in cases:
  src=temp/'in.bdf';src.write_text(case['source']);font=Font().load_string(case['source'])
  for variant in case['variants']:
   opts=variant['options'];actual,_=pcf.convert(str(pcf.ROOT/'target/release/fontpixel'),src,temp/'out.pcf',**opts)
   assert pcf.digest(actual)==variant['sha256'],(variant['key'],'Rust vs TypeScript')
   assert actual==write_pcf(font,**opts),(variant['key'],'Rust vs Python')
   external=subprocess.run(['bdftopcf',*variant['flags']],input=case['source'].encode(),capture_output=True,check=True).stdout
   expected=manifest[variant['key']];assert expected in (pcf.digest(actual),pcf.digest(external)),(variant['key'],'supplied SHA drift')
   bug=actual!=external
   if bug:assert opts.get('byteorder','msb')!=opts.get('bitorder','msb') and opts.get('scanunit',1)>1,variant['key']
   results.append(dict(key=variant['key'],committed_sha256=expected,rust_sha256=pcf.digest(actual),bdftopcf_sha256=pcf.digest(external),python='byte-identical',typescript='byte-identical',compiler_bug_corrected=bug))
 assert {r['key'] for r in results}==set(manifest)
(pcf.ROOT/'tests/crosscheck/pcf-js-hashes-results.json').write_text(json.dumps(results,indent=2)+'\n')
print('PASS',len(results),'TypeScript hash goldens;',sum(r['compiler_bug_corrected'] for r in results),'known compiler differences')
