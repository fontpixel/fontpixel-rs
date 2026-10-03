#!/usr/bin/env python3
"""Five-run native CLI timings; run only after concurrent correctness/fuzz work."""
import argparse,hashlib,json,os,platform,statistics,subprocess,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
def main():
 p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--work',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.work.mkdir(parents=True,exist_ok=True)
 sample=ROOT.parent/'samples/tong-pixel-sans';path=lambda region:sample/f'TongPixelSans14Large-{region}.bdf'
 common=['variants','--base',str(path('SC')),'--lang','zh-Hant,zh-HK='+str(path('TC')),'--lang','ja='+str(path('JP')),'--lang','ko='+str(path('KR')),'--script','latn,grek,cyrl='+str(path('Latin')),'--family','Benchmark Regional Pixel','--font-version','1.0'];results=[]
 def measure(label,args,out):
  times=[];hashes=[]
  for _ in range(5):
   start=time.perf_counter();r=subprocess.run([str(a.binary),*args,'-o',str(out)],capture_output=True,text=True);elapsed=time.perf_counter()-start;assert r.returncode==0,r.stderr;times.append(elapsed);hashes.append(hashlib.sha256(out.read_bytes()).hexdigest())
  assert len(set(hashes))==1
  results.append({'case':label,'seconds':times,'median_seconds':statistics.median(times),'bytes':out.stat().st_size,'sha256':hashes[0]});print(label,statistics.median(times),flush=True)
 for shape in ['square','dot']:
  for kind in ['ttf','otf']:
   output=a.work/(shape+'.'+kind);measure('variants-'+shape+'-'+kind,common+['--shape',shape],output)
   measure('subset-'+shape+'-'+kind,['subset',str(output),'--text','汉字“Hello”…骨辻国語あ한'],a.work/(shape+'-subset.'+kind))
 a.output.write_text(json.dumps({'platform':platform.platform(),'cpu':platform.processor(),'logical_cpus':os.cpu_count(),'binary_sha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),'source':'all provided Large SC/TC/JP/KR/Latin BDFs, including ZHT/ZHH aliases','scope':'Native CLI wall-clock timings, five repetitions. No comparison with unlike reference workloads. No whole-phase export performance claim.','results':results},indent=2)+'\n')
if __name__=='__main__':main()
