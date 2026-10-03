#!/usr/bin/env python3
"""All supplied proportional/monospaced/small Latin samples through actual CLI."""
import argparse,json
from pathlib import Path
from variants import invoke,compare_full,compare_subset
from glyf import load_reference,parse_bdf
ROOT=Path(__file__).resolve().parents[2]
def main():
 p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--work',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.work.mkdir(parents=True,exist_ok=True);ref=load_reference(ROOT.parent);results=[]
 for path in sorted((ROOT.parent/'samples/tong-pixel-sans').glob('*Latin.bdf')):
  records=parse_bdf(path);ref.build.faces=lambda root,size:[('SC',records,records),('TC',{},{}),('JP',{},{}),('KR',{},{})];ref.build.latin_faces=lambda root,size:({},{})
  size='Small'if'Small'in path.name else'Large';spacing='mono'if'Mono'in path.name else'prop';plan=ref.plan(spacing,size);ref.set_size(size);family=ref.family_of(spacing,size);names=ref.names(family,'Square','test')
  common=['variants','--base',path,'--family',family,'--vendor','TONG','--font-version','test','--copyright',names['copyright'],'--license',names['licenseDescription'],'--license-url',names['licenseInfoURL']]
  for shape in ['square','dot']:
   for kind,builder in [('ttf',ref.build_ttf),('otf',ref.build_otf)]:
    label=path.stem+'-'+shape+'-'+kind;native=a.work/(label+'.'+kind);reference=a.work/(label+'-reference.'+kind);invoke(a.binary,*common,'--shape',shape,'-o',native);builder(spacing,shape,'test',plan,size).save(reference)
    results.append(compare_full(native,reference,plan,shape,label));cps=sorted(records)[::max(1,len(records)//12)];results.append(compare_subset(a.binary,native,a.work,label,cps));print(label,'PASS',flush=True)
  a.output.write_text(json.dumps({'partial':True,'results':results},indent=2)+'\n')
 a.output.write_text(json.dumps({'partial':False,'results':results},indent=2)+'\n')
if __name__=='__main__':main()
