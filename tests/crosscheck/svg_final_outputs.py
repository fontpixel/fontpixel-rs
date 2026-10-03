#!/usr/bin/env python3
"""Re-export all oracle cases and require exact bytes from the final CLI build."""
import argparse,hashlib,json,subprocess,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def main():
 p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--report',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();results=[]
 with tempfile.TemporaryDirectory(prefix='fontpixel-w28-final-output-') as td:
  output=Path(td)/'regenerated.svg'
  for case in json.loads(a.report.read_text())['cases']:
   source=ROOT.parent/'samples/tong-pixel-sans'/case['source']
   subprocess.run([a.binary,'convert',str(source),'-o',str(output),'--shape',case['shape'],'--dotsize','1' if case['shape']=='square' else '0.9'],check=True,capture_output=True)
   got=hashlib.sha256(output.read_bytes()).hexdigest();assert got==case['sha256'],case
   results.append(dict(source=case['source'],shape=case['shape'],sha256=got,bytes=output.stat().st_size))
 a.output.write_text(json.dumps(results,indent=2)+'\n');print('PASS: all six final-code outputs are byte-identical to the exhaustive independent oracle inputs.')
if __name__=='__main__':main()
