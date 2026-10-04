#!/usr/bin/env python3
"""Independent monobit + original Qt4/Apple-tool consumers of original fixtures.

Qt and Apple consumer implementations stay outside this repository. The Qt
wrapper is original (reports/w5.d/qt-driver.cpp), and calls the real font engines.
"""
import argparse,hashlib,importlib.util,json,logging,os,re,struct,subprocess
from pathlib import Path
import monobit
from monobit.storage.containerformats.binhex import BinHex
from monobit.storage.streams import Stream
from io import BytesIO
p=argparse.ArgumentParser();p.add_argument('--driver',type=Path,required=True);p.add_argument('--work',type=Path,required=True);p.add_argument('--qt-consumer',type=Path,required=True);p.add_argument('--qt-lib',type=Path,required=True);p.add_argument('--lisa-consumer',type=Path,required=True);p.add_argument('--mgtk-tools',type=Path,required=True);p.add_argument('--report',type=Path,required=True);args=p.parse_args()
root=Path(__file__).resolve().parents[2];spec=importlib.util.spec_from_file_location('seeds',root/'fuzz/seed_w5d.py');seeds=importlib.util.module_from_spec(spec);spec.loader.exec_module(seeds)
args.work.mkdir(parents=True,exist_ok=True);records=[];commands=[];limitations=[]
oracle_errors=[]
class ErrorLog(logging.Handler):
 def emit(self, record):
  if record.levelno >= logging.ERROR:oracle_errors.append(record.getMessage())
logging.getLogger().addHandler(ErrorLog())
def digest(data):return hashlib.sha256(data).hexdigest()
def run(cmd,data=None):
 env=os.environ.copy();env['LD_LIBRARY_PATH']=str(args.qt_lib.resolve());result=subprocess.run([str(x) for x in cmd],input=data,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env)
 commands.append(dict(command=[str(x) for x in cmd],exit=result.returncode,stdout=result.stdout.decode('utf-8','backslashreplace'),stderr=result.stderr.decode('utf-8','backslashreplace')))
 if result.returncode:raise RuntimeError(commands[-1])
 return result.stdout

def native(fmt,path):return json.loads(run([args.driver,'inspect',fmt,path]))
def rewrite(fmt,path,operation,choice='default'):
 out=args.work/(path.name+'-'+operation+'-'+choice)
 report=json.loads(run([args.driver,operation,fmt,path,out,choice]));return out,report

def simple(g):return {key:g[key] for key in ['codes','width','height','x','y','advance','pixels']}
def monobit_check(fmt,path,mbformat):
 got=native(fmt,path)
 try: expected=monobit.load(path,format=mbformat)
 except Exception as error:
  if fmt=='lisa' and path.name.startswith('lisa-plain') and '-canonical-' not in path.name:
   limitations.append(dict(oracle='monobit-0.54.0',file=path.name,error=str(error),scope='Valid Lisa resource omits Macintosh FFFF terminator; covered by original specification regression and the document C consumer.'))
   return
  raise
 assert len(got['fonts'])==len(expected),(path,'font count')
 for font,oracle in zip(got['fonts'],expected):
  glyphs=[dict(codes=[int(c) for c in g.codepoints],width=g.width,height=g.height,x=g.left_bearing,y=g.shift_up,advance=g.advance_width,pixels=g.as_bytes().hex()) for g in oracle.glyphs]
  assert [simple(g) for g in font['glyphs']]==glyphs,(path,font['glyphs'],glyphs)
  assert font['ascent']==oracle.ascent and font['descent']==oracle.descent,(path,'global metrics')
 records.append(dict(oracle='monobit-0.54.0-unmodified',format=fmt,file=path.name,sha256=digest(path.read_bytes()),glyphs=sum(len(f.glyphs) for f in expected)))

def alpha(g):
 data=bytes.fromhex(g['pixels']);w=g['width'];h=g['height']
 if g['gray']:return data.hex()
 stride=(w+7)//8;return bytes(255 if data[y*stride+x//8]&(0x80>>(x%8)) else 0 for y in range(h) for x in range(w)).hex()

def qt_check(fmt,path):
 font=native(fmt,path)['fonts'][0];ids=[g['codes'][0] if fmt=='qpf' else g['id'] for g in font['glyphs']]
 codes=[cp for g in font['glyphs'] for cp in g['codes'] if not 0xd800<=cp<=0xdfff]
 got=json.loads(run([args.qt_consumer,fmt,path,*map(str,ids),'--',*map(str,codes)]))
 assert all(got[k]==font[k] for k in ['ascent','descent','leading']),(path,'global metrics',got,font)
 for g,actual in zip(font['glyphs'],got['glyphs']):
  assert (actual['width'],actual['height'],actual['x'],-actual['y_down']-actual['height'],actual['advance'],actual['alpha'])==(g['width'],g['height'],g['x'],g['y'],g['advance'],alpha(g)),(path,g,actual)
 expected_mapping=[[cp,g['codes'][0] if fmt=='qpf' else g['id']] for g in font['glyphs'] for cp in g['codes'] if not 0xd800<=cp<=0xdfff]
 assert got['mapping']==expected_mapping,(path,'mapping',got['mapping'],expected_mapping)
 records.append(dict(oracle='Qt-4.8.7-original-font-engine',format=fmt,file=path.name,sha256=digest(path.read_bytes()),glyphs=len(ids),mappings=len(codes)))

def mgtk_check(path,generator=False):
 font=native('mgtk',path)['fonts'][0];dump=run(['perl',args.mgtk_tools/'bin-dump_font.pl'],path.read_bytes())
 rows=re.split(r'== 0x[0-9a-f]+ ==\n',dump.decode())[1:]
 for g,text in zip(font['glyphs'],rows):
  expected=''.join('#' if v==255 else '.' for v in bytes.fromhex(alpha(g)))
  assert text.replace('\n','')==expected,(path,g,text)
 records.append(dict(oracle='a2d-original-dump_font.pl',format='mgtk',file=path.name,sha256=digest(path.read_bytes()),glyphs=len(font['glyphs'])))
 if generator:
  out=args.work/(path.name+'-a2d-generated');out.write_bytes(run(['perl',args.mgtk_tools/'bin-make_font.pl'],dump));monobit_check('mgtk',out,'mgtk');assert native('mgtk',out)['fonts']==native('mgtk',path)['fonts'];records.append(dict(oracle='a2d-original-make_font.pl',format='mgtk',file=out.name,sha256=digest(out.read_bytes()),glyphs=len(font['glyphs'])))

def lisa_resources(data):
 end=4+2*struct.unpack_from('>H',data)[0];count=struct.unpack_from('>H',data,2)[0];at=4;records=[]
 for _ in range(count):
  n=data[at];at=(at+1+n+1)&~1;a,z,_,_=struct.unpack_from('>IIII',data,at);at+=16;records.append(data[end+2*a:end+2*z])
 return records

def lisa_c_check(path):
 resource=lisa_resources(path.read_bytes())[0]
 if resource[1]&0x80:
  inp=args.work/(path.name+'.resource');out=args.work/(path.name+'.expanded');inp.write_bytes(resource);run([args.lisa_consumer,inp,out]);output=out.read_bytes()
  # C outputs the exact Lisa resource, which can omit Mac's extra table word.
  # Add only that ignored sentinel to a separate compatibility fixture for the
  # unchanged upstream NFNT consumer; never modify the native C output.
  compat=out
  first,last=struct.unpack_from('>HH',output,2);ow=16+2*struct.unpack_from('>H',output,16)[0]
  if len(output)<ow+2*(last-first+3):
   compat=out.with_name(out.name+'.mac-terminator');compat.write_bytes(output+b'\xff\xff')
  fonts=monobit.load(compat,format='nfnt')
  glyphs=[dict(codes=[int(c) for c in g.codepoints],width=g.width,height=g.height,x=g.left_bearing,y=g.shift_up,advance=g.advance_width,pixels=g.as_bytes().hex()) for g in fonts[0].glyphs]
  assert [simple(g) for g in native('lisa',path)['fonts'][0]['glyphs']]==glyphs
  records.append(dict(oracle='Lisa-document-original-f.c',format='lisa',file=path.name,sha256=digest(path.read_bytes()),expanded_sha256=digest(output),nfnt_compatibility_suffix='ffff' if compat!=out else None,glyphs=len(glyphs)))

for fmt,items in seeds.fixtures().items():
 for name,data in items.items():
  if name=='zero-width':continue # Empty-width representation is tested natively.
  path=args.work/(fmt+'-'+name);path.write_bytes(data);actual_fmt='hfnt' if name=='hfnt' else ('binhex' if fmt=='nfnt' else fmt)
  for operation in ['original','rewrite','canonical']:
   output=path if operation=='original' else rewrite(actual_fmt,path,operation)[0]
   if fmt in ['qpf','qpf2']:qt_check(fmt,output)
   else:
    mb='mac' if fmt=='nfnt' or name in ['resource-fork','macbinary-name'] and operation=='original' else ('hfnt' if actual_fmt=='hfnt' else fmt)
    monobit_check(actual_fmt,output,mb)
   if fmt=='mgtk':mgtk_check(output,generator=operation=='original')
   if fmt=='lisa':lisa_c_check(output)
  if fmt=='fbit' and name=='inline':
   for choice in ['macbinary','dfont','binhex','applesingle']:
    out,_=rewrite('fbit',path,'rewrite',choice);monobit_check('fbit',out,'mac')
  if fmt=='lisa':
   for choice in ['compressed','uncompressed']:
    out,_=rewrite('lisa',path,'canonical',choice);monobit_check('lisa',out,'lisa');lisa_c_check(out)

# Decode both exact forks with the independent upstream BinHex transport.
path=args.work/'nfnt-binhex';expected=BinHex.decode_all(None,Stream(BytesIO(path.read_bytes()),mode='r'))
for operation in ['rewrite','canonical']:
 out,_=rewrite('binhex',path,operation);decoded=BinHex.decode_all(None,Stream(BytesIO(out.read_bytes()),mode='r'))
 assert set(decoded)==set(expected) and decoded['data/Test']==expected['data/Test']
 if operation=='rewrite':assert decoded['rsrc/Test']==expected['rsrc/Test']
 # Resource-fork offsets and canonical NFNT headers can normalize. Verify
 # both decoded fork font models, with byte hashes recorded independently.
 original_rsrc=args.work/'binhex-original-resource';written_rsrc=args.work/('binhex-'+operation+'-resource')
 original_rsrc.write_bytes(expected['rsrc/Test']);written_rsrc.write_bytes(decoded['rsrc/Test'])
 original_fonts=monobit.load(original_rsrc,format='mac');written_fonts=monobit.load(written_rsrc,format='mac')
 assert [[(g.codepoints,g.width,g.height,g.left_bearing,g.shift_up,g.advance_width,g.as_bytes()) for g in f.glyphs] for f in original_fonts]==[[(g.codepoints,g.width,g.height,g.left_bearing,g.shift_up,g.advance_width,g.as_bytes()) for g in f.glyphs] for f in written_fonts]
 records.append(dict(oracle='monobit-BinHex-both-forks',format='binhex',file=out.name,data_sha256=digest(decoded['data/Test']),resource_sha256=digest(decoded['rsrc/Test'])))

assert not oracle_errors, oracle_errors
sources=[]
for path in [args.qt_consumer,args.lisa_consumer,args.driver,args.mgtk_tools/'bin-dump_font.pl',args.mgtk_tools/'bin-make_font.pl',root/'reports/w5.d/qt-driver.cpp']:
 sources.append(dict(path=str(path.resolve()),sha256=digest(path.read_bytes())))
report=dict(checks=records,commands=commands,source_hashes=sources,oracle_errors=oracle_errors,upstream_failures=limitations,limitations=['monobit 0.54 has no QPF/QPF2 codec; actual Qt 4.8.7 engines are used instead','Qt QPF2 alpha rendering ignores packed-Mono tag; native consumer tests use Gray8','monobit BinHex writer is unsupported; both forks and all Rust-produced CRCs are read by its unmodified decoder','Lisa original consumer source was extracted from the public PDF appendix, formfeed removal only, compiled externally; its implementation is not redistributed','a2d executable scripts have no repository-wide license declaration; executed externally, not redistributed'])
args.report.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'checks':len(records),'by_oracle':{name:sum(r['oracle']==name for r in records) for name in sorted({r['oracle'] for r in records})}}))
