#!/usr/bin/env python3
"""Five-run medians: full Tong SC CFF square/TrueType dot WOFF2 strategies.

Do not run concurrently with fuzzing/builds/other benchmarks. All JSON results
are checkpointed after each measurement. Disk reads/writes are outside timers.
The reference executable is the precise woff 0.6.3 call in Tong export-rs.
"""
import argparse, io, importlib.metadata, json, os, platform, statistics, subprocess, sys, time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'tests/crosscheck'))
from glyf import load_reference,parse_bdf
from fontTools.ttLib import TTFont
from fontTools.ttLib.woff2 import WOFF2FlavorData

def command(binary,*args):return json.loads(subprocess.check_output([str(binary),*map(str,args)],text=True))
def main():
    p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--reference',type=Path,required=True);p.add_argument('--work',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--package',type=Path,default=ROOT.parent);p.add_argument('--prepare-only',action='store_true');p.add_argument('--resume',action='store_true');a=p.parse_args();a.work.mkdir(parents=True,exist_ok=True)
    paths=[a.work/'Tong-SC-square.otf',a.work/'Tong-SC-dot.ttf']
    if not all(p.exists() for p in paths):
        ref=load_reference(a.package);source=parse_bdf(a.package/'samples/tong-pixel-sans/TongPixelSans14Large-SC.bdf');cps=sorted(source);records={ref.gname(cp):source[cp] for cp in cps};order=['.notdef']+list(records);cmap={cp:ref.gname(cp) for cp in cps};ref.set_size('Large')
        for path,builder,shape in [(paths[0],ref.build_otf,'square'),(paths[1],ref.build_ttf,'dot')]:
            f=builder('prop',shape,'test',(order,records,cmap,{}),'Large');f['head'].created=f['head'].modified=2082844800;f.recalcTimestamp=False;f.save(path);print('prepared',path,path.stat().st_size,flush=True)
    if a.prepare_only:return
    result=json.loads(a.output.read_text()) if a.resume and a.output.exists() else {'machine':{'platform':platform.platform(),'cpu':next((v.split(':',1)[1].strip() for v in Path('/proc/cpuinfo').read_text().splitlines() if v.startswith('model name')),''),'cpus':os.cpu_count(),'memory':Path('/proc/meminfo').read_text().splitlines()[0]},'tools':{n:importlib.metadata.version(n) for n in ['fonttools','brotli']},'results':[]}
    completed={x['label'] for x in result['results']}
    def record(label,data):
        data['label']=label;data['median_seconds']=statistics.median(data['seconds']);result['results'].append(data);a.output.write_text(json.dumps(result,indent=2)+'\n');print(label,round(data['median_seconds'],6),data['bytes'],flush=True)
    for path in paths:
        for quality,threads,transform in [(q,1,True) for q in [5,8,9,11]]+[(11,2,True),(11,4,True),(5,1,False)]:
            label=f'native-{path.stem}-q{quality}-t{threads}-x{int(transform)}'
            if label in completed:continue
            data=command(a.binary,'bench','woff2',path,a.work/(label+'.woff2'),quality,threads,*([] if transform else ['null']));record(label,data)
        for quality in [5,11]:
            label=f'tong-woff063-{path.stem}-q{quality}'
            if label in completed:continue
            data=command(a.reference,path,a.work/(label+'.woff2'),quality);record(label,data)
        label=f'fonttools-{path.stem}-q11'
        if label not in completed:
            raw=path.read_bytes();times=[];output=None
            for _ in range(5):
                begin=time.perf_counter();f=TTFont(io.BytesIO(raw),recalcTimestamp=False);f.flavor='woff2';f.flavorData=WOFF2FlavorData(transformedTables={'glyf','loca','hmtx'});out=io.BytesIO();f.save(out);output=out.getvalue();times.append(time.perf_counter()-begin)
            (a.work/(label+'.woff2')).write_bytes(output);record(label,{'seconds':times,'bytes':len(output)})
    # Four independent full fonts, same multiset and order in every strategy.
    for quality in [5,11]:
        for jobs in [1,2,4]:
            label=f'native-four-fonts-q{quality}-jobs{jobs}'
            if label in completed:continue
            data=command(a.binary,'batch',quality,1,jobs,*paths,*paths);record(label,data)
if __name__=='__main__':main()
