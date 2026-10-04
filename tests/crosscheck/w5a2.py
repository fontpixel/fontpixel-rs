#!/usr/bin/env python3
"""Native W5.A2 crosschecks; execute only project-authored COM writer fixtures.

Unmodified Unicorn x86 and kosarev/z80 CPU engines execute emitted instructions.
Minimal BIOS hooks implement documented video-font/CP/M USERF contracts; no input
executable from users or third parties is executed. Oracle files stay external.
"""
import argparse, hashlib, importlib.metadata, json, os, pathlib, struct, subprocess, sys
ROOT=pathlib.Path(__file__).resolve().parents[2]
WORK=pathlib.Path(os.environ.get('FONTPIXEL_WORK_ROOT',ROOT.parent/'local-work'))/'w5a2'
def run(cmd):
 p=subprocess.run(list(map(str,cmd)),capture_output=True,text=True)
 if p.returncode:raise RuntimeError(f'{cmd}: exit {p.returncode}\n{p.stdout}\n{p.stderr}')
 return p.stdout
def x86_install(data, expected, tsr=False):
 from unicorn import Uc,UC_ARCH_X86,UC_MODE_16,UC_HOOK_INTR
 from unicorn.x86_const import UC_X86_REG_CS as CS,UC_X86_REG_DS as DS,UC_X86_REG_ES as ES,UC_X86_REG_SS as SS,UC_X86_REG_SP as SP,UC_X86_REG_IP as IP,UC_X86_REG_EFLAGS as FLAGS,UC_X86_REG_AX as AX,UC_X86_REG_BX as BX,UC_X86_REG_CX as CX,UC_X86_REG_DX as DX,UC_X86_REG_BP as BP
 u=Uc(UC_ARCH_X86,UC_MODE_16);u.mem_map(0,0x100000);seg=0x2000;base=seg*16
 u.mem_write(base+0x100,data);u.mem_write(base,b'\xcd\x20');u.mem_write(0xf0100,b'\xcd\xf1\xcf');u.mem_write(base+0x80,b'\xcd\xf2')
 u.mem_write(0x40,struct.pack('<HH',0x100,0xf000));u.mem_write(base+0xfffe,b'\0\0')
 for reg in (CS,DS,ES,SS):u.reg_write(reg,seg)
 u.reg_write(SP,0xfffe);u.reg_write(FLAGS,0x202)
 installed=[];residents=[];modes=[]
 def push(v):
  sp=(u.reg_read(SP)-2)&65535;u.reg_write(SP,sp);u.mem_write(u.reg_read(SS)*16+sp,struct.pack('<H',v&65535))
 def interrupt(_,num,user):
  ax=u.reg_read(AX)
  if num==0x10:
   ip,cs=struct.unpack('<HH',u.mem_read(0x40,4));push(u.reg_read(FLAGS));push(u.reg_read(CS));push(u.reg_read(IP));u.reg_write(CS,cs);u.reg_write(IP,ip)
  elif num==0xf1:
   if ax==0x1110:
    bx=u.reg_read(BX);n=u.reg_read(CX);start=u.reg_read(DX);h=bx>>8
    assert bx&255==0 and n==256 and start==0,(ax,bx,n,start)
    installed.append(bytes(u.mem_read(u.reg_read(ES)*16+u.reg_read(BP),n*h)))
   elif ax>>8==0:modes.append(ax&255)
   else:raise AssertionError(f'unexpected BIOS {ax:04x}')
  elif num==0x21:
   if ax==0x3510:
    ip,cs=struct.unpack('<HH',u.mem_read(0x40,4));u.reg_write(BX,ip);u.reg_write(ES,cs)
   elif ax==0x2510:u.mem_write(0x40,struct.pack('<HH',u.reg_read(DX),u.reg_read(DS)))
   elif ax>>8==0x31:residents.append(u.reg_read(DX)*16);u.emu_stop()
   elif ax>>8==0x4c:assert ax&255==(1 if expected is None else 0);u.emu_stop()
   else:raise AssertionError(f'unexpected DOS {ax:04x}')
  elif num in (0x20,0xf2):u.emu_stop()
  else:raise AssertionError(f'unexpected interrupt {num:x}')
 u.hook_add(UC_HOOK_INTR,interrupt);u.emu_start(base+0x100,0x100000,count=100000)
 assert installed==([] if expected is None else [expected]),(len(installed),expected is None)
 if tsr:
  assert residents and residents[0]>=0x100+len(data)
  handler,cs=struct.unpack('<HH',u.mem_read(0x40,4));assert cs==seg
  for mode in (3,7,0x13):
   u.reg_write(SP,0xffe0);u.reg_write(AX,mode);u.reg_write(CX,0x5678);u.reg_write(DS,0x3456);u.reg_write(ES,0x4567)
   push(0x202);push(seg);push(0x80);u.reg_write(CS,seg)
   u.emu_start(base+handler,0x100000,count=100000)
   assert u.reg_read(CX)==0x5678 and u.reg_read(DS)==0x3456 and u.reg_read(ES)==0x4567
  assert modes==[3,7,0x13] and installed==[expected]*3,(modes,len(installed))
 return {'engine':'unicorn '+importlib.metadata.version('unicorn'),'font_installs':len(installed),'mode_changes':modes,'resident_bytes':residents}
def z80_install(data,expected,target,height):
 import z80
 m=z80.Z80Machine();m.memory[:]=b'\xa6'*65536;m.memory[0x100:0x100+len(data)]=data;m.memory[1:3]=struct.pack('<H',0xf000);m.pc=0x100;m.sp=0xff00;m.set_breakpoint(0xf057);m.set_breakpoint(0)
 calls=0
 for step in range(200):
  m.ticks_to_stop=200000;m.run()
  if m.pc==0:break
  if m.pc==0xf057:
   ret=int.from_bytes(m.memory[m.sp:m.sp+2],'little');fn=int.from_bytes(m.memory[ret:ret+2],'little');callback=m.bc if target=='pcw' else fn
   assert callback>=0xc000 and ret>=0xc000
   if target=='pcw':assert fn==0xe9
   # USERF exposes bank-zero screen memory, so transient lower-memory data is unavailable.
   m.memory[:0xc000]=b'\x5a'*0xc000
   m.memory[m.sp:m.sp+2]=struct.pack('<H',ret+2);m.pc=callback;calls+=1
 else:raise AssertionError(f'Z80 did not terminate: PC={m.pc:04x}')
 assert calls==1
 wanted=expected if height==8 else expected[::2]
 dest={'pcw':0xb800,'cpc':0x8000,'spectrum':0x3000}[target]
 assert bytes(m.memory[dest:dest+len(wanted)])==wanted,(target,height,next((i for i,(a,b) in enumerate(zip(m.memory[dest:],wanted)) if a!=b),None))
 assert bytes(m.memory[:dest])==bytes([0x5a])*dest
 assert bytes(m.memory[dest+len(wanted):0xc000])==bytes([0x5a])*(0xc000-dest-len(wanted))
 return {'engine':'z80 '+importlib.metadata.version('z80'),'target':target,'installed_bytes':len(wanted),'bank_switch_calls':calls,'native_16_to_8':height==16}
def compare(expected,font,kind):
 for rec in expected['glyphs']:
  if not rec['codes']:continue
  cp=rec['codes'][0]
  g=font.get_glyph(codepoint=cp)
  matrix=g.as_matrix();pixels=[255*int(x) for row in matrix for x in (row[:8] if kind=='wyse' else row)]
  assert pixels==rec['pixels'],(kind,cp,'pixels',len(pixels),len(rec['pixels']))
  w=8 if kind=='wyse' else g.width;advance=8 if kind=='wyse' else g.advance_width
  assert (w,g.height,g.left_bearing,g.shift_up,advance)==(rec['width'],rec['height'],rec['x'],rec['y'],rec['advance']),(kind,cp,'metrics',(w,g.height,g.left_bearing,g.shift_up,advance),rec)
def main():
 import monobit
 p=argparse.ArgumentParser();p.add_argument('--bridge',type=pathlib.Path,default=ROOT.parent/'local-work/target-w5a2/debug/examples/w5a2_timing');p.add_argument('--out',type=pathlib.Path,default=ROOT/'reports/w5.a2-crosscheck.json');a=p.parse_args();WORK.mkdir(parents=True,exist_ok=True)
 kinds=['bbc','wyse','grasp','hrcg','xbin','xbin-512','consoleet','zapfont-old','zapfont-redraw']
 kinds+=['comfont-'+v+h for v in ['fontedit','frapt','frapt-tsr','mania'] for h in ['', '-32']]
 kinds+=['comfont-'+v+'-'+t+h for v in ['letafont','udg','psfcom'] for t in ['pcw','cpc','spectrum'] for h in (['','-16'] if v=='psfcom' else [''])]
 kinds+=['comfont-psfcom-spectrum-512','comfont-psfcom-spectrum-512-16']
 results={}
 for kind in kinds:
  file=WORK/('fixture-'+kind);run([a.bridge,'fixture',kind,file]);expect=json.loads(run([a.bridge,'inspect',kind,file]));fmt=kind.split('-')[0];variant=None
  if fmt=='comfont':variant='frapt-tsr' if 'tsr' in kind else kind.split('-')[1];fmt=variant
  elif fmt=='zapfont':fmt='zapfont' if 'old' in kind else 'zapredraw'
  f=monobit.load(file,format=fmt)[0];compare(expect,f,kind.split('-')[0]);result={'glyphs':len(expect['glyphs']),'monobit_read':'pass'}
  if file.is_file():result['sha256']=hashlib.sha256(file.read_bytes()).hexdigest()
  if kind=='wyse':result['oracle_projection']='compare native eight columns; monobit adds two blank display columns'
  if variant:
   data=file.read_bytes();h=expect['glyphs'][0]['height'];rows=b''.join(bytes(sum((pixel!=0)<<(7-x) for x,pixel in enumerate(g['pixels'][y*8:y*8+8])) for y in range(h)) for g in expect['glyphs'])
   if variant in ('fontedit','frapt','frapt-tsr','mania'):result['execution']=x86_install(data,rows,variant=='frapt-tsr')
   else:
    result['execution']=z80_install(data,rows,next(t for t in ('pcw','cpc','spectrum') if t in kind),h)
    if variant=='psfcom':result['dos_unsupported_exit']=x86_install(data,None)
  else:
   out=WORK/('oracle-'+kind+('' if kind=='consoleet' else '.bin'))
   try:
    if kind=='consoleet':monobit.save(f,out,format=fmt,overwrite=True)
    else:
     with out.open('wb') as stream:monobit.save(f,stream,format=fmt)
    back=json.loads(run([a.bridge,'inspect',kind,out]));compare(back,monobit.load(out,format=fmt)[0],kind.split('-')[0]);result['oracle_output_reader_comparison']='pass';result['oracle_output_glyphs']=len(back['glyphs']);
    before={cp:g for g in expect['glyphs'] for cp in g['codes']};after={cp:g for g in back['glyphs'] for cp in g['codes']};
    for cp,g in before.items():assert after[cp]==g,(kind,cp,'upstream source projection')
    result['monobit_write']='pass';result['upstream_added_slots']=len(after)-len(before)
   except Exception as exc:
    result['upstream_write_failure']=str(exc)
    if kind=='wyse':
     native=monobit.Font([monobit.Glyph.from_matrix([row[:8] for row in g.as_matrix()],inklevels=(0,1),codepoint=int(g.codepoint)) for g in f.glyphs])
     with out.open('wb') as stream:monobit.save(native,stream,format=fmt)
     back=json.loads(run([a.bridge,'inspect',kind,out]));assert back==expect;result['explicit_wire_width_reemit']='pass'
  results[kind]=result
  a.out.write_text(json.dumps({'results':results},indent=2)+'\n')
 # Independently authored non-monotonic ordinal table from the primary Zap specification.
 original=bytearray(578);original[:4]=b'ZRUF'
 for at,value in [(8,8),(12,1),(16,32),(36,256),(40,64),(48,0x2600),(52,2),(56,320)]:struct.pack_into('<I',original,at,value)
 original[321]=2;original[322]=1;original[323]=2;original[576]=1;original[577]=2
 path=WORK/'zap-ordinals.bin';path.write_bytes(original);actual=json.loads(run([a.bridge,'inspect','zapfont-redraw',path]));bycode={cp:g for g in actual['glyphs'] for cp in g['codes']}
 assert bycode[0x2601]['pixels']==[0,255,0,0,0,0,0,0] and bycode[0x2602]['pixels']==[255,0,0,0,0,0,0,0] and bycode[0x2601]==bycode[0x2603]
 upstream=monobit.load(path,format='zapredraw')[0];up_pixels=[int(v)*255 for row in upstream.get_glyph(codepoint=0x2601).as_matrix() for v in row]
 assert up_pixels!=bycode[0x2601]['pixels']
 results['zap-ordinal-alias-primary']={'sha256':hashlib.sha256(original).hexdigest(),'primary_table_mapping':'pass','upstream_read_limitation':'0.54.0 reads sorted nonzero table positions instead of actual one-based ordinals; does not preserve repeated ordinal aliases'}
 path=WORK/'zap-width9.bin';path.write_bytes(b'ZapFont\r'+struct.pack('<6I',9,1,65,67,0,0)+bytes([1,1,2,0]));actual=json.loads(run([a.bridge,'inspect','zapfont-old',path]));compare(actual,monobit.load(path,format='zapfont')[0],'zapfont');results['zap-width9']={'monobit_read':'pass','glyphs':2,'sha256':hashlib.sha256(path.read_bytes()).hexdigest()}
 provenance={}
 import monobit.storage.fontformats.raw.comloaders as com
 import z80, importlib
 for path in [pathlib.Path(com.__file__),*pathlib.Path(z80.__file__).parent.glob('*.so')]:
  if path.exists():provenance[str(path)]=hashlib.sha256(path.read_bytes()).hexdigest()
 for module in ['softfont.bbc','softfont.wyse','raw.grasp','raw.hrcg','raw.xbin','text.consoleet','raw.zapfont','zapredraw']:
  path=pathlib.Path(importlib.import_module('monobit.storage.fontformats.'+module).__file__);provenance[str(path)]=hashlib.sha256(path.read_bytes()).hexdigest()
 for name in ['fontraption-formats.inc','wyse.pdf','zap-formats.txt','xbios.html','xbiosint.html']:
  path=WORK/name;provenance[str(path)]=hashlib.sha256(path.read_bytes()).hexdigest()
 a.out.write_text(json.dumps({'python':sys.version,'monobit':importlib.metadata.version('monobit'),'provenance':provenance,'results':results},indent=2)+'\n');print(json.dumps({'passed':len(results),'report':str(a.out)}))
if __name__=='__main__':main()
