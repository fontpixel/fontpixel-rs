#!/usr/bin/env python3
"""Original E2 native consumers: Pillow, monobit, Processing and Bits' N' Picas.

Only public APIs execute third-party code. No third-party implementation is
included in the Rust codecs. Native editors' transparent-border contraction is
recorded separately from exact baseline coordinates, advances and sample values.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import sys
from PIL import Image, ImageFont, FontFile
import PIL
import monobit

JAR='22685a52bc1a7f26680f80197548e74dc61a9f58eb083d6f700220a48a1165a6'
PROCESSING='bab342ad2b80e42b8a728e05a7ec8212bc369513fe8c4ed2381266f1e5e8627d'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def canonical(g,top=None):
    w,h=g['width'],g['height']
    return (g['cp'],g['advance'],tuple((g['x']+x,(g['y']+h-1-y) if top is None else (top-g['y']-h+y),g['pixels'][y*w+x]) for y in range(h)for x in range(w)if g['pixels'][y*w+x]))
def mapped(s,top=None):return {g['cp']:canonical(g,top)for g in s['glyphs']}
def external_dump(raw,vlw=False):
    glyphs=[];font=None
    for line in raw.decode().splitlines():
        p=line.split('\t')
        if p[0]=='FONT':font=p[1:]
        elif p[0]=='GLYPH':
            cp,w,h,x,y,advance=map(int,p[1:7]);glyphs.append(dict(cp=cp,width=w,height=h,x=x,y=y,advance=advance,pixels=list(bytes.fromhex(p[7]))))
    return dict(glyphs=glyphs,font=font)
def mono_signature(f):
    glyphs=[];levels=tuple(round(i*255/(f.levels-1))for i in range(f.levels))
    for g in f.glyphs:
        if g.codepoint: glyphs.append(dict(cp=int(g.codepoint),width=g.width,height=g.height,x=g.left_bearing,y=g.shift_up,advance=g.advance_width,pixels=[p for row in g.as_matrix(inklevels=levels)for p in row]))
    return dict(glyphs=glyphs)
def pil_wire(path):
    data=path.read_bytes();start=data.index(b'DATA\n')+5;assert len(data)==start+5120
    im=Image.open(path.with_suffix('.pbm')).convert('L');glyphs=[]
    for cp in range(256):
        rec=struct.unpack_from('>10h',data,start+cp*20)
        if not any(rec):continue
        ax,ay,left,top,right,bottom,sx,sy,ex,ey=rec
        assert right-left==ex-sx and bottom-top==ey-sy
        glyphs.append(dict(cp=cp,width=ex-sx,height=ey-sy,x=left,y=-bottom,advance=ax,advance_y=-ay,pixels=list(im.crop((sx,sy,ex,ey)).get_flattened_data())))
    return dict(glyphs=glyphs)
def pillow_render(path,native):
    f=ImageFont.load(path);top=max(g['y']+g['height']for g in native['glyphs'])
    for g in native['glyphs']:
        c=chr(g['cp']);assert f.getlength(c)==g['advance']
        # The legacy text-mask API clips negative bearings at string start.
        # A repeated middle glyph exercises the full bearing inside the canvas.
        m=f.getmask(c*3);w,h=m.size;pixels=list(m)
        for y in range(g['height']):
            for x in range(g['width']):
                px=g['advance']+g['x']+x;py=top-g['y']-g['height']+y
                assert 0<=px<w and 0<=py<h
                assert pixels[py*w+px]==g['pixels'][y*g['width']+x],(g['cp'],x,y)

def main():
    ap=argparse.ArgumentParser(description=__doc__)
    for key in ['binary','jar','processing','sdl-reader','output-dir']:ap.add_argument('--'+key,type=Path,required=True)
    a=ap.parse_args();root=Path(__file__).resolve().parents[2];out=a.output_dir.resolve();out.mkdir(parents=True,exist_ok=True)
    assert sha(a.jar)==JAR and sha(a.processing)==PROCESSING
    commands=[];checks=[];fail=None
    def run(*args):
        p=subprocess.run(list(map(str,args)),capture_output=True)
        commands.append(dict(argv=list(map(str,args)),exit=p.returncode,stdout_sha256=hashlib.sha256(p.stdout).hexdigest(),stderr=p.stderr.decode(errors='replace')))
        assert p.returncode==0,(args,p.stderr.decode(errors='replace'));return p.stdout
    def inspect(fmt,path):return json.loads(run(a.binary,'inspect',fmt,path))
    def java(kind,*args):return run('java','-Djava.awt.headless=true','-cp',str(a.jar if kind=='SrFontOracle'else a.processing)+':'+str(out),kind,*args)
    def checked(name,s,other,top=None,other_top=None):
        assert mapped(s,top)==mapped(other,other_top),(name,mapped(s,top),mapped(other,other_top))
        checks.append(dict(name=name,glyphs=len(s['glyphs']),samples=sum(len(g['pixels'])for g in s['glyphs']),comparison='all nonzero samples in baseline coordinates and exact advances; transparent native cell padding retained separately'))
    try:
        run('javac','-cp',a.jar,'-d',out,root/'reports/w5.e2-image/oracles/SrFontOracle.java')
        run('javac','-cp',a.processing,'-d',out,root/'reports/w5.e2-image/oracles/VlwOracle.java')
        for fmt in ['vlw8','vlw9','vlw10','vlw']:
            p=out/(fmt+'.vlw');generated=json.loads(run(a.binary,'emit',fmt,p));native=generated['native'];consumer=external_dump(java('VlwOracle','inspect',p),True)
            checked('Rust→Processing '+fmt,native,consumer)
            assert int(consumer['font'][0])==native['pixel_size'];assert list(map(int,consumer['font'][1:3]))==[native['ascent'],native['descent']]
            if fmt in ['vlw10','vlw']:
                assert bytes.fromhex(consumer['font'][4]).decode()==native['full'];assert bytes.fromhex(consumer['font'][5]).decode()==native['postscript']
            if fmt=='vlw':assert consumer['font'][3]=='true'
            for x,y in zip(native['glyphs'],consumer['glyphs']):
                for key in ['width','height','x','y','advance','pixels']:assert x[key]==y[key]
        p=out/'processing-original.vlw';java('VlwOracle','emit',p);checked('Processing→Rust VLW11',inspect('vlw',p),external_dump(java('VlwOracle','inspect',p),True))
        pil=out/'rust-original.pil';native=json.loads(run(a.binary,'emit','pilfont',pil))['native'];wire=pil_wire(pil)
        checked('Rust→Pillow PILfont native metrics and decoded mask',native,wire);pillow_render(pil,native)
        for x,y in zip(native['glyphs'],wire['glyphs']):
            for key in ['width','height','x','y','advance','advance_y','pixels']:assert x[key]==y[key]
        checks.append(dict(name='Pillow ImageFont.load actual glyph rendering',glyphs=len(native['glyphs']),samples=sum(len(g['pixels'])for g in native['glyphs'])))
        ff=FontFile.FontFile()
        for cp in [65,255]:
            mask=Image.new('1',(3,4));mask.putdata([0,1,0,1,0,1,1,1,0,0,0,1]);ff.glyph[cp]=((5,0),(-1,-6,2,-2),(0,0,3,4),mask)
        pp=out/'pillow-original.pil';ff.save(pp);checked('Pillow FontFile producer→Rust',inspect('pilfont',pp),pil_wire(pp))
        mb=mono_signature(monobit.load(pp,format='pilfont')[0]);checked('Pillow↔monobit↔Rust PILfont',inspect('pilfont',pp),mb)
        reverse=out/'monobit-original.pil';monobit.save(monobit.load(pp,format='pilfont'),reverse,format='pilfont',overwrite=True);checked('monobit PILfont producer→Rust',inspect('pilfont',reverse),pil_wire(reverse))
        for fmt in ['rfont','sfont']:
            p=out/('rust-'+fmt+'.png');native=json.loads(run(a.binary,'emit',fmt,p))['native'];consumer=external_dump(java('SrFontOracle','inspect',p))
            if fmt=='sfont':
                assert bytes.fromhex(consumer['font'][6]).decode()==native['family'];assert bytes.fromhex(consumer['font'][7]).decode()==native['style']
                consumer['glyphs']=[g for g in consumer['glyphs']if g['cp']!=32]
                checked('Rust→BitsNPicas SFont (runtime space excluded)',native,consumer,native['ascent'],int(consumer['font'][0]))
                im=Image.open(p).convert('RGBA');raw=out/'sfont.rgba';raw.write_bytes(struct.pack('<II',*im.size)+im.tobytes());render=run(a.sdl_reader,raw)
                rendered=[]
                for line in render.decode().splitlines():
                    _,cp,w,h,advance,pixels=line.split('\t');rendered.append(dict(cp=int(cp),width=int(w),height=int(h),x=-32,y=0,advance=int(advance),pixels=list(bytes.fromhex(pixels))))
                checked('Rust→unmodified SDL SFont_InitFont/SFont_Write',native,dict(glyphs=rendered),native['ascent'],rendered[0]['height'])
            else:
                checked('Rust→BitsNPicas RFont',native,consumer);assert list(map(int,consumer['font'][:2]))==[native['ascent'],native['descent']];assert int(consumer['font'][5])==native['line_gap'];assert bytes.fromhex(consumer['font'][6]).decode()==native['family']
            incoming=out/('java-'+fmt+'.png');java('SrFontOracle','emit',fmt,incoming);rs=inspect(fmt,incoming);js=external_dump(java('SrFontOracle','inspect',incoming))
            if fmt=='sfont':js['glyphs']=[g for g in js['glyphs']if g['cp']!=32]
            checked('BitsNPicas producer→Rust '+fmt,rs,js,rs['ascent'] if fmt=='sfont'else None,int(js['font'][0]) if fmt=='sfont'else None)
            rewritten=out/('rewritten-'+fmt+'.png');run(a.binary,'rewrite',fmt,incoming,rewritten);reread=inspect(fmt,rewritten);js=external_dump(java('SrFontOracle','inspect',rewritten))
            if fmt=='sfont':js['glyphs']=[g for g in js['glyphs']if g['cp']!=32]
            checked('Rust native rewrite→BitsNPicas '+fmt,reread,js,reread['ascent'] if fmt=='sfont'else None,int(js['font'][0]) if fmt=='sfont'else None)
        # Archive a real limitation of the external Java importer independently.
        incoming=out/'java-sfont-overhang.png';java('SrFontOracle','emit','sfont-overhang',incoming)
        args=['java','-Djava.awt.headless=true','-cp',str(a.jar)+':'+str(out),'SrFontOracle','inspect',str(incoming)]
        rejected=subprocess.run(args,capture_output=True);assert rejected.returncode!=0
        commands.append(dict(argv=args,exit=rejected.returncode,expected_failure='BitsNPicas rejects its own SFont first-glyph overhang in vertical detection column',stdout_sha256=hashlib.sha256(rejected.stdout).hexdigest(),stderr=rejected.stderr.decode(errors='replace')))
        native=inspect('sfont',incoming);im=Image.open(incoming).convert('RGBA');raw=out/'sfont-overhang.rgba';raw.write_bytes(struct.pack('<II',*im.size)+im.tobytes());rendered=[]
        for line in run(a.sdl_reader,raw).decode().splitlines():
            _,cp,w,h,advance,pixels=line.split('\t');rendered.append(dict(cp=int(cp),width=int(w),height=int(h),x=-32,y=0,advance=int(advance),pixels=list(bytes.fromhex(pixels))))
        checked('Java SFont first-glyph overhang→Rust and unmodified SDL',native,dict(glyphs=rendered),native['ascent'],rendered[0]['height'])
        p=out/'rust-imageset';native=json.loads(run(a.binary,'emit','imageset',p))['native'];mono=monobit.load(p,format='imageset',background='darkest')[0];other=mono_signature(mono);checked('Rust imageset→monobit native directory',native,other)
        for g in native['glyphs']:
            im=Image.open(p/(format(g['cp'],'x')+'.png'));assert im.size==(g['width'],g['height']);assert list(im.get_flattened_data())==g['pixels']
        p=out/'monobit-imageset';monobit.save(mono,p,format='imageset',image_mode='grey',overwrite=True);native=inspect('imageset',p);other=mono_signature(monobit.load(p,format='imageset',background='darkest')[0]);checked('monobit imageset producer→Rust',native,other)
    except Exception as e:
        fail=repr(e);raise
    finally:
        files={str(p.relative_to(out)):dict(bytes=p.stat().st_size,sha256=sha(p))for p in sorted(out.rglob('*'))if p.is_file()and p.name!='crosscheck.json'}
        result=dict(binary_sha256=sha(a.binary),python=sys.version,pillow=PIL.__version__,monobit=monobit.__version__,jar_sha256=sha(a.jar),processing_sha256=sha(a.processing),sdl_reader_sha256=sha(a.sdl_reader),commands=commands,checks=checks,files=files,failure=fail)
        (out/'crosscheck.json').write_text(json.dumps(result,indent=2,ensure_ascii=False)+'\n');print(json.dumps(dict(checks=len(checks),failure=fail)))
if __name__=='__main__':main()
