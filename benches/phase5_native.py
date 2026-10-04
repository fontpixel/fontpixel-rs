#!/usr/bin/env python3
"""Prepare verified release adapters; collect five quiet, memory-only repetitions.

Preparation and smoke results are never formal benchmark evidence. This runner
owns orchestration and an independent in-memory monobit adapter, not codecs.
"""
from __future__ import annotations
import argparse, datetime, hashlib, importlib.util, io, json, os, pathlib, platform
import math, statistics, subprocess, sys, time, shutil, tempfile
sys.dont_write_bytecode = True
ROOT = pathlib.Path(__file__).resolve().parents[1]
WORK = pathlib.Path(os.environ.get('FONTPIXEL_WORK_ROOT', ROOT.parent / 'local-work'))
SCRIPT = pathlib.Path(__file__).resolve()
FEATURES = {
 'a1':'dosstart,edwin,mkwinfont,psf2txt,pcr,plus3dos,drhalo,writeon',
 'a2':'bbc,wyse,grasp,hrcg,xbin,consoleet,zapfont,comfont',
 'c1':'daisy,polyprint,printshop,tasprint,pxl,signum',
 'e1':'rockbox,u8m,cybiko,playdate,hmzk,sbf', 'x':'raw,bmfont',
 'p3_52':'hanfont,iigs', 'p3_53':'vfont,fzx,figlet','p3_54':'palm,gdr',
 'c':'c,drcs','color':'color',
 'd':'nfnt,fbit,lisa,mgtk,qpf,qpf2','b':'beos,geos,gpi,oberon,pcgeos,symbos,vtfont,wsfont',
 'c2':'catalyst,chiwriter,dashen,hppcl,prebuilt,riscos,xerox',
 'e2':'image,pilfont,sfont,ssfn,vlw,pfr',
 'transport':'raw,gzip',
}
EXAMPLES={'a1':['w5a1_timing'],'a2':['w5a2_timing'],'c1':['w5c1_timing'],
 'e1':['w5e1_timing','phase5_device_read_timing'],'x':['w5x_timing'],
 'p3_52':['w52_timing'],'p3_53':['w53_timing'],'p3_54':['w54_timing'],
 'c':['w55_timing'],'color':['w56_timing'],'d':['w5d_timing'],'b':['w5b_timing'],
 'c2':['w5c2_timing'],'e2':['w5e2_timing','w5e2_outline_timing'],'transport':['transport_timing']}
PENDING={}
# Populated only from the owning workers' final freeze handoffs. Interface
# readiness alone is insufficient to make a new group's preparation formal.
GROUP_FREEZES={
 'c':{'reports/w5.fon-detection.sha256':'c48a435bb9627e0c0f8e0b572bfeb0d40dd79e35a7ed5b5e740a4d1e51315889'},
 'b':{'reports/w5.b-freeze-final4.json':'c40f6f6c41ae3ef3d620d6eb4e4dadadb65335e8a9535b7b09a90a26299bea50'},
 'c2':{'reports/w5.c2-freeze.json':'e6033f85b8c910bd0485438b957e09067be28f37620893d40e5a975b15b00a70'},
 'e2':{'reports/w5.e2-outline-freeze.json':'91f1fa5467ef09a8dbc31b7109281b78913f77444c645e653456ef8a59a5905f','reports/w5.e2-image/source-hashes.json':'7dfb929f932eda0605555fc292e56793118403318691626b4f7ff142bb0c14d8'},
 'transport':{'reports/w5.transport-budget-sha256.txt':'6d210c7927917214db5f9b027bf05bc22e0219be809143296de8cafb87bf708a'},
}
FREEZE_REQUIRED={'b','c','c2','e2','transport'}
def sha(path):
 h=hashlib.sha256()
 with pathlib.Path(path).open('rb') as stream:
  for b in iter(lambda:stream.read(1024*1024),b''):h.update(b)
 return h.hexdigest()
def dump(path,value):
 path=pathlib.Path(path);path.parent.mkdir(parents=True,exist_ok=True)
 temporary=path.with_name(path.name+'.tmp');temporary.write_text(json.dumps(value,indent=2,ensure_ascii=False)+'\n');temporary.replace(path)
def fresh(path):
 if pathlib.Path(path).exists():raise ValueError(f'Refusing to overwrite evidence: {path}')
def module(path):
 spec=importlib.util.spec_from_file_location('phase5_'+path.stem,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m

def catalog():
 rows=[]
 def add(group,case,fmt,path=None,parser='ns_per_op',mode='bench',kwargs=None,write=True,notes=''):
  rows.append(dict(group=group,id=case,format=fmt,source=path,parser=parser,mode=mode,kwargs=kwargs or {},python_write=write,notes=notes))
 for n in FEATURES['a1'].split(','):add('a1',n,n,f'{n}.oracle','ns_total',write=n!='drhalo')
 for n in ['bbc','wyse','grasp','hrcg','xbin','xbin-512','consoleet','zapfont-old','zapfont-redraw']:
  add('a2',n,n,'fixture-'+n,mode='time',write=n not in ['xbin-512','zapfont-redraw'],notes='Known independent writer projection; no writer ratio' if n in ['xbin-512','zapfont-redraw'] else '')
 for n in ['fontedit','frapt','frapt-tsr','mania','letafont-pcw','udg-pcw','psfcom-pcw','psfcom-spectrum-512-16']:
  add('a2','comfont-'+n,'comfont-'+n,'fixture-comfont-'+n,mode='time',write=False,notes='Independent COM oracle has no executable writer')
 for n,path in [('daisy','daisy-v3-dense/source.nlq'),('polyprint','polyprint-dense/source.bin'),('printshop','printshop-dense/source.bin'),('tasprint','tasprint-48k-dense/source.bin'),('pxl','pxl-1001-dense/source.bin'),('signum','signum-e24-dense/source.bin')]:
  add('c1',n,n,path,'us_per_op',kwargs={'version':'48k'} if n=='tasprint' else {},write=n=='tasprint')
 add('c1','pxl-1002','pxl','pxl-1002-dense/source.bin','us_per_op',write=False)
 add('c1','signum-p24','signum','signum-p24-dense/source.bin','us_per_op',write=False)
 for n,ext in [('rb11','rb11'),('rb12','rb12'),('u8m','u8m'),('cybiko','cyf'),('hmzk','hmzk'),('sbf','sbf'),('playdate','fnt'),('split','fnt'),('pft','pft')]:
  path='original-'+n+'.'+ext if n in ['playdate','split'] else 'original.'+ext
  add('e1','read-'+n,n,path,mode='device_read',write=False,notes='Original fixture pixels verified before memory-only reads; writer benchmark has a separate generated model')
 add('e1','writers','device-writers',parser='e1_tsv',mode='device_write',write=False)
 for name,fmt,config in [('legacy','raw',{'width':8,'height':8,'count':8}),('bits9','raw',{'width':9,'height':3,'count':7,'packing':'bit'}),('columns16','raw',{'width':16,'height':5,'count':6,'order':'column'}),('harlekin512','raw',{'width':8,'height':8,'count':512,'strike_count':512}),('bmfont','bmfont',{'encoding':'json','page_width':4,'page_height':4})]:
  add('x',name,fmt,f'{name}/source.'+('fnt' if fmt=='bmfont' else 'raw'),'ns_total',kwargs=config)
 add('p3_52','hanfont-components','components','hanfont/original','seconds_total','time',{'cell':[16,16],'count':360},notes='Independent raw parse of native components, not Korean composition')
 add('p3_52','hanfont-syllables','syllables','hanfont/original','seconds_total','time',write=False,notes='Composition has no matched memory-only independent timing adapter')
 for n in ['original','version105','binscii-0']:add('p3_52','iigs-'+n,'iigs','iigs/'+n,'seconds_total','time',write=n!='binscii-0')
 for n,path in [('vfont','vfont-full256.source'),('fzx','fzx-full224.source'),('figlet','figlet-full358.source')]:add('p3_53',n,n,path,'us_per_op')
 for n,path in [('palm','large-palm.bin'),('gdr','gdr-large.bin')]:add('p3_54',n,n,path,'us_per_op',write=False,notes='Palm reads bare v1 but native writer emits PRC; no matching monobit PRC writer' if n=='palm' else 'No independent GDR reader/writer')
 for n in ['drcs','u8g2','u8glib','u8glib-compact','u8glib-gray','u8x8','u8x8-legacy','netbsd','lvgl','lvgl-rle','lvgl-xor','lvgl2-rle','lvgl4-xor','lvgl8','lvgl4-binary-xor']:
  add('c',n,n,'fixture-'+n+'.h',mode='time',write=n in ['drcs','netbsd'],notes='C runtime consumers verify pixels; they do not supply a matched parser/writer timing baseline')
 for name in ['fbit-256','lisa-256','mgtk-256','qpf-256','qpf-2048','qpf2-256','qpf2-2048']:
  for mode in ['time','time-canonical']:add('d',name+'-'+mode,name.split('-')[0],'benchmark/'+name,'seconds_total',mode,write=False,notes='Native replay' if mode=='time' else 'Canonical regeneration; source preservation disabled')
 for mode in ['time','time-canonical']:add('d','binhex-'+mode,'binhex','nfnt/binhex','seconds_total',mode,write=False,notes='Monobit has no BinHex writer')
 add('color','cbdt-sbix','color',parser='seconds_per_op',mode='color',write=False,notes='4096 original 14x14 glyphs; no matching in-memory independent 4096-glyph timing adapter')
 large={'beos','oberon','symbos-big-extended','wsfont','wsfont-alpha','vtfont','geos','geos-utf8','gpi','gpi-lx','pcgeos','pcgeos-dbcs'}
 for profile in ['beos','beos-16','oberon','symbos','symbos-small','symbos-big','symbos-big-extended','symbos-small-extended','wsfont','wsfont-rbit','wsfont-rbyte','wsfont-rbit-rbyte','wsfont-alpha','vtfont','geos-bare','geos','geos-extended','geos-utf8','geos-mega','geos-mega-utf8','gpi-fixed','gpi-prop','gpi','gpi-ne','gpi-lx','pcgeos','pcgeos-styled','pcgeos-dbcs']:
  matched={'beos-16':'beos','oberon':'oberon','symbos':'symbos','wsfont':'wsfont','vtfont':'vtfont','geos-bare':'vlir','geos':'geos','pcgeos':'pcgeos'}
  add('b',profile,profile,profile+('.large.bin' if profile in large else '.bin'),'seconds_split_total',write=profile in matched,notes='Fixed 100 reads / 20 writes; three warmups. Original native metadata retained. '+('BeOS Native8 interpretation differs from monobit FullNibble; independent timing unavailable.' if profile=='beos' else 'Only independently validated profiles receive a monobit baseline.'))
  rows[-1]['oracle_format']=matched.get(profile)
  if profile=='vtfont':
   rows[-1]['python_write']=False
   rows[-1]['notes']+=' Default native writer preserves the original byte stream; this passthrough has no comparable independent writer timing.'
 add('b','vtfont-rebuild','vtfont-rebuild','vtfont.large.bin','seconds_split_total',notes='Explicit canonical VFNT rebuild; default passthrough remains a separate baseline.')
 rows[-1]['oracle_format']='vtfont'
 seen_c2=set()
 for fmt,label,_ in module(ROOT/'fuzz/seed_w5c2.py').cases():
  if (fmt,label) in seen_c2:continue
  seen_c2.add((fmt,label))
  profile=fmt+(':'+label.split('-')[0] if fmt=='xerox' else ':data' if fmt=='catalyst' and label=='data' else ':'+label.replace('-stride3',':stride3') if fmt=='chiwriter' and 'stride' in label else ':legacy' if fmt=='hppcl' and label=='legacy' else '')
  excluded=(fmt=='catalyst' and label!='dense')or(fmt=='chiwriter' and 'stride' in label)or(fmt=='riscos' and label in ['v7','v8','phases','wide'])or(fmt=='hppcl' and label=='rle')or(fmt=='dashen' and label=='v4')or(fmt=='prebuilt' and label.startswith('le-'))
  add('c2',fmt+'-'+label,profile,fmt+'/'+label,'us_per_op',write=fmt=='hppcl' and not excluded,notes='Fixed 300 reads and writes; ten warmups. '+('Native/spec-only variant; independent reader unavailable.' if excluded else 'Independent reader profile verified in W5.C2 crosscheck; only HPPCL has a matching writer.'))
  rows[-1].update(oracle_format=None if excluded else label.split('-')[0] if fmt=='xerox' else 'riscos-xy' if fmt=='riscos' and label.startswith('xy') else fmt,oracle_read_kwargs={'filetype':17 if label=='v4' else 16} if fmt=='chiwriter' else {})
  if fmt=='xerox':rows[-1]['notes']+=' Alto oracle adds one y pixel; Prepress retains padded height; no automatic model-equivalence speed ratio.'
  if fmt=='prebuilt':rows[-1]['notes']+=' The independent reader omits vertical advance and mirrors little-endian rasters; LE has no direct matching timing baseline. Native fields remain intact.'
 for profile,ext,count in [('pilfont','pil',100),('vlw8','vlw',100),('vlw9','vlw',100),('vlw10','vlw',100),('vlw','vlw',100),('sfont','png',94),('rfont','png',100),('imageset','',100)]:
  add('e2','read-'+profile,profile,profile+'/font'+('.'+ext if ext else ''),'read_ns_total','image_read',write=False,notes='Original emitted fixture; primary and companions preloaded. Five warmups; each read timed separately and full model/report equality checked after stopping its clock.')
  rows[-1].update(source_glyphs=count,oracle_format='imageset' if profile=='imageset' else None)
  if profile in ['sfont','rfont']:rows[-1]['notes']+=' BitsNPicas may add a runtime virtual space; actual returned glyph counts remain unmodified (SFont: 95 versus native 94 encoded glyphs).'
 add('e2','image-writers','image-writers',parser='e2_tsv',mode='image_write',write=False,notes='One default native write per format; source model has 100 glyphs (SFont clamps to 94). Original metadata remains present; these writer models differ from the emitted/readback fixtures. Independent writers are unavailable for this matching workload.')
 for name in ['ssfn-original.sfn','ssfn-original.asc','ssfn-gray.asc','ssfn-ligature.asc','ssfn-gray.sfn','ssfn-repeat.sfn','ssfn-collection.sfn','ssfn-gzip.sfn.gz']:
  add('e2',name,'ssfn','outline/'+name,'ns_total','outline',write=False,notes='Default native binary writer; original native metadata retained. sfnconv is a file/process correctness oracle, not a memory timing baseline.')
 for name in ['pfr-0-0-1.pfr','pfr-1-0-1.pfr','pfr-2-0-1.pfr','pfr-ft-0-0-1.pfr','pfr-ft-1-0-1.pfr','pfr-ft-2-0-1.pfr','pfr-ft-multistrike.pfr']:
  add('e2',name,'pfr','outline/'+name,'ns_total','outline',write=False,notes='Default native writer preserves dialect. FreeType baseline only for its explicit dialect; direct known-code lookup, full bitmap materialization, no 65,536-code scan.')
  rows[-1]['freetype']=name.startswith('pfr-ft-')
 add('transport','raw-gzip','raw','original.raw',parser='transport_total',mode='transport',write=False,notes='Original 256-glyph 8x16 in-memory raw/gzip pipeline; glyph formula is regenerated inside the probe before timing and the same original bytes are archived by the runner. Gzip default write includes the second same-profile budget audit. Native-only, no external ratio.')
 return rows

def log_paths(log):
 """Append stream suffixes: case IDs can already contain .sfn/.asc/.pfr."""
 log=pathlib.Path(log)
 return pathlib.Path(str(log)+'.stdout'),pathlib.Path(str(log)+'.stderr')

def verify_command_logs(records):
 """One invocation owns two immutable logs; verify bytes before publication."""
 seen=set()
 for record in records:
  for stream in ['stdout','stderr']:
   path=pathlib.Path(record[stream]);key=str(path.resolve())
   if key in seen:raise ValueError('reused raw log path across invocations: '+str(path))
   seen.add(key)
   try:actual=sha(path)
   except OSError as error:raise ValueError('unreadable raw '+stream+' log: '+str(path)) from error
   if actual!=record[stream+'_sha256']:raise ValueError('raw '+stream+' log hash changed: '+str(path))
 return dict(commands=len(records),files=len(seen),all_sha256_match=True)

def command(cmd,log,cwd=ROOT,env=None):
 log=pathlib.Path(log);log.parent.mkdir(parents=True,exist_ok=True)
 stdout,stderr=log_paths(log)
 # Preflight both names before creating either; exclusive opens also reject
 # races and dangling symlinks without truncating any existing evidence.
 for path in [stdout,stderr]:
  if os.path.lexists(path):raise FileExistsError('Refusing to overwrite raw log: '+str(path))
 with stdout.open('xb') as out,stderr.open('xb') as err:
  start=time.monotonic()
  proc=subprocess.Popen(list(map(str,cmd)),cwd=cwd,env=env,stdout=out,stderr=err)
  _,status,usage=os.wait4(proc.pid,0);proc.returncode=os.waitstatus_to_exitcode(status)
 elapsed=time.monotonic()-start
 record=dict(command=list(map(str,cmd)),exit=proc.returncode,process_wall_seconds=elapsed,peak_rss_kib=usage.ru_maxrss if sys.platform!='darwin' else usage.ru_maxrss/1024,stdout=str(stdout),stderr=str(stderr),stdout_sha256=sha(stdout),stderr_sha256=sha(stderr))
 if proc.returncode:raise RuntimeError(json.dumps(record)+'\n'+stderr.read_text(errors='replace')[-6000:])
 return record,stdout.read_text()

def generated(group,directory,bins,python):
 if group!='x':directory.mkdir(parents=True,exist_ok=True)
 if group=='a1':module(ROOT/'tests/crosscheck/w5a1.py').emit(directory)
 elif group=='c1':
  seed=module(ROOT/'fuzz/seed_w5c1.py')
  for fmt,label,data in seed.cases():
   p=directory/(fmt+'-'+label)/('source.nlq' if fmt=='daisy' else 'source.bin');p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(data)
   if fmt=='daisy' and label=='mag':p.with_suffix('.nl2').write_bytes(seed.daisy('mag',part=1))
 elif group=='p3_53':
  seed=module(ROOT/'fuzz/seed_w53.py')
  for name,fn in [('vfont-full256.source','vfont_full'),('fzx-full224.source','fzx_full'),('figlet-full358.source','figlet_full')]:
   data=getattr(seed,fn)()
   # The independent FIGlet oracle cannot read the optional negative tag fixture.
   (directory/name).write_bytes(data)
 elif group=='d':subprocess.run([python,str(ROOT/'fuzz/seed_w5d.py'),str(directory)],check=True)
 elif group=='p3_52':
  subprocess.run([python,str(ROOT/'fuzz/seed_w52.py'),'--directory',str(directory)],check=True)
 elif group in ['a2','c']:
  for c in catalog():
   if c['group']==group:subprocess.run([bins[0],'fixture',c['format'],str(directory/c['source'])],check=True)
 elif group in ['p3_54','e1','b']:subprocess.run([bins[0],'emit',str(directory)],check=True)
 elif group=='c2':subprocess.run([python,str(ROOT/'fuzz/seed_w5c2.py'),str(directory)],check=True)
 elif group=='e2':
  for case in catalog():
   if case['group']=='e2' and case['mode']=='image_read':
    path=directory/case['source'];path.parent.mkdir(parents=True,exist_ok=True)
    result=subprocess.run([bins[0],'emit',case['format'],str(path),str(case['source_glyphs'])],check=True,capture_output=True,text=True)
    (path.parent/'generation.json').write_text(result.stdout)
  subprocess.run([python,str(ROOT/'fuzz/seed_w5e2_outline.py'),'--out',str(directory/'outline')],check=True)
 elif group=='x':subprocess.run([python,str(ROOT/'tests/crosscheck/w5x.py'),'--binary',bins[0],'--output-dir',str(directory)],check=True)
 elif group=='transport':
  data=bytes(0 if row in [0,15] else (((glyph<<(row%8))|(glyph>>(8-row%8)))&255)^(((0x81>>(row//2))|(0x81<<(8-row//2)))&255) for glyph in range(256) for row in range(16))
  (directory/'original.raw').write_bytes(data)
 elif group=='color':pass
 else:raise ValueError('No fixture generator for '+group)

def monobit_worker(spec,path,loops):
 import monobit,importlib.metadata
 from monobit.storage.streams import Stream
 fmt=spec.get('oracle_format') or spec['format'];read_args=dict(spec.get('oracle_read_kwargs',spec.get('kwargs',{})));write_args=dict(spec.get('oracle_write_kwargs',read_args))
 allow_write=spec.get('python_write',True);why=None;projection=None
 if fmt.startswith('comfont-'):fmt='frapt-tsr' if 'frapt-tsr' in fmt else fmt.split('-')[1]
 elif fmt.startswith('zapfont-'):fmt='zapfont' if fmt.endswith('old') else 'zapredraw'
 elif fmt=='xbin-512':fmt='xbin'
 elif fmt=='drcs':fmt='dec'
 elif fmt=='components':fmt='raw';write_args={'strike_count':1}
 elif fmt=='palm':fmt='nfnt';read_args={};write_args={}
 elif fmt=='binhex':fmt='mac';read_args={};write_args={}
 if pathlib.Path(path).is_dir():
  files={p.name:p.read_bytes() for p in pathlib.Path(path).iterdir() if p.is_file()};size=sum(map(len,files.values()))
  class Location:
   path=pathlib.Path('.')
   def iter_sub(self,_):return files.keys()
   def open(self,name,mode):return Stream.from_data(files[str(name)],mode='r',name=str(name),where=self) if mode=='r' else Stream(io.BytesIO(),mode='w',name=str(name),where=self)
  if fmt=='imageset':
   from monobit.storage.fontformats.image.image import load_imageset
   def read():return [load_imageset(Location(),background='darkest')]
   allow_write=False
   def write(fonts):raise NotImplementedError('No matching original-model writer workload')
  else:
   from monobit.storage.fontformats.text.consoleet import load_clt,save_clt
   def read():return [load_clt(Location())]
   def write(fonts):return save_clt(fonts,Location())
 else:
  data=pathlib.Path(path).read_bytes();size=len(data)
  def read():return monobit.load(io.BufferedReader(io.BytesIO(data)),format=fmt,**read_args)
  def write(fonts):return monobit.save(fonts,io.BytesIO(),format=fmt,**write_args)
  if spec.get('group') in ['b','c2']:
   files={}
   if fmt.startswith('riscos') and pathlib.Path(path).with_name('IntMetrics').is_file():
    files['IntMetrics']=pathlib.Path(path).with_name('IntMetrics').read_bytes();size+=len(files['IntMetrics'])
   class MemoryParent:
    path=pathlib.Path('.')
    def open(self,name,mode='r'):
     if mode!='r' or str(name) not in files:raise FileNotFoundError(str(name))
     return Stream.from_data(files[str(name)],mode='r',name=str(name),where=self)
   def read():
    with Stream.from_data(data,mode='r',name=pathlib.Path(path).name,where=MemoryParent()) as stream:return monobit.load(stream,format=fmt,**read_args)
   if fmt in ['riscos','riscos-xy']:
    from monobit.storage.fontformats.riscos import load_riscos,load_x90y45
    # Direct unchanged loader preserves the explicit optional IntMetrics
    # memory resolver; the public registry otherwise replaces its location.
    def read():
     with Stream.from_data(data,mode='r',name=pathlib.Path(path).name,where=MemoryParent()) as stream:
      value=(load_riscos if fmt=='riscos' else load_x90y45)(stream)
      return [value] if isinstance(value,monobit.Font) else value
  if fmt=='pxl':
   from monobit.storage.fontformats.metafont.pxl import load_pxl
   # The registry's anonymous stream has no parent Location. Supply an
   # explicit memory-only parent; missing optional TFM is an ordinary OSError.
   files={p.name:p.read_bytes() for p in pathlib.Path(path).parent.glob('*.tfm')}
   class MemoryLocation:
    def open(self,name,mode):
     if mode!='r' or str(name) not in files:raise FileNotFoundError(str(name))
     return Stream.from_data(files[str(name)],mode='r',name=str(name),where=self)
   def read():
    with Stream.from_data(data,mode='r',name=pathlib.Path(path).name,where=MemoryLocation()) as stream:return [load_pxl(stream)]
 try:fonts=read()
 except Exception as exc:
  print(json.dumps(dict(format=spec['format'],iterations=loops,glyphs=None,input_bytes=size,read_ns=None,write_ns=None,reader_unavailable=f'{type(exc).__name__}: {exc}',writer_unavailable='Independent reader unavailable for this fixture',oracle='monobit',version=importlib.metadata.version('monobit'))));return
 source_glyphs=sum(len(f.glyphs) for f in fonts)
 if spec['format']=='wyse':fonts=[monobit.Font([monobit.Glyph.from_matrix([row[:8] for row in g.as_matrix()],inklevels=(0,1),codepoint=int(g.codepoint)) for g in fonts[0].glyphs])]
 if spec['format']=='drcs':
  # Same public-model reconstruction validated by the frozen W5.5 crosscheck.
  # The original reader stores a malformed two-value dec_drcs property.
  fonts=[monobit.Font(f.glyphs,encoding='ascii') for f in fonts]
  projection='Discard malformed upstream dec_drcs metadata before oracle writer; original glyphs retained'
 if allow_write:
  try:write(fonts)
  except Exception as exc:allow_write=False;why=f'{type(exc).__name__}: {exc}'
 else:why='No validated independent writer for this native profile'+(': '+spec['notes'] if spec.get('notes') else '')
 for _ in range(3):read()
 start=time.perf_counter_ns()
 for _ in range(loops):read()
 read_ns=(time.perf_counter_ns()-start)/loops;write_ns=None
 if allow_write:
  for _ in range(3):write(fonts)
  start=time.perf_counter_ns()
  for _ in range(loops):write(fonts)
  write_ns=(time.perf_counter_ns()-start)/loops
 print(json.dumps(dict(format=spec['format'],iterations=loops,glyphs=source_glyphs,input_bytes=size,read_ns=read_ns,write_ns=write_ns,writer_unavailable=why,writer_projection=projection,oracle='monobit',version=importlib.metadata.version('monobit'))))

def fixture_metadata(case,binary,primary,log):
 """Read complete models outside timed regions where adapters omit counts."""
 g=case['group'];cmd=None
 if g in ['a1','c1','p3_53','p3_54','b','c2'] or (g=='e2' and case['mode']=='outline'):cmd=[binary,'dump',case['format'],str(primary)]
 elif g in ['a2','c','d']:cmd=[binary,'inspect',case['format'],str(primary)]
 elif g=='x':cmd=[binary,'dump',case['format'],str(primary),json.dumps(case['kwargs'])]
 elif g=='e2' and case['mode']=='image_read':cmd=[binary,'inspect',case['format'],str(primary)]
 meta={};record=None
 if cmd:
  record,output=command(cmd,log);value=json.loads(output)
  fonts=value if isinstance(value,list) else value['fonts'] if 'fonts' in value else [value]
  glyphs=[gl for f in fonts for gl in f['glyphs']]
  meta.update(fonts=len(fonts),glyphs=len(glyphs),pixels=sum(gl.get('width',gl.get('w',0))*gl.get('height',gl.get('h',0)) for gl in glyphs),model_sha256=hashlib.sha256(output.encode()).hexdigest(),model_dump=record['stdout'])
 if primary:
  if primary.is_dir():files=[p for p in primary.iterdir() if p.is_file()]
  elif g=='x' and case['format']=='bmfont':files=[primary]+[primary.parent/p for p in json.loads(primary.read_text())['pages']]
  elif g=='e2' and case['format']=='pilfont':files=[primary,primary.with_suffix('.pbm')]
  elif g=='c2' and case['format'].startswith('riscos') and primary.with_name('IntMetrics').exists():files=[primary,primary.with_name('IntMetrics')]
  else:files=[primary]
  meta['fixture_bytes']=sum(p.stat().st_size for p in files)
  meta['fixture_files']={str(p):sha(p) for p in files}
 return meta,record

JAVA_READER=r'''// Original MIT timing wrapper; all native decoding belongs to the external JARs.
import java.io.*;
import java.nio.file.*;
import processing.core.PFont;
import com.kreative.bitsnpicas.BitmapFont;
import com.kreative.bitsnpicas.importer.SRFontBitmapFontImporter;
public final class Phase5NativeReader {
 static volatile Object retained;
 static Object read(String kind, byte[] bytes) throws Exception {
  return kind.startsWith("vlw") ? new PFont(new ByteArrayInputStream(bytes)) : new SRFontBitmapFontImporter().importFont(bytes);
 }
 static int glyphs(Object font) {
  if (font instanceof PFont) return ((PFont)font).getGlyphCount();
  int count=0;for(BitmapFont f:(BitmapFont[])font)count+=f.characters(false).size();return count;
 }
 public static void main(String[] args) throws Exception {
  javax.imageio.ImageIO.setUseCache(false);
  String kind=args[0];byte[] bytes=Files.readAllBytes(Path.of(args[1]));int loops=Integer.parseInt(args[2]);
  Object font=read(kind,bytes);int count=glyphs(font);
  for(int i=0;i<5;i++)retained=read(kind,bytes);
  long start=System.nanoTime();for(int i=0;i<loops;i++)retained=read(kind,bytes);long elapsed=System.nanoTime()-start;
  if(glyphs(retained)!=count)throw new AssertionError("glyph count changed");
  System.out.printf(java.util.Locale.ROOT,"{\"format\":\"%s\",\"iterations\":%d,\"warmup\":5,\"glyphs\":%d,\"input_bytes\":%d,\"read_ns\":%.9f,\"write_ns\":null,\"oracle\":\"%s\",\"scope\":\"original public in-memory font decoder; no timed file IO\"}%n",kind,loops,count,bytes.length,(double)elapsed/loops,kind.startsWith("vlw")?"Processing PFont":"BitsNPicas SRFontBitmapFontImporter");
 }
}
'''

def java_reader(root,steps,env):
 jars=[WORK/'w44-oracles/BitsNPicas.jar',WORK/'w5e2-specs/processing-core-4.5.2.jar']
 if any(not p.is_file() for p in jars) or not shutil.which('javac') or not shutil.which('java'):return None
 dest=root/'java';dest.mkdir();source=dest/'Phase5NativeReader.java';source.write_text(JAVA_READER)
 cp=os.pathsep.join(map(str,jars));steps.append(command(['javac','-cp',cp,'-d',dest,source],root/'logs/java-build',env=env)[0])
 artifacts={str(p):sha(p) for p in [source,dest/'Phase5NativeReader.class',*jars]}
 java=pathlib.Path(shutil.which('java')).resolve();artifacts[str(java)]=sha(java)
 return dict(command=[str(java),'-Djava.awt.headless=true','-Xmx512m','-cp',str(dest)+os.pathsep+cp,'Phase5NativeReader'],artifacts=artifacts)

def freetype_worker(path,loops):
 import freetype,importlib.metadata
 data=pathlib.Path(path).read_bytes()
 def read():
  face=freetype.Face(io.BytesIO(data));result=[]
  for strike in range(face.num_fixed_sizes):
   face.select_size(strike);index=face.get_char_index(65)
   if not index:raise ValueError('Original PFR fixture lost its known U+0041 mapping')
   face.load_glyph(index,freetype.FT_LOAD_DEFAULT);g=face.glyph;b=g.bitmap;buffer=bytes(b.buffer)
   pixels=bytes(255 if buffer[y*b.pitch+x//8]&(128>>(x%8)) else 0 for y in range(b.rows) for x in range(b.width))
   result.append((b.width,b.rows,g.bitmap_left,g.bitmap_top-b.rows,g.advance.x//64,g.advance.y//64,pixels))
  return result
 expected=read()
 source=module(ROOT/'fuzz/seed_w5e2_outline.py');pixels=bytes(255 if p=='X' else 0 for row in source.ROWS for p in row)
 assert expected and all(g==(9,5,-2,-1,11,0,pixels) for g in expected),'FreeType original full raster/metrics mismatch'
 for _ in range(3):assert read()==expected
 start=time.perf_counter_ns()
 for _ in range(loops):read()
 elapsed=time.perf_counter_ns()-start
 print(json.dumps(dict(format='pfr',iterations=loops,warmup=3,glyphs=len(expected),input_bytes=len(data),read_ns=elapsed/loops,write_ns=None,oracle='FreeType via freetype-py',version=freetype.version(),wrapper_version=importlib.metadata.version('freetype-py'),scope='memory Face construction, all strikes, direct known-code glyph loads and full raster/metric materialization; includes Python wrapper overhead',full_original_pixels_verified=True)))

def pillow_worker(path,loops):
 from PIL import Image,ImageFont
 import PIL,struct
 path=pathlib.Path(path);data=path.read_bytes();image_bytes=path.with_suffix('.pbm').read_bytes()
 start_records=data.index(b'DATA\n')+5
 records=[struct.unpack_from('>10h',data,start_records+cp*20) for cp in range(256)]
 count=sum(any(r) for r in records)
 def read():
  image=Image.open(io.BytesIO(image_bytes));image.load();font=ImageFont.ImageFont()
  font._load_pilfont_data(io.BytesIO(data),image)
  return font
 font=read()
 for cp,r in enumerate(records):
  if any(r):assert font.getlength(chr(cp))==r[0]
 for _ in range(5):read()
 start=time.perf_counter_ns()
 for _ in range(loops):read()
 elapsed=time.perf_counter_ns()-start
 print(json.dumps(dict(format='pilfont',iterations=loops,warmup=5,glyphs=count,input_bytes=len(data)+len(image_bytes),read_ns=elapsed/loops,write_ns=None,oracle='Pillow ImageFont',version=PIL.__version__,scope='original Pillow _load_pilfont_data with in-memory decoded companion; native font construction and original advances validated, no timed file IO')))

def oracle_identity():
 import monobit,importlib.metadata
 root=pathlib.Path(monobit.__file__).parent
 packages={}
 for name in ['monobit','Pillow','fonttools','numpy','freetype-py']:
  try:packages[name]=importlib.metadata.version(name)
  except importlib.metadata.PackageNotFoundError:packages[name]=None
 extras={};versions={}
 for name in ['PIL','freetype']:
  try:
   mod=__import__(name);folder=pathlib.Path(mod.__file__).parent
   extras.update({str(p.resolve()):sha(p) for p in sorted(folder.rglob('*')) if p.is_file() and '__pycache__' not in p.parts})
   if name=='freetype':
    versions['freetype']=mod.version();binary=pathlib.Path(mod.raw._lib._name)
    if binary.is_file():extras[str(binary.resolve())]=sha(binary)
  except ImportError:versions[name]=None
 print(json.dumps(dict(executable=sys.executable,executable_sha256=sha(sys.executable),python=sys.version,packages=packages,native_versions=versions,additional_sources=extras,monobit=importlib.metadata.version('monobit'),root=str(root),sources={str(p.relative_to(root)):sha(p) for p in sorted(root.rglob('*')) if p.is_file() and '__pycache__' not in p.parts})))

def verify_freeze(path,digest):
 path=ROOT/path
 if sha(path)!=digest:raise ValueError('group freeze proof changed: '+str(path))
 if path.suffix=='.sha256' or path.name.endswith('-sha256.txt'):
  sources={}
  for line in path.read_text().splitlines():
   if not line.strip():continue
   expected,name=line.split(maxsplit=1)
   if len(expected)!=64 or any(c not in '0123456789abcdef' for c in expected):raise ValueError('invalid source checksum in '+str(path))
   sources[name.removeprefix('*')]=expected
 elif path.suffix=='.json':
  value=json.loads(path.read_text());sources=value.get('owned_files',value.get('owned_sources',value.get('sources',value.get('source_hashes'))))
  if sources is None and value and all(isinstance(v,dict) and 'sha256' in v for v in value.values()):sources=value
  if sources is None:raise ValueError('unrecognized owned-source freeze schema: '+str(path))
 else:return
 rows=sources.items() if isinstance(sources,dict) else ((row['path'],row['sha256']) for row in sources)
 for name,expected in rows:
  if isinstance(expected,dict):expected=expected['sha256']
  if sha(ROOT/name)!=expected:raise ValueError('owned source differs from group freeze: '+name)

def competing_processes():
 """Conservative coordinator guard; CPU utilization remains in the run note."""
 found=[]
 for p in pathlib.Path('/proc').iterdir():
  if not p.name.isdecimal() or int(p.name)==os.getpid():continue
  try:
   argv=(p/'cmdline').read_bytes().split(b'\0');comm=(p/'comm').read_text().strip()
   text=b' '.join(argv).decode(errors='replace')
   if comm in ['bash','sh','timeout']:continue
   cwd=str((p/'cwd').resolve())
   local=str(ROOT) in text or str(WORK) in text or cwd.startswith((str(ROOT),str(WORK)))
   runner_busy='phase5_native.py' in text and any(a in [b'prepare',b'smoke',b'measure'] for a in argv)
   if local and (runner_busy or comm in ['cargo','rustc','rustdoc','ninja','make','cc1','cc1plus','java'] or '/examples/' in text or 'crosscheck/' in text or '-max_total_time=' in text):found.append(dict(pid=int(p.name),comm=comm))
  except (OSError,ProcessLookupError):pass
 return found

def identities():
 paths=[ROOT/'Cargo.toml',ROOT/'Cargo.lock',ROOT/'crates/fontpixel/Cargo.toml',SCRIPT]
 paths+=list((ROOT/'crates/fontpixel/src').rglob('*.rs'))
 paths+=list((ROOT/'crates/fontpixel/examples').glob('*.rs'))
 paths+=list((ROOT/'tests/crosscheck').glob('*.py'))
 paths+=list((ROOT/'fuzz').glob('seed_w5*.py'))
 return {str(p.relative_to(ROOT)):sha(p) for p in sorted(set(paths)) if p.is_file()}

def environment():
 cpu={};mem={}
 for line in pathlib.Path('/proc/cpuinfo').read_text().splitlines():
  if ':' in line:
   k,v=map(str.strip,line.split(':',1))
   if k in ['model name','vendor_id','cpu family','model','stepping','microcode']:cpu.setdefault(k,v)
 for line in pathlib.Path('/proc/meminfo').read_text().splitlines():
  k,v=line.split(':',1)
  if k in ['MemTotal','MemAvailable','SwapTotal']:mem[k]=v.strip()
 cores=set()
 for item in pathlib.Path('/proc/cpuinfo').read_text().split('\n\n'):
  fields=dict(line.split(':',1) for line in item.splitlines() if ':' in line);fields={k.strip():v.strip() for k,v in fields.items()}
  if 'core id' in fields:cores.add((fields.get('physical id'),fields['core id']))
 def version(cmd):
  p=subprocess.run(cmd,capture_output=True,text=True);return {'command':cmd,'exit':p.returncode,'output':(p.stdout+p.stderr).strip()}
 return dict(platform=platform.platform(),cpu=cpu,logical_cpus=os.cpu_count(),physical_cores=len(cores) or None,affinity=sorted(os.sched_getaffinity(0)),memory=mem,loadavg=os.getloadavg(),rustc=version(['rustc','-Vv']),python=sys.version,cargo_profile='release',rustflags=os.environ.get('RUSTFLAGS'),cargo_encoded_rustflags=os.environ.get('CARGO_ENCODED_RUSTFLAGS'),timestamp_utc=datetime.datetime.now(datetime.timezone.utc).isoformat())

def prepare(args):
 args.manifest=args.manifest.resolve();fresh(args.manifest);root=args.manifest.parent/args.manifest.stem;fresh(root);root.mkdir(parents=True)
 groups=args.groups.split(',') if args.groups else list(FEATURES)
 unknown=set(groups)-FEATURES.keys()
 if unknown:raise ValueError('unknown groups '+str(unknown))
 target=pathlib.Path(args.target).resolve();env=os.environ.copy();env.update(CARGO_TARGET_DIR=str(target),CARGO_BUILD_JOBS='2',CARGO_INCREMENTAL='0',CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0',PYTHONDONTWRITEBYTECODE='1')
 initial_sources=identities();steps=[];jobs=[];pending={g:PENDING[g] for g in groups if g in PENDING};active=[g for g in groups if g not in PENDING]
 freeze_evidence={};unfrozen=[];freeze_errors={}
 for group in sorted(set(groups)&FREEZE_REQUIRED):
  evidence=GROUP_FREEZES.get(group)
  if not evidence:unfrozen.append(group);continue
  try:
   for path,digest in evidence.items():verify_freeze(path,digest)
  except (ValueError,OSError) as error:
   if not args.smoke_only:raise
   unfrozen.append(group);freeze_errors[group]=str(error)
  freeze_evidence[group]=evidence
 examples=[n for g in active for n in EXAMPLES[g]]
 if not args.skip_build:
  cmd=['cargo','build','--locked','--release','-p','fontpixel','--no-default-features','--features',','.join(sorted({f for g in active for f in FEATURES[g].split(',')}))]
  for n in examples:cmd+=['--example',n]
  if examples:steps.append(command(cmd,root/'logs/build',env=env)[0])
 record,output=command([args.python,str(SCRIPT),'oracle-identity'],root/'logs/oracle-identity',env=env);steps.append(record);oracle=json.loads(output)
 java=java_reader(root,steps,env) if 'e2' in active else None
 for g in active:
  binaries=[str(target/'release/examples'/n) for n in EXAMPLES[g]]
  for binary in binaries:
   if not pathlib.Path(binary).is_file():raise ValueError('missing release adapter '+binary)
  dest=root/'fixtures'/g
  cmd=[args.python,str(SCRIPT),'generate',g,str(dest),'--binaries',json.dumps(binaries),'--python',args.python]
  steps.append(command(cmd,root/('logs/generate-'+g),env=env)[0])
  for case in [x.copy() for x in catalog() if x['group']==g]:
   primary=dest/case['source'] if case['source'] else None
   if primary is not None and not primary.exists():raise ValueError('missing generated fixture '+str(primary))
   if case['mode']=='device_read':cmd=[binaries[1],case['format'],str(primary),'{iterations}'];parser='ns_per_op'
   elif case['mode']=='device_write':cmd=[binaries[0],'bench','256','{iterations}'];parser='e1_tsv'
   elif case['mode']=='color':cmd=[binaries[0],'timing','4096'];parser=case['parser']
   elif case['mode']=='image_read':cmd=[binaries[0],'bench-read',case['format'],str(primary),'{iterations}'];parser=case['parser']
   elif case['mode']=='image_write':cmd=[binaries[0],'bench','100'];parser=case['parser']
   elif case['mode']=='outline':cmd=[binaries[1],'bench',case['format'],str(primary),'{iterations}'];parser=case['parser']
   elif case['mode']=='transport':cmd=[binaries[0],'{iterations}','256'];parser=case['parser']
   else:
    cmd=[binaries[0],case['mode'],case['format'],str(primary)];parser=case['parser']
    if g=='x':cmd.append(json.dumps(case['kwargs'],separators=(',',':')))
    if parser in ['ns_total','ns_per_op','seconds_total']:cmd.append('{iterations}')
   engines=[dict(name='rust',command=cmd,parser=parser,binary_sha256=sha(cmd[0]))]
   if g in ['a1','x']:
    cmd=[args.python,str(ROOT/f'tests/crosscheck/w5{g}.py'),'--bench',case['format'],'--file',str(primary),'--count','{iterations}']
    if g=='x':cmd+=['--config',json.dumps(case['kwargs'])]
    engines.append(dict(name='python',command=cmd,parser='ns_total'))
   elif g in ['a2','c1','p3_53'] or (g=='p3_52' and case['format']!='syllables') or (g=='p3_54' and case['format']=='palm') or (g=='c' and case['format'] in ['drcs','netbsd']) or (g=='d' and case['format'] in ['fbit','mgtk','lisa','binhex']) or (g in ['b','c2','e2'] and case.get('oracle_format')):
    engines.append(dict(name='python',command=[args.python,str(SCRIPT),'monobit',json.dumps(case),str(primary),'{iterations}'],parser='ns_per_op'))
   if g=='e2' and case['mode']=='image_read' and java and (case['format'].startswith('vlw') or case['format'] in ['sfont','rfont']):
    engines.append(dict(name='java',command=java['command']+[case['format'],str(primary),'{iterations}'],parser='ns_per_op',artifact_hashes=java['artifacts']))
   if g=='e2' and case['format']=='pilfont':
    engines.append(dict(name='python',command=[args.python,str(SCRIPT),'pillow',str(primary),'{iterations}'],parser='ns_per_op'))
   if g=='e2' and case.get('freetype'):
    engines.append(dict(name='native',command=[args.python,str(SCRIPT),'freetype',str(primary),'{iterations}'],parser='ns_per_op'))
   metadata,record=fixture_metadata(case,binaries[1] if case['mode']=='outline' else binaries[0],primary,root/('logs/inspect-'+g+'-'+case['id']))
   if record:steps.append(record)
   case.update(fixture_metadata=metadata,primary=str(primary) if primary else None,engines=engines,missing_baselines={k:'No verified, comparable, memory-only timing adapter available' for k in ['python','java','native'] if k not in {e['name'] for e in engines}})
   jobs.append(case)
 assets={str(p):{'sha256':sha(p),'bytes':p.stat().st_size} for p in sorted((root/'fixtures').rglob('*')) if p.is_file()}
 for group,evidence in freeze_evidence.items():
  try:
   for path,digest in evidence.items():verify_freeze(path,digest)
  except (ValueError,OSError) as error:
   if not args.smoke_only:raise
   if group not in unfrozen:unfrozen.append(group)
   freeze_errors[group]=str(error)
 final_sources=identities()
 source_changes={path:{'before':initial_sources.get(path),'after':final_sources.get(path)} for path in initial_sources.keys()|final_sources.keys() if initial_sources.get(path)!=final_sources.get(path)}
 result=dict(oracle=oracle,formal=False,formal_eligible=not args.skip_build and not args.smoke_only and not unfrozen and not source_changes,explicit_smoke_only=args.smoke_only,group_freeze_evidence=freeze_evidence,freeze_proof_failures=freeze_errors,unfrozen_groups=unfrozen,skip_build_smoke_only=args.skip_build,schema=2,purpose='Preparation only; no formal timing',root=str(root),target=str(target),groups=groups,pending=pending,pending_catalog=PENDING,steps=steps,jobs=jobs,assets=assets,source_hashes=final_sources,source_changes_during_preparation=source_changes,environment=environment())
 result['raw_log_verification']=verify_command_logs(steps)
 dump(args.manifest,result);print(json.dumps({'manifest':str(args.manifest),'jobs':len(jobs),'pending':pending}))

def parse_records(text,parser):
 if parser not in ['ns_per_op','ns_total','us_per_op','seconds_total','seconds_per_op','seconds_split_total','read_ns_total','e1_tsv','e2_tsv','transport_total']:raise ValueError('unknown timing unit contract')
 if parser=='e2_tsv':
  result=[]
  for line in text.splitlines():
   fmt,glyphs,total_bytes,elapsed=line.split('\t')
   if min(int(glyphs),int(elapsed),int(total_bytes))<0:raise ValueError('invalid E2 workload')
   result.append(dict(format=fmt,glyphs=int(glyphs),glyph_count_scope='original source model passed to default writer; separately emitted native read fixtures are different models',iterations=1,read_iterations=None,write_iterations=1,warmup=0,input_bytes=None,output_bytes=int(total_bytes),read_ns=None,write_ns=int(elapsed)))
  if not result:raise ValueError('adapter produced no E2 timing records')
  return result
 if parser=='transport_total':
  value=json.loads(text);result=[]
  for mode in ['raw','gzip']:
   row=dict(value,format=mode,read_ns=value[mode+'_read_ns'],write_ns=value[mode+'_write_ns'],input_bytes=value[mode+'_bytes'])
   result+=parse_records(json.dumps(row),'ns_total')
  return result
 if parser=='e1_tsv':
  result=[]
  for line in text.splitlines():
   fmt,glyphs,loops,elapsed,total_bytes=line.split('\t')
   if int(loops)<1 or min(int(glyphs),int(elapsed),int(total_bytes))<0:raise ValueError('invalid E1 workload')
   result.append(dict(format=fmt,glyphs=int(glyphs),glyph_count_scope='source model passed to writer; native output may add default slots',iterations=int(loops),warmup=0,input_bytes=None,output_bytes=int(total_bytes)/int(loops),read_ns=None,write_ns=int(elapsed)/int(loops)))
  return result
 try:value=json.loads(text);records=value if isinstance(value,list) else [value]
 except json.JSONDecodeError:records=[json.loads(line) for line in text.splitlines() if line.startswith('{')]
 if not records:raise ValueError('adapter produced no timing records')
 out=[]
 for value in records:
  if parser=='read_ns_total':value=dict(value,iterations=value['actual_iterations'],read_ns=value['total_ns'])
  loops=value.get('iterations',value.get('loops',value.get('count',1)))
  if not isinstance(loops,int) or loops<1:raise ValueError('invalid reported iteration count')
  normalized=dict(value);normalized['iterations']=None if parser=='seconds_split_total' else loops
  for op in ['read','write']:
   key=op+('_us' if parser=='us_per_op' else '_seconds' if parser.startswith('seconds_') else '_ns');n=value.get(key)
   op_loops=value.get(op+'_iterations',loops) if parser=='seconds_split_total' else loops
   if not isinstance(op_loops,int) or op_loops<1:raise ValueError('invalid operation iteration count')
   if n is not None:
    if not isinstance(n,(int,float)) or not math.isfinite(n) or n<0:raise ValueError('invalid elapsed time')
    n*=1000 if parser=='us_per_op' else 1e9 if parser.startswith('seconds_') else 1
    if parser.endswith('_total'):n/=op_loops
   normalized[op+'_ns']=n
   normalized[op+'_iterations']=op_loops if n is not None else None
  out.append(normalized)
 return out

def verify_manifest(manifest,strict=True):
 verify_command_logs(manifest.get('steps',[]))
 changes=[]
 oracle=manifest.get('oracle',{})
 for path,h in oracle.get('sources',{}).items():
  if sha(pathlib.Path(oracle['root'])/path)!=h:raise ValueError('Independent oracle source changed: '+path)
 if oracle and sha(oracle['executable'])!=oracle['executable_sha256']:raise ValueError('Independent Python executable changed')
 for path,digest in oracle.get('additional_sources',{}).items():
  if sha(path)!=digest:raise ValueError('Pillow/FreeType source or native library changed: '+path)
 for path,meta in manifest['assets'].items():
  if sha(path)!=meta['sha256']:raise ValueError('fixture changed: '+path)
 for path,h in manifest['source_hashes'].items():
  if not (ROOT/path).is_file() or sha(ROOT/path)!=h:changes.append(path)
 for path in identities().keys()-manifest['source_hashes'].keys():changes.append('new source: '+path)
 for job in manifest['jobs']:
  for engine in job['engines']:
   if engine['name']=='rust' and sha(engine['command'][0])!=engine['binary_sha256']:raise ValueError('release binary changed')
   for path,digest in engine.get('artifact_hashes',{}).items():
    if sha(path)!=digest:raise ValueError('external native adapter or JAR changed: '+path)
 for evidence in manifest.get('group_freeze_evidence',{}).values():
  for path,digest in evidence.items():
   try:verify_freeze(path,digest)
   except (ValueError,OSError) as error:
    if strict:raise
    changes.append('freeze proof: '+str(error))

 if changes and strict:raise ValueError('source changed since preparation: '+str(changes))
 return changes

def measure(args,formal):
 args.manifest=args.manifest.resolve();args.output=args.output.resolve()
 fresh(args.output);manifest=json.loads(args.manifest.read_text());changes=verify_manifest(manifest,formal)
 if formal and not manifest.get('formal_eligible',False):raise ValueError('Preparation is smoke-only, lacks final group freezes, used --skip-build, or sources changed during build; prepare again before formal measurements')
 if formal and (not args.quiet_window_confirmed or not args.environment_note):raise ValueError('Formal measurement requires the coordinator-confirmed quiet window and environment note')
 competitors=competing_processes()
 if formal and competitors:raise ValueError('Task-owned build/fuzz/oracle activity prevents formal measurement: '+str(competitors))
 before_environment=environment()
 root=args.output.parent/args.output.stem;fresh(root);root.mkdir(parents=True);records=[];command_runs=[]
 selected=[j for j in manifest['jobs'] if not args.cases or j['group']+':'+j['id'] in args.cases.split(',')]
 if not selected:raise ValueError('No selected cases')
 if args.cases and set(args.cases.split(','))!={j['group']+':'+j['id'] for j in selected}:raise ValueError('Unknown case in --cases')
 for job in selected:
  for engine in job['engines']:
   engine_runs=[]
   for repetition in range(5 if formal else 1):
    cmd=[str(args.iterations) if a=='{iterations}' else a for a in engine['command']]
    record,text=command(cmd,root/f"{job['group']}-{job['id']}-{engine['name']}-{repetition+1}")
    record.update(repetition=repetition+1,records=parse_records(text,engine['parser']));engine_runs.append(record);command_runs.append(record)
   for slot in range(len(engine_runs[0]['records'])):
    if any(len(r['records'])!=len(engine_runs[0]['records']) for r in engine_runs):raise ValueError('Adapter changed record count between repetitions')
    samples=[r['records'][slot] for r in engine_runs];metrics={}
    for op in ['read_ns','write_ns']:
     values=[r[op] for r in samples]
     if any(v is None for v in values) and not all(v is None for v in values):raise ValueError('Adapter availability changed between repetitions')
     metrics[op]=None if all(v is None for v in values) else dict(median=statistics.median(values),min=min(values),max=max(values))
    if any(any(r.get(k)!=samples[0].get(k) for k in ['iterations','read_iterations','write_iterations','glyphs']) for r in samples):raise ValueError('Adapter workload changed between repetitions')
    records.append(dict(case=job['group']+':'+job['id'],engine=engine['name'],record_index=slot,format=samples[0].get('format',samples[0].get('flavor',job['format'])),metrics=metrics,metadata=samples[0],fixture_metadata=job.get('fixture_metadata'),peak_rss_kib=max(r['peak_rss_kib'] for r in engine_runs),median_peak_rss_kib=statistics.median(r['peak_rss_kib'] for r in engine_runs),runs=engine_runs,notes=job['notes']))
    with (root/'progress.jsonl').open('a') as progress:progress.write(json.dumps(records[-1],ensure_ascii=False)+'\n')
 changes=sorted(set(changes+verify_manifest(manifest,formal)))
 after_competitors=competing_processes()
 if formal and after_competitors:raise ValueError('Task-owned activity detected after measurement; raw records retained but not formal: '+str(after_competitors))
 result=dict(source_changes_during_busy_smoke=changes,competing_processes_before=competitors,competing_processes_after=after_competitors,environment_before=before_environment,schema=1,purpose='quiet five-run release measurements' if formal else 'interface smoke only; not performance evidence',formal=formal,repetitions=5 if formal else 1,environment_note=args.environment_note,environment=environment(),manifest=str(args.manifest),manifest_sha256=sha(args.manifest),source_hashes=manifest['source_hashes'],results=records,pending=manifest['pending'],pending_catalog=manifest.get('pending_catalog',PENDING),missing_baselines={j['group']+':'+j['id']:j['missing_baselines'] for j in selected},ratios=None,ratio_policy='No automatic speedup ratios: adapters may project metadata, native defaults, or writer behavior differently; compare explicitly supported matching operations only')
 result['comparison_columns']={j['group']+':'+j['id']:{engine:([dict(format=r['format'],read_ns=r['metrics']['read_ns'],write_ns=r['metrics']['write_ns'],peak_rss_kib=r['peak_rss_kib'],iterations=r['metadata']['iterations'],read_iterations=r['metadata'].get('read_iterations'),write_iterations=r['metadata'].get('write_iterations')) for r in records if r['case']==j['group']+':'+j['id'] and r['engine']==engine] or None) for engine in ['rust','python','java','native']} for j in selected}
 result['raw_log_verification']=verify_command_logs(command_runs)
 dump(args.output,result);print(json.dumps({'output':str(args.output),'formal':formal,'records':len(records)}))

def log_selfcheck():
 """Real child processes reproduce the dotted case / five-repeat collision."""
 with tempfile.TemporaryDirectory(prefix='phase5-log-regression-') as directory:
  root=pathlib.Path(directory);records=[]
  for extension in ['sfn','asc']:
   for repetition in range(1,6):
    token=f'{extension}-{repetition}';log=root/f'e2-ssfn-original.{extension}-rust-{repetition}'
    record,output=command([sys.executable,'-c','import sys; print(sys.argv[1]); print(sys.argv[1],file=sys.stderr)',token],log)
    assert output==token+'\n'
    assert pathlib.Path(record['stdout']).name==log.name+'.stdout'
    assert pathlib.Path(record['stderr']).read_text()==token+'\n'
    records.append(record)
  assert verify_command_logs(records)==dict(commands=10,files=20,all_sha256_match=True)
  marker=root/'must-not-launch';first=root/'e2-ssfn-original.sfn-rust-1'
  before={str(p):p.read_bytes() for p in log_paths(first)}
  try:command([sys.executable,'-c','import pathlib,sys; pathlib.Path(sys.argv[1]).touch()',marker],first)
  except FileExistsError:pass
  else:raise AssertionError('existing logs overwritten')
  assert not marker.exists() and all(pathlib.Path(p).read_bytes()==value for p,value in before.items())
  stderr_only=root/'stderr-only.sfn';out,err=log_paths(stderr_only);err.write_bytes(b'preserve')
  try:command([sys.executable,'-c','raise AssertionError("must not launch")'],stderr_only)
  except FileExistsError:pass
  else:raise AssertionError('existing stderr overwritten')
  assert err.read_bytes()==b'preserve' and not out.exists()
  for stream in ['stdout','stderr']:
   path=pathlib.Path(records[0][stream]);original=path.read_bytes();path.write_bytes(original+b'tampered')
   try:verify_command_logs(records)
   except ValueError as error:assert 'raw '+stream+' log hash changed' in str(error)
   else:raise AssertionError('tampered raw log accepted')
   path.write_bytes(original)
  try:verify_command_logs([records[0],records[0]])
  except ValueError as error:assert 'reused raw log path' in str(error)
  else:raise AssertionError('duplicate invocation log accepted')
  assert verify_command_logs(records)['all_sha256_match']
 return dict(checks=6,child_processes=10,scenarios=['distinct .sfn/.asc stems across five repetitions','same-path refusal before child launch','stderr-only conflict preserves both paths','modified stdout rejected','modified stderr rejected','duplicate invocation paths rejected'])

def selfcheck():
 """Unit contracts matter: each frozen adapter reports a different scale."""
 for parser,value,expected in [
  ('ns_total',{'count':4,'read_ns':120,'write_ns':None},30),
  ('ns_per_op',{'iterations':4,'read_ns':120},120),
  ('us_per_op',{'iterations':300,'read_us':2.5},2500),
  ('seconds_total',{'iterations':20,'read_seconds':0.5},25_000_000),
  ('seconds_per_op',{'read_seconds':0.5},500_000_000),
 ]:
  actual=parse_records(json.dumps(value),parser)[0]
  assert actual['read_ns']==expected,(parser,actual)
  assert actual['write_ns'] is None
 value=parse_records('cybiko\t190\t7\t7000\t14000\n','e1_tsv')[0]
 assert value['write_ns']==1000 and value['iterations']==7 and value['glyphs']==190 and value['output_bytes']==2000
 assert len(parse_records('{"read_seconds":1}\n{"read_seconds":2}\n','seconds_per_op'))==2
 value=parse_records('{"read_iterations":100,"read_seconds":2,"write_iterations":20,"write_seconds":0.1}','seconds_split_total')[0]
 assert value['iterations'] is None and value['read_iterations']==100 and value['write_iterations']==20
 assert value['read_ns']==20_000_000 and value['write_ns']==5_000_000
 value=parse_records('{"actual_iterations":5,"total_ns":100,"glyphs":94}','read_ns_total')[0]
 assert value['read_ns']==20 and value['write_ns'] is None and value['read_iterations']==5
 value=parse_records('sfont\t94\t500\t12345\n','e2_tsv')[0]
 assert value['iterations']==1 and value['write_ns']==12345 and value['glyphs']==94
 values=parse_records('{"iterations":2,"raw_read_ns":10,"raw_write_ns":20,"gzip_read_ns":30,"gzip_write_ns":40,"raw_bytes":64,"gzip_bytes":80}','transport_total')
 assert [(r['read_ns'],r['write_ns']) for r in values]==[(5,10),(15,20)]
 cases=catalog();keys=[(c['group'],c['id']) for c in cases];assert len(keys)==len(set(keys))
 for text,parser in [('{"read_ns":-1}','ns_total'),('{"read_ns":NaN}','ns_per_op'),('{"iterations":0}','ns_total'),('x\t1\t0\t1\t1','e1_tsv')]:
  try:parse_records(text,parser)
  except ValueError:pass
  else:raise AssertionError('invalid measurement accepted')
 log_checks=log_selfcheck()
 print(json.dumps(dict(status='pass',checks=16+log_checks['checks'],unit_checks=16,raw_log_regressions=log_checks,cases=len(cases),purpose='unit normalization and real raw-log integrity regressions; no font performance measurements')))

def main():
 p=argparse.ArgumentParser(description=__doc__);sub=p.add_subparsers(dest='action',required=True)
 sub.add_parser('inventory')
 sub.add_parser('oracle-identity')
 sub.add_parser('selfcheck')
 q=sub.add_parser('prepare');q.add_argument('--manifest',type=pathlib.Path,required=True);q.add_argument('--target',default=str(WORK/'target-phase5-performance'));q.add_argument('--python',default=str(WORK/'oracles/bin/python'));q.add_argument('--groups');q.add_argument('--skip-build',action='store_true');q.add_argument('--smoke-only',action='store_true')
 for name in ['smoke','measure']:
  q=sub.add_parser(name);q.add_argument('--manifest',type=pathlib.Path,required=True);q.add_argument('--output',type=pathlib.Path,required=True);q.add_argument('--cases');q.add_argument('--iterations',type=int,default=100 if name=='measure' else 1);q.add_argument('--environment-note',default='');q.add_argument('--quiet-window-confirmed',action='store_true')
 q=sub.add_parser('generate');q.add_argument('group');q.add_argument('directory',type=pathlib.Path);q.add_argument('--binaries',required=True);q.add_argument('--python',required=True)
 q=sub.add_parser('monobit');q.add_argument('spec');q.add_argument('path');q.add_argument('loops',type=int)
 for name in ['freetype','pillow']:
  q=sub.add_parser(name);q.add_argument('path');q.add_argument('loops',type=int)
 a=p.parse_args()
 if a.action=='inventory':print(json.dumps({'groups':FEATURES,'pending':PENDING,'freeze_required':sorted(FREEZE_REQUIRED),'group_freeze_evidence':GROUP_FREEZES,'cases':catalog()},indent=2))
 elif a.action=='oracle-identity':oracle_identity()
 elif a.action=='selfcheck':selfcheck()
 elif a.action=='generate':generated(a.group,a.directory,json.loads(a.binaries),a.python)
 elif a.action=='monobit':monobit_worker(json.loads(a.spec),a.path,a.loops)
 elif a.action=='freetype':freetype_worker(a.path,a.loops)
 elif a.action=='pillow':pillow_worker(a.path,a.loops)
 elif a.action=='prepare':prepare(a)
 else:
  if a.iterations<1:p.error('iterations must be positive')
  measure(a,a.action=='measure')
if __name__=='__main__':main()
