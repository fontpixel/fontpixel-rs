#!/usr/bin/env python3
"""Generate original native fixtures for all eight W5.A2 fuzz harnesses."""
import argparse,pathlib,subprocess,tempfile
ROOT=pathlib.Path(__file__).resolve().parents[1]
def main():
 p=argparse.ArgumentParser();p.add_argument('--bridge',type=pathlib.Path,default=ROOT.parent/'local-work/target-w5a2/debug/examples/w5a2_timing');p.add_argument('--out',type=pathlib.Path,default=ROOT.parent/'local-work/w5a2/corpus');a=p.parse_args()
 kinds=['bbc','wyse','grasp','hrcg','xbin','xbin-512','consoleet','zapfont-old','zapfont-redraw']+['comfont-'+x for x in ['fontedit','frapt','frapt-tsr','letafont','mania','psfcom','udg','psfcom-spectrum-512-16','fontedit-32','frapt-32','frapt-tsr-32','mania-32']]
 with tempfile.TemporaryDirectory(prefix='w5a2-seed-') as tmp:
  for kind in kinds:
   f=pathlib.Path(tmp)/kind;subprocess.run([str(a.bridge),'fixture',kind,str(f)],check=True);family=kind.split('-')[0];dest=a.out/family;dest.mkdir(parents=True,exist_ok=True)
   payloads=[x.read_bytes() for x in sorted(f.iterdir())] if f.is_dir() else [f.read_bytes()]
   for i,b in enumerate(payloads):
    for opt in (range(21) if family=='comfont' else range(6) if family=='xbin' else range(2)):(dest/f'{kind}-{i}-{opt}').write_bytes(bytes([opt])+b)
 print(a.out)
if __name__=='__main__':main()
