#!/usr/bin/env python3
"""W4.6 bidirectional oracle checks. Monobit is MIT; this script is original MIT.
Run with ../local-work/oracles/bin/python tests/crosscheck/w46.py --adapter PATH.
No downloaded or third-party font bytes are written into the repository.
"""
from __future__ import annotations
import argparse, hashlib, json, pathlib, struct, subprocess, tempfile
import monobit

ROOT=pathlib.Path(__file__).resolve().parents[2]
def run(adapter,*args):
    return subprocess.run([str(adapter),*map(str,args)],check=True,text=True,capture_output=True).stdout

def model(font):
    out={}
    for g in font.glyphs:
        if not g.codepoint: continue
        cp=int(g.codepoint)
        pixels=g.as_matrix(inklevels=(0,1))
        points=sorted((x+g.left_bearing,g.height-1-y+g.shift_up) for y,row in enumerate(pixels) for x,p in enumerate(row) if p)
        out[cp]=(g.advance_width,points)
    return out

def rust_model(data):
    out={}
    for g in data[0]['glyphs']:
        stride=(g['width']+7)//8
        points=sorted((x+g['x'],g['height']-1-y+g['y'])for y in range(g['height'])for x in range(g['width'])if g['bytes'][y*stride+x//8]&(128>>(x%8)))
        for cp in g['codes']:out[cp]=(g['advance'],points)
    return out

def snf_encode(font,endian):
    """Independent encoder transcribed from field sizes in X11 snfstr.h."""
    glyphs={int(g.codepoint):g for g in font.glyphs if g.codepoint}
    cols=[c&255 for c in glyphs];rows=[c>>8 for c in glyphs]
    firstcol,lastcol,firstrow,lastrow=min(cols),max(cols),min(rows),max(rows)
    count=(lastcol-firstcol+1)*(lastrow-firstrow+1)
    metrics=[];bitmap=bytearray();existing=[]
    for row in range(firstrow,lastrow+1):
        for col in range(firstcol,lastcol+1):
            g=glyphs.get(row*256+col)
            if g is None:metrics.append(bytes(16));continue
            vals=(g.left_bearing,g.left_bearing+g.width,g.advance_width,g.height+g.shift_up,-g.shift_up,0)
            off=len(bitmap);bits=(off|0x1000000)if endian=='<'else((off<<8)|128)
            metrics.append(struct.pack(endian+'5hHI',*vals,bits));bitmap.extend(g.as_bytes());existing.append(vals)
    hdr=bytearray(108)
    for at,val in [(0,4),(4,int(len(glyphs)==count)),(28,firstcol),(32,lastcol),(36,firstrow),(40,lastrow),(52,65535),(56,font.descent),(60,font.ascent),(96,1),(100,1),(104,4)]:struct.pack_into(endian+'I',hdr,at,val)
    for at,fn in [(64,min),(80,max)]:
        vals=[fn(v[i]for v in existing)for i in range(6)]
        off=len(bitmap)if at==80 else 0
        struct.pack_into(endian+'5hHI',hdr,at,*vals,off if endian=='<'else off<<8)
    bitmap.extend(bytes((-len(bitmap))%4))
    return hdr+b''.join(metrics)+bitmap

def snf_decode(data):
    """Independent field/bitmap interpretation; default writer layout is MSB/4-byte rows."""
    e='<'if data[:4]==b'\4\0\0\0'else'>'
    u32=lambda at:struct.unpack_from(e+'I',data,at)[0]
    assert u32(104)==4
    fc,lc,fr,lr=(u32(i)for i in(28,32,36,40));count=(lc-fc+1)*(lr-fr+1)
    base=108+16*count
    word=u32(92);size=(word&0xffffff)if e=='<'else word>>8
    prop_at=base+(size+3)//4*4;strings_at=prop_at+u32(44)*12
    props={}
    def string(at):return data[at:data.index(0,at)].decode('latin1')
    for i in range(u32(44)):
        name,value,indirect=struct.unpack_from(e+'III',data,prop_at+i*12)
        props[string(strings_at+name)]=string(strings_at+value)if indirect else value
    layout=props['_FONTPIXEL_SNF_BITMAP_FORMAT'];assert layout==0x104
    out={}
    for i in range(count):
        left,right,advance,asc,desc,attr,bits=struct.unpack_from(e+'5hHI',data,108+16*i)
        off=(bits&0xffffff)if e=='<'else bits>>8;exists=bits&(0x1000000 if e=='<'else 128)
        if not exists:continue
        width=right-left;height=asc+desc;stride=((width+7)//8+3)//4*4
        points=sorted((x+left,height-y-1-desc)for y in range(height)for x in range(width)if data[base+off+y*stride+x//8]&(128>>(x%8)))
        cp=(fr+i//(lc-fc+1))*256+fc+i%(lc-fc+1);out[cp]=(advance,points)
    return out

def main():
    p=argparse.ArgumentParser();p.add_argument('--adapter',type=pathlib.Path,required=True);p.add_argument('--output',type=pathlib.Path);a=p.parse_args();a.adapter=a.adapter.resolve()
    g1=monobit.Glyph.from_matrix(((1,0,1),(0,1,0),(1,1,1)),inklevels=(0,1),codepoint=65,shift_up=-1,left_bearing=-1,right_bearing=2)
    g2=monobit.Glyph.from_matrix(((1,1,0,0,1),(1,0,0,1,0),(1,1,1,1,1)),inklevels=(0,1),codepoint=66,shift_up=-1,left_bearing=1,right_bearing=1)
    synthetic=monobit.Font((g1,g2),name='Original W46',encoding='latin-1',ascent=2,descent=1)
    sample=ROOT.parent/'samples/fontpixel-catalogue/tom-thumb/tom-thumb.bdf'
    real=monobit.load(sample)[0].subset(codepoints=range(32,127)).modify(encoding='latin-1').equalise_horizontal()
    results=[]
    with tempfile.TemporaryDirectory(prefix='fontpixel-w46-')as tmp:
        tmp=pathlib.Path(tmp)
        for label,font in [('original',synthetic),('tom-thumb',real)]:
            for fmt,oracle in [('amiga','amiga'),('gem','gdos')]:
                incoming=tmp/f'{label}-oracle.{fmt}'
                monobit.save(font,incoming,format=oracle,overwrite=True,**({'endianness':'big'}if fmt=='gem'else{}))
                oracle_model=model(monobit.load(incoming,format=oracle)[0])
                ours=rust_model(json.loads(run(a.adapter,'dump',fmt,incoming)))
                assert ours==oracle_model,(label,fmt,'oracle-to-rust',set(ours)^set(oracle_model))
                outgoing=tmp/f'{label}-rust.{fmt}'
                run(a.adapter,'write',fmt,incoming,fmt,outgoing)
                back=model(monobit.load(outgoing,format=oracle)[0])
                assert back==oracle_model,(label,fmt,'rust-to-oracle')
                results.append({'font':label,'format':fmt,'glyphs':len(ours),'oracle_sha256':hashlib.sha256(incoming.read_bytes()).hexdigest(),'rust_sha256':hashlib.sha256(outgoing.read_bytes()).hexdigest()})
            for e in ['<','>']:
                incoming=tmp/f'{label}-oracle-{ord(e)}.snf';incoming.write_bytes(snf_encode(font,e))
                ours=rust_model(json.loads(run(a.adapter,'dump','snf1',incoming)))
                assert ours==model(font),(label,'snf independent writer',e)
                outgoing=tmp/f'{label}-rust-{ord(e)}.snf';run(a.adapter,'write','snf1',incoming,'snf',outgoing)
                assert snf_decode(outgoing.read_bytes())==model(font),(label,'snf independent reader',e)
                results.append({'font':label,'format':'snf','endian':e,'glyphs':len(ours),'oracle_sha256':hashlib.sha256(incoming.read_bytes()).hexdigest(),'rust_sha256':hashlib.sha256(outgoing.read_bytes()).hexdigest()})
    result={'monobit':monobit.__version__,'sample':str(sample.relative_to(ROOT.parent)),'sample_sha256':hashlib.sha256(sample.read_bytes()).hexdigest(),'checks':results}
    text=json.dumps(result,indent=2)+'\n'
    if a.output:a.output.write_text(text)
    print(text,end='')
if __name__=='__main__':main()
