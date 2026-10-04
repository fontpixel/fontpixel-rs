#!/usr/bin/env python3
"""Original MIT C2 fixtures and independent monobit 0.54 oracle comparisons."""
import argparse,hashlib,importlib.util,json,pathlib,subprocess,sys
import monobit
ROOT=pathlib.Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('c2seed',ROOT/'fuzz/seed_w5c2.py');seed=importlib.util.module_from_spec(spec);spec.loader.exec_module(seed)
def digest(b):return hashlib.sha256(b).hexdigest()
def run(adapter,*args):
 p=subprocess.run([str(adapter),*map(str,args)],capture_output=True,text=True)
 assert p.returncode==0,(args,p.returncode,p.stderr)
 return p.stdout
def profile(fmt,label):
 return fmt+(':'+label.split('-')[0] if fmt=='xerox' else ':data' if fmt=='catalyst' and label=='data' else ':'+label.replace('-stride3',':stride3') if fmt=='chiwriter' and 'stride' in label else ':legacy' if fmt=='hppcl' and label=='legacy' else '')
def native(adapter,fmt,path):
 result=[]
 raw=run(adapter,'dump',fmt,path);path.with_name(path.name+'.fontpixel.json').write_text(raw)
 for f in json.loads(raw)['fonts']:
  out={}
  for g in f['glyphs']:
   pts=[];stride=(g['w']+7)//8 if g['depth']==1 else g['w']
   for y in range(g['h']):
    for x in range(g['w']):
     value=(255 if g['bytes'][y*stride+x//8]&(128>>(x%8)) else 0) if g['depth']==1 else g['bytes'][y*stride+x]
     if value:pts.append((x+g['x'],g['h']-1-y+g['y'],value))
   keys=g['codes'] or [g['name']]
   for key in keys:out[key]={'rect':[g['w'],g['h'],g['x'],g['y']]if g['w'] and g['h'] else None,'points':sorted(pts),'advance':g['advance'],'scalable':g['scalable']}
  result.append(out)
 return result
def external(font,fmt):
 out={}
 for g in font.glyphs:
  if fmt=='prebuilt':key=g.tags[0].value
  elif not g.codepoint:key='dummy'
  else:key=int(g.codepoint)
  if fmt=='alto' and not g.height:continue
  yy=g.shift_up-(1 if fmt=='alto' else 0)
  pts=sorted((x+g.left_bearing,g.height-1-y+yy,int(v)*255//(font.levels-1))for y,row in enumerate(g.as_matrix(inklevels=tuple(range(font.levels))))for x,v in enumerate(row)if v)
  out[key]={'rect':[g.width,g.height,g.left_bearing,yy]if g.width and g.height else None,'points':pts,'advance':[g.advance_width,0]}
 return out
def oracle_name(fmt,label):return label.split('-')[0] if fmt=='xerox' else 'riscos-xy' if fmt=='riscos' and label.startswith('xy') else fmt
def compare(a,b,fmt):
 # Oracle ignores vertical advances in Prebuilt/AC. AC exposes 16-row padded bbox.
 def comparable(g):
  return {'points':g['points'],'advance_x':g['advance'][0],**({'rect':g['rect']}if fmt!='prepress' else {})}
 x={k:comparable(v)for k,v in a.items()};y={k:comparable(v)for k,v in b.items()}
 assert x==y,(fmt,[(k,x.get(k),y.get(k))for k in x.keys()|y.keys()if x.get(k)!=y.get(k)][:1])
def prebuilt_big_endian(data):
 """Swap numeric fields only; preserve original producer's MSB raster bytes."""
 import struct
 if data[:2]!=b'\0\0':return data
 out=bytearray(data);u16=lambda a:struct.unpack_from('<H',data,a)[0];u32=lambda a:struct.unpack_from('<I',data,a)[0]
 n,m=u16(16),u16(18)
 for at in [0,2,16,18]:struct.pack_into('>H',out,at,1 if at==0 else u16(at))
 for at in [4,8,12,20,24,28,32,36,40,44,48]:struct.pack_into('>I',out,at,u32(at))
 for base,count in [(u32(20),n*2),(u32(32),m*7),(u32(40),n*2 if u32(40) else 0)]:
  for i in range(count):struct.pack_into('>I',out,base+i*4,u32(base+i*4))
 for i in range(n*m):
  at=u32(36)+i*12;struct.pack_into('>H',out,at+6,u16(at+6));struct.pack_into('>I',out,at+8,u32(at+8))
 return bytes(out)

def verify(adapter,prof,source,oname,kwargs):
 ours=native(adapter,prof,source);oracle=source
 if oname=='prebuilt' and source.read_bytes()[:2]==b'\0\0':
  oracle=source.with_name(source.name+'.oracle-be.bepf');oracle.write_bytes(prebuilt_big_endian(source.read_bytes()))
 fonts=monobit.load(oracle,format=oname,**kwargs)
 source.with_name(source.name+'.monobit.json').write_text(json.dumps([{'ascent':f.ascent,'descent':f.descent,'line_height':f.line_height,'point_size':str(f.point_size),'dpi':str(f.dpi),'glyphs':{str(k):v for k,v in external(f,oname).items()}}for f in fonts],indent=2)+'\n')
 assert len(ours)==len(fonts),(prof,'font count',len(ours),len(fonts))
 for a,b in zip(ours,fonts):compare(a,external(b,oname),oname)
 return sum(map(len,ours))
def pcl_pixels(data):
 import re,struct
 records={};cp=None;at=0;current=None
 while at<len(data):
  m=re.match(rb"\x1b([()*])([a-z])([0-9]+)([EW])",data[at:]);assert m,(at,data[at:at+20]);at+=m.end();prefix=m[1]+m[2];n=int(m[3])
  if m[4]==b'E':cp=n;continue
  payload=data[at:at+n];assert len(payload)==n;at+=n
  if prefix==b')s':continue
  assert prefix==b'(s' and payload[0]==4
  if payload[1]:assert current is not None;records[current][1]+=payload[2:]
  else:current=cp;records[cp]=[payload[:16],bytearray(payload[16:])]
 out={}
 for cp,(head,raw) in records.items():
  orientation=head[4];left,top,w,h,advance=struct.unpack('>hhHHh',head[6:16]);rows=[]
  if head[3]==1:
   stride=(w+7)//8;assert len(raw)==stride*h
   rows=[[bool(raw[y*stride+x//8]&(128>>(x%8)))for x in range(w)]for y in range(h)]
  else:
   assert head[3]==2;at=0
   while len(rows)<h:
    repeats=raw[at]+1;at+=1;row=[];black=False
    while len(row)<w:
     n=raw[at];at+=1;row.extend([black]*n);black=not black
    assert len(row)==w;rows.extend([row]*repeats)
   assert len(rows)==h and at==len(raw)
  pts=[]
  for y,row in enumerate(rows):
   for x,ink in enumerate(row):
    if ink:xx,yy=[(left+x,top-y),(top-y,-left-x),(-left-x,-top+y),(-top+y,left+x)][orientation];pts.append((xx,yy,255))
  out[cp]={'points':sorted(pts),'advance':advance//4}
 return out

def bitblt_pixels(data):
 import struct
 u=lambda a:struct.unpack_from('>H',data,a)[0]
 flags,first,last=u(0),u(2),u(4);n=last-first+1;kerned=bool(flags&0x1000);at=16 if kerned else 8;left=struct.unpack_from('>h',data,8)[0] if kerned else 0
 if flags&0x4000:
  bodies_count=u(at+4);mapping=[u(at+6+2*i)for i in range(n+1)];at+=6+2*(n+1)
 else:bodies_count=1;mapping=list(range(n+1))
 bodies=[]
 for _ in range(bodies_count):
  size=u(at)*2;asc,desc,row=u(at+2),u(at+4),u(at+8)*2;h=asc+desc;end=at+10+row*h;count=(at+size-end)//2-1;offsets=[u(end+2*i)for i in range(count+1)];bodies.append((at+10,row,h,desc,offsets));at+=size
 out={}
 for i,index in enumerate(mapping):
  if index&0x8000:continue
  body,cp=(index>>8,index&255)if flags&0x4000 else(0,index);start,row,h,desc,offsets=bodies[body];a,b=offsets[cp:cp+2];width=b-a
  if kerned:
   offset,advance=data[at+2*i:at+2*i+2]
   if (offset,advance)==(255,255):continue
   x=left+offset
  else:x,advance=0,width
  points=sorted((xx-a+x,h-1-y-desc,255)for y in range(h)for xx in range(a,b)if data[start+y*row+xx//8]&(128>>(xx%8)))
  out['dummy' if i==n else first+i]={'points':points,'rect':[width,h,x,-desc]if width and h else None,'advance':[advance,0]}
 return out

def main():
 p=argparse.ArgumentParser();p.add_argument('--adapter',type=pathlib.Path,required=True);p.add_argument('--artifacts',type=pathlib.Path,required=True);p.add_argument('--output',type=pathlib.Path,required=True);a=p.parse_args();a.adapter=a.adapter.resolve();a.artifacts.mkdir(parents=True,exist_ok=True);cases=[]
 for fmt,label,data in seed.cases():
  d=a.artifacts/(fmt+'-'+label);d.mkdir(exist_ok=True);source=d/'source.bin';source.write_bytes(data);prof=profile(fmt,label);oname=oracle_name(fmt,label)
  kwargs={'filetype':16 if data[0]==16 else 17}if fmt=='chiwriter' else {}
  ours=native(a.adapter,prof,source);out=d/'native.bin';run(a.adapter,'write',prof,source,out);back=native(a.adapter,prof,out)
  for x,y in zip(ours,back):compare(x,y,'prepress' if fmt=='xerox' and label=='prepress' else fmt)
  result={'format':fmt,'fixture':label,'glyphs':sum(map(len,ours)),'native_read_write':True,'source_sha256':digest(data),'native_sha256':digest(out.read_bytes())}
  excluded=(fmt=='catalyst' and label!='dense')or(fmt=='chiwriter' and 'stride' in label)or(fmt=='riscos' and label in ['v7','v8','phases','wide'])or(fmt=='hppcl' and label=='rle')or(fmt=='dashen' and label=='v4')
  if not excluded:
   verify(a.adapter,prof,source,oname,kwargs);verify(a.adapter,prof,out,oname,kwargs);result['independent_reader_source_and_native_output']=True
   if fmt=='prebuilt' and data[:2]==b'\0\0':result['oracle_numeric_endian_normalization']=True
  else:result['native_variant_fixture_without_external_reader']=True
  cases.append(result);print(fmt,label,'PASS',flush=True)
 # Independent byte-level PCL renderer from HP's RLE/coordinate definitions.
 extra=[];d=a.artifacts/'hppcl-independent';d.mkdir(exist_ok=True);src=d/'source.bin';src.write_bytes(seed.hppcl());original=native(a.adapter,'hppcl',src)[0]
 for orientation in range(4):
  for compression in ['raw','rle']:
   prof=f'hppcl:o{orientation}:{compression}:chunks';out=d/f'native-{orientation}-{compression}.bin';run(a.adapter,'write',prof,src,out);actual=pcl_pixels(out.read_bytes())
   assert actual=={cp:{'points':g['points'],'advance':g['advance'][0]}for cp,g in original.items()}
   extra.append({'profile':prof,'independent_hp_spec_decoder':True,'sha256':digest(out.read_bytes())})
 # All four BITBLT layouts are decoded independently from the primary body/index tables.
 xd=a.artifacts/'xerox-independent';xd.mkdir(exist_ok=True);xs=xd/'source.bin';xs.write_bytes(seed.bitblt(True))
 indexed=[]
 for layout in ['plain','kerned','plain-index','kerned-index']:
  xp=xd/f'{layout}.bin';run(a.adapter,'write','xerox:'+layout,xs,xp);compare(native(a.adapter,'xerox:bitblt',xp)[0],bitblt_pixels(xp.read_bytes()),'bitblt');indexed.append({'layout':layout,'independent_primary_decoder':True,'sha256':digest(xp.read_bytes())})
 # The independently maintained PCL saver emits historical size=16 descriptors.
 sourcefont=monobit.load(src,format='hppcl')[0]
 for orientation in ['portrait','landscape']:
  out=d/f'monobit-{orientation}.bin';monobit.save(sourcefont,out,format='hppcl',orientation=orientation,overwrite=True);verify(a.adapter,'hppcl:legacy',out,'hppcl',{});extra.append({'profile':orientation,'external_writer_native_reader':True,'sha256':digest(out.read_bytes())})
 # Original NeXT producer outputs from original BDF inputs, compiled only as an external oracle.
 research=ROOT.parent/'local-work/prebuilt-research';official=[]
 for evidence in json.loads((research/'pixel-evidence.json').read_text()):
  name=evidence['name'];d=a.artifacts/('original-prebuild-'+name);d.mkdir(exist_ok=True);source=d/'source.lepf';source.write_bytes((research/(name+'.lepf')).read_bytes());(d/'source.bdf').write_bytes((research/(name+'.bdf')).read_bytes())
  assert digest(source.read_bytes())==evidence['output_sha256'];ours=native(a.adapter,'prebuilt',source)[0];g=next(iter(ours.values()));pixels=evidence['expected_pixels'];h=len(pixels);mono=evidence['observed']['stored_depth']==1
  expected=sorted((x-2,h-1-y-1,int(value)*(255 if mono else 1))for y,row in enumerate(pixels)for x,value in enumerate(row)if value)
  assert g['points']==expected and g['advance']==[14,0],(name,g,expected)
  out=d/'native.lepf';run(a.adapter,'write','prebuilt',source,out);compare(ours,native(a.adapter,'prebuilt',out)[0],'prebuilt');verify(a.adapter,'prebuilt',out,'prebuilt',{})
  official.append({'name':name,'original_producer_exit':evidence['exit'],'native_pixels_advance_exact':True,'source_sha256':digest(source.read_bytes()),'native_sha256':digest(out.read_bytes())})
 report={'original_prebuild':official,'original_prebuild_manifest_sha256':digest((research/'handoff-manifest.json').read_bytes()),'original_prebuild_executable_sha256':digest((research/'prebuild').read_bytes()),'bitblt_independent':indexed,'pcl_independent':extra,'python':sys.version,'monobit':monobit.__version__,'fixture_license':'Original project MIT byte assemblers in fuzz/seed_w5c2.py','cases':cases,'normalizations':['Alto: subtract monobit extra +1 from y, primary AltoFontFormats p5 defines baseline-HD-XH','Prebuilt/Prepress: compare advance x; independent monobit drops y advance, native tests verify both axes','Prepress: compare physical pixels; oracle retains storage height ceil16 rather than logical BBdy','Original explicit relative offsets and new RISC OS v7/v8/phases/12bit require primary spec fixtures because monobit lacks these profiles','PCL RLE and all four orientations have a separate independent Python decoder derived from HP tables','Catalyst sparse reader fails zero-width modulo in monobit; independent validation uses dense128-slot fixture','Prebuilt LE: original NeXT producer confirms MSB-first rasters; monobit incorrectly mirrors them. Numeric fields alone are converted to BE for its independent pixel/name/metric comparison; raster bytes are untouched.']}
 report['artifacts']=[{'path':str(f.relative_to(a.artifacts)),'bytes':f.stat().st_size,'sha256':digest(f.read_bytes())}for f in sorted(a.artifacts.rglob('*'))if f.is_file()]
 a.output.write_text(json.dumps(report,indent=2)+'\n')
if __name__=='__main__':main()
