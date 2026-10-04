#!/usr/bin/env python3
"""Original MIT raw/BMFont JSON fixtures, immutable monobit 0.54.0 in both directions."""
import argparse
import hashlib
import importlib.metadata
import io
import time
import json
from pathlib import Path
import struct
import subprocess
import sys
import zlib
import monobit
from monobit import Font, Glyph
from monobit.core import Raster
import monobit.storage.fontformats


def digest(p): return hashlib.sha256(Path(p).read_bytes()).hexdigest()

def original(width, height, count):
    return Font(tuple(Glyph(tuple(''.join('1' if (g*11+x*7+y*(width*7+3))%13<6 else '0' for x in range(width)) for y in range(height)),inklevels='01',codepoint=g,shift_up=0) for g in range(count)),ascent=height,descent=0,name='Original W5X')

def snapshot(f):
    return [dict(codes=[ord(g.char)] if g.char else [int(g.codepoint)],width=g.width,height=g.height,x=g.left_bearing,y=g.shift_up,advance=g.advance_width,pixels=[p for row in g.as_matrix(inklevels=tuple(range(0,256,255//(g.levels-1)))) for p in row]) for g in f.glyphs]

def raw_wire(font, config):
    """Independent row concatenation and byte-matrix transpose, not native addressing."""
    w,h=config['width'],config['height']; strike=config.get('strike_count',1)
    bit=config.get('packing')=='bit'; rows=[]
    glyphs=[g.as_matrix(inklevels='01') for g in font.glyphs]
    for first in range(0,len(glyphs),strike):
        group=[]
        for y in range(h):
            sy=h-1-y if config.get('bottom') else y
            line=''.join(''.join(glyphs[g][sy]) if g<len(glyphs) else '0'*w for g in range(first,first+strike))
            if not bit: line=line.ljust((len(line)+7)//8*8+8*config.get('padding',0),'0')
            group.append(line)
        rows.append(''.join(group))
    def pack(bits):
        result=bytes(int(bits[i:i+8].ljust(8,'0'),2) for i in range(0,len(bits),8))
        if config.get('lsb'):result=bytes(int(f'{v:08b}'[::-1],2) for v in result)
        return result
    if bit: body=pack(''.join(rows))
    else:
        groups=[pack(row) for row in rows]
        if config.get('order')=='column': groups=[bytes(group[y*len(group)//h+x] for x in range(len(group)//h) for y in range(h)) for group in groups]
        body=b''.join(groups)
    return bytes(config.get('offset',0))+body

def png(width,height,pixels):
    def chunk(tag,b):return struct.pack('>I',len(b))+tag+b+struct.pack('>I',zlib.crc32(tag+b))
    raw=b''.join(b'\0'+bytes(pixels[y*width:(y+1)*width]) for y in range(height))
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',width,height,8,0,0,0,0))+chunk(b'IDAT',zlib.compress(raw))+chunk(b'IEND',b'')

def benchmark(kind, path, config, loops):
    """All inputs are preloaded; output pages remain in independent memory streams."""
    from monobit.storage.streams import Stream
    from monobit.storage.location import Location
    from monobit.storage.fontformats.image import bmfont as oracle_bmfont
    b=path.read_bytes()
    if kind=='raw':
        if any(config.get(k) for k in ['padding','offset','lsb','bottom']):
            raise ValueError('raw benchmark currently uses unpadded, top-down, MSB input')
        options=dict(cell=(config['width'],config['height']),count=config.get('count',-1),strike_count=config.get('strike_count',1),align='bit' if config.get('packing')=='bit' else 'left')
        def read():return monobit.load(io.BufferedReader(io.BytesIO(b)),format='raw',**options)[0]
        def write(font):monobit.save(font,io.BytesIO(),format='raw',strike_count=options['strike_count'],align=options['align'])
        if config.get('order')=='column':
            w,h,strike=config['width'],config['height'],options['strike_count'];rowbytes=(w*strike+7)//8
            def read():
                glyphs=[]
                for start in range(0,len(b),rowbytes*h):
                    raster=Raster.from_bytes(b[start:start+rowbytes*h],width=w*strike,height=h,order='column-major')
                    for cell in range(strike):glyphs.append(Glyph(raster.crop(left=cell*w,right=(strike-cell-1)*w),codepoint=len(glyphs),shift_up=0))
                return Font(tuple(glyphs),ascent=h,descent=0)
            def write(font):
                stream=io.BytesIO();monobit.save(font,stream,format='raw',strike_count=strike);data=stream.getvalue()
                return b''.join(bytes(group[y*rowbytes+x] for x in range(rowbytes) for y in range(h)) for group in [data[i:i+rowbytes*h] for i in range(0,len(data),rowbytes*h)])
    else:
        pages={str(p.relative_to(path.parent)):p.read_bytes() for p in path.parent.rglob('*.png')}
        class MemoryLocation(Location):
            def __init__(self): pass
            def open(self,name,mode):
                return Stream.from_data(pages[str(name)],mode='r',name=str(name),where=self) if mode=='r' else Stream(io.BytesIO(),mode='w',name=str(name),where=self)
            def unused_name(self,name):return name
        def read():
            buffer=io.BytesIO(b);buffer.name='font.fnt'
            with Stream(io.BufferedReader(buffer),mode='r',name='font.fnt',where=MemoryLocation()) as stream:return oracle_bmfont.load_bmfont(stream)
        def write(font):
            with Stream(io.BytesIO(),mode='w',name='font.fnt',where=MemoryLocation()) as stream:oracle_bmfont.save((font,),stream,descriptor=config.get('encoding','json'),image_size=(config.get('page_width',64),config.get('page_height',64)))
    font=read();write(font)
    start=time.perf_counter_ns()
    for _ in range(loops):read()
    read_ns=time.perf_counter_ns()-start
    start=time.perf_counter_ns()
    for _ in range(loops):write(font)
    write_ns=time.perf_counter_ns()-start
    print(json.dumps(dict(format=kind,loops=loops,read_ns=read_ns,write_ns=write_ns,monobit=importlib.metadata.version('monobit'),config=config)))


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--binary',type=Path);p.add_argument('--output-dir',type=Path);p.add_argument('--bench',choices=['raw','bmfont']);p.add_argument('--file',type=Path);p.add_argument('--config',default='{}');p.add_argument('--count',type=int,default=100);a=p.parse_args()
    if a.bench:
        if not a.file or a.count<1:p.error('benchmark requires --file and positive --count')
        benchmark(a.bench,a.file,json.loads(a.config),a.count);return
    if not a.binary or not a.output_dir:p.error('crosscheck requires --binary and --output-dir')
    out=a.output_dir.resolve();out.mkdir(parents=True,exist_ok=False);binary=a.binary.resolve();checks=[];commands=[]
    def native(command,kind,path,config,extra=None):
        argv=[str(binary),command,kind,str(path),json.dumps(config,separators=(',',':'))]+([str(extra)] if extra is not None else [])
        proc=subprocess.run(argv,capture_output=True,text=True);commands.append(dict(argv=argv,exit=proc.returncode));assert proc.returncode==0,(argv,proc.stderr)
        return json.loads(proc.stdout) if proc.stdout.strip() else None
    cases=[('legacy',dict(width=8,height=8,count=8)),('bits9',dict(width=9,height=3,count=7,packing='bit')),('bits3',dict(width=3,height=3,count=5,packing='bit')),('bits_many',dict(width=9,height=8,count=256,packing='bit')),('columns16',dict(width=16,height=5,count=6,order='column')),('columns9_strike',dict(width=9,height=3,count=6,order='column',strike_count=3)),('strike9',dict(width=9,height=3,count=6,strike_count=3)),('harlekin512',dict(width=8,height=8,count=512,strike_count=512)),('harlekin8',dict(width=8,height=8,count=256,strike_count=256)),('harlekin16',dict(width=8,height=16,count=256,strike_count=256)),('partial_padding_lsb_bottom',dict(width=9,height=3,count=5,strike_count=3,padding=2,offset=3,order='column',lsb=True,bottom=True)),('bits_lsb_bottom',dict(width=3,height=3,count=5,packing='bit',lsb=True,bottom=True))]
    for name,config in cases:
        case=out/name;case.mkdir();font=original(config['width'],config['height'],config['count']);expected=snapshot(font);source=case/'source.raw'
        source.write_bytes(raw_wire(font,config));assert native('dump','raw',source,config)['glyphs']==expected,name
        target=case/'native';native('rewrite','raw',source,config,target);assert (target/'font.raw').read_bytes()==source.read_bytes(),name
        direct=not any(config.get(k) for k in ['padding','lsb','bottom'])
        if direct:
            mono=monobit.load(target/'font.raw',format='raw',cell=(config['width'],config['height']),count=config['count'],align='bit' if config.get('packing')=='bit' else 'left',byte_order='column-major' if config.get('order')=='column' else 'row-major',strike_count=config.get('strike_count',1))[0]
            if config.get('order')=='column':
                # Published 0.54 raw wrapper passes byte_order= into Raster's
                # **kwargs; actual public keyword is order=. Use that independent
                # public reader directly, without monkeypatching/install edits.
                assert snapshot(mono)!=expected, 'oracle wrapper mismatch changed'
                w,h,strike=config['width'],config['height'],config.get('strike_count',1)
                data=(target/'font.raw').read_bytes();stride=(w*strike+7)//8;glyphs=[]
                for start in range(0,len(data),stride*h):
                    raster=Raster.from_bytes(data[start:start+stride*h],width=w*strike,height=h,order='column-major')
                    for cell in range(strike):glyphs.append(Glyph(raster.crop(left=cell*w,right=(strike-cell-1)*w),codepoint=len(glyphs),shift_up=0))
                mono=Font(tuple(glyphs),ascent=h,descent=0)
            assert snapshot(mono)==expected,name
            independent=case/'oracle.raw';monobit.save(font,independent,format='raw',align='bit' if config.get('packing')=='bit' else 'left',strike_count=config.get('strike_count',1))
            if config.get('order')=='column':
                rowbytes=(config['width']*config.get('strike_count',1)+7)//8;h=config['height'];data=independent.read_bytes();independent.write_bytes(b''.join(bytes(group[y*rowbytes+x] for x in range(rowbytes) for y in range(h)) for group in [data[i:i+rowbytes*h] for i in range(0,len(data),rowbytes*h)]))
            assert independent.read_bytes()==source.read_bytes(),name
            assert native('dump','raw',independent,config)['glyphs']==expected,name
        checks.append(dict(format='raw',case=name,config=config,glyphs=len(expected),directions=2,oracle='monobit writer/reader'+(' Raster(order=column-major) + independent byte-column transpose; raw wrapper ignores obsolete byte_order keyword' if config.get('order')=='column' else '') if direct else 'independent original row-string assembler + exact wire equality',native_fixture=str(source),sha256=digest(source)))
    # Original hand-authored schema, independent zlib/PNG assembler, unsorted codes,
    # two pages, Unicode JSON escapes, negative bearings and differing baseline.
    case=out/'bmfont';case.mkdir();pixels=[0,127,255,255,128,0,10,20,30];(case/'page0.png').write_bytes(png(3,3,pixels));(case/'page1.png').write_bytes(png(3,3,list(reversed(pixels))))
    chars=[dict(id=code,x=0,y=0,width=3,height=3,xoffset=-2+i,yoffset=5+i,xadvance=5+i,page=0 if i<2 else 1,chnl=15) for i,code in enumerate([90,65,0x20000])]
    descriptor=dict(info=dict(face='Original "JSON" 字',size=9,unicode=True,charset='',bold=False,italic=False,padding=[0,0,0,0],spacing=[0,0]),common=dict(lineHeight=11,base=8,scaleW=3,scaleH=3,pages=2,packed=False),pages=['page0.png','page1.png'],chars=chars,kernings=[dict(first=90,second=65,amount=-2)])
    source=case/'source.fnt';source.write_text(json.dumps(descriptor,ensure_ascii=True));config=dict(encoding='json',page_width=4,page_height=4)
    expected=[dict(codes=[c['id']],width=3,height=3,x=c['xoffset'],y=8-c['yoffset']-3,advance=c['xadvance'],pixels=pixels if c['page']==0 else list(reversed(pixels))) for c in chars]
    assert native('dump','bmfont',source,config)['glyphs']==expected
    target=case/'native';native('rewrite','bmfont',source,config,target);written=json.loads((target/'font.fnt').read_text());assert len(written['pages'])==3
    assert written['kernings']==descriptor['kernings'];assert [c['id'] for c in written['chars']]==[90,65,0x20000]
    mono=monobit.load(target/'font.fnt',format='bmfont')[0];shifted=[dict(g,y=g['y']+3-written['common']['base']) for g in expected];assert snapshot(mono)==shifted,(snapshot(mono),shifted)
    # Independent writer starts from explicit original glyph metrics, rather than
    # reusing monobit's known baseline-normalising BMFont reader.
    glyphs=tuple(Glyph.from_matrix(tuple(tuple(g['pixels'][y*3:(y+1)*3]) for y in range(3)),inklevels=tuple(range(256)),char=chr(g['codes'][0]),left_bearing=g['x'],right_bearing=g['advance']-g['x']-3,shift_up=g['y']) for g in expected)
    independent=case/'oracle.fnt';monobit.save(Font(glyphs,ascent=8,descent=3,line_height=11,name='Original JSON',encoding='unicode'),independent,format='bmfont',descriptor='json',image_size=(16,16));assert native('dump','bmfont',independent,config)['glyphs']==expected
    checks.append(dict(format='bmfont',encoding='json',directions=2,glyphs=3,pages=3,oracle='monobit 0.54.0 + original schema/PNG assembler',known_difference='monobit reader ignores common.base; exact shift max glyph height minus common.base independently asserted',native_fixture=str(source)))
    origin=Path(monobit.storage.fontformats.__file__).parent
    evidence=dict(checks=checks,commands=commands,python=sys.version,monobit=importlib.metadata.version('monobit'),binary_sha256=digest(binary),oracle_sources={name:digest(origin/name) for name in ['raw/raw.py','image/bmfont.py','../../core/raster.py']},artifacts={str(p.relative_to(out)):dict(bytes=p.stat().st_size,sha256=digest(p)) for p in out.rglob('*') if p.is_file()})
    (out/'results.json').write_text(json.dumps(evidence,indent=2)+'\n');print(json.dumps(checks,indent=2))
if __name__=='__main__':main()
