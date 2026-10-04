#!/usr/bin/env python3
"""Validate original scaling workloads without collecting performance timings."""
import argparse,json,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('driver',type=Path);p.add_argument('seeds',type=Path);p.add_argument('report',type=Path);a=p.parse_args();records=[]
for path in sorted(a.seeds.iterdir()):
 if '.canonical' in path.name:continue
 fmt=path.name.split('-')[0];out=path.with_name(path.name+'.canonical')
 command=[str(a.driver),'canonical',fmt,str(path),str(out)];r=subprocess.run(command,capture_output=True)
 assert r.returncode==0,(command,r.stderr)
 original=json.loads(subprocess.check_output([str(a.driver),'inspect',fmt,str(path)]));written=json.loads(subprocess.check_output([str(a.driver),'inspect',fmt,str(out)]))
 assert len(original['fonts'][0]['glyphs'])==len(written['fonts'][0]['glyphs'])
 for before,after in zip(original['fonts'][0]['glyphs'],written['fonts'][0]['glyphs']):
  assert all(before[k]==after[k] for k in ['codes','width','height','x','y','advance','gray','pixels']),(path,before,after)
 records.append({'file':str(path),'command':command,'exit':r.returncode,'glyphs':len(original['fonts'][0]['glyphs']),'report':json.loads(r.stdout)})
a.report.write_text(json.dumps(records,indent=2)+'\n');print(json.dumps({'fixtures':len(records),'timing_collected':False}))
