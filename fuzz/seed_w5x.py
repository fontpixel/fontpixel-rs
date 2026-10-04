#!/usr/bin/env python3
"""Original project-authored MIT raster seeds; stdlib only, no historical artwork."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import zlib


def raw(width,height,count,flags,offset):
    strike=[2,3,8,256,512][offset%5] if flags&128 else 1
    bit=bool(flags&32);rows=[]
    for first in range(0,count,strike):
        group=[]
        for y in range(height):
            sy=height-1-y if flags&16 else y
            line=''.join(''.join('1' if (g*11+x*7+sy*3)%13<6 else '0' for x in range(width)) for g in range(first,first+strike))
            if not bit:line=line.ljust((len(line)+7)//8*8+8*(flags&7),'0')
            group.append(line)
        rows.append(''.join(group))
    def pack(bits):
        body=bytes(int(bits[i:i+8].ljust(8,'0'),2) for i in range(0,len(bits),8))
        return bytes(int(f'{v:08b}'[::-1],2) for v in body) if flags&8 else body
    groups=[pack(''.join(rows))] if bit else [pack(row) for row in rows]
    if flags&64 and not bit:groups=[bytes(g[y*len(g)//height+x] for x in range(len(g)//height) for y in range(height)) for g in groups]
    return bytes([width-1,height-1,flags,offset])+bytes(offset)+b''.join(groups)


def png():
    def chunk(tag,b):return struct.pack('>I',len(b))+tag+b+struct.pack('>I',zlib.crc32(tag+b))
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',3,2,8,0,0,0,0))+chunk(b'IDAT',zlib.compress(b'\0\0\x7f\xff\0\xff\x80\0'))+chunk(b'IEND',b'')


def fixtures():
    d=dict(info=dict(face='Original "JSON" 字',size=8,unicode=True,padding=[0,0,0,0],spacing=[0,0]),common=dict(lineHeight=9,base=6,scaleW=3,scaleH=2,pages=1),pages=['page.png'],chars=[dict(id=90,x=0,y=0,width=3,height=2,xoffset=-1,yoffset=5,xadvance=5,page=0,chnl=15)],kernings=[])
    desc=json.dumps(d).encode();page=png()
    def bundled(b):return struct.pack('<I',len(b))+b+page
    raws={'w5x-bit3':raw(3,3,5,32,0),'w5x-bit9-lsb-bottom':raw(9,3,5,32|8|16,0),'w5x-column16':raw(16,5,5,64,0),'w5x-strike3-padding':raw(3,3,6,128|64|8|16|2,1),'w5x-harlekin512':raw(8,8,512,128,4),'w5x-harlekin256x16':raw(8,16,256,128,3)}
    bm={'w5x-json':bundled(desc),'w5x-json-bom':bundled(b'\xef\xbb\xbf'+desc),'w5x-json-duplicate':bundled(desc.replace(b'{',b'{"info":{},',1)),'w5x-json-depth':bundled(b'{"info":'+b'['*40+b'1'+b']'*40+b'}'),'w5x-json-count':bundled(desc.replace(b'"id": 90',b'"id": 4294967295')),'w5x-json-traversal':bundled(desc.replace(b'page.png',b'../page.png')),'w5x-json-float':bundled(desc.replace(b'"size": 8',b'"size": 1.5'))}
    for key,value in list(raws.items()):raws[key+'-truncated']=value[:-1]
    for key,value in list(bm.items()):bm[key+'-truncated']=value[:-1]
    return {'raw':raws,'bmfont':bm}


def main():
    p=argparse.ArgumentParser();p.add_argument('--root',type=Path,default=Path(__file__).parent/'corpus');p.add_argument('--manifest',type=Path);a=p.parse_args();result={}
    for ident,seeds in fixtures().items():
        dest=a.root/ident;dest.mkdir(parents=True,exist_ok=True)
        for name,data in seeds.items():
            (dest/name).write_bytes(data);result[f'{ident}/{name}']=dict(bytes=len(data),sha256=hashlib.sha256(data).hexdigest())
    if a.manifest:a.manifest.write_text(json.dumps(dict(license='Original project-authored fixtures: MIT',seeds=result),indent=2)+'\n')
    print(f'{len(result)} original W5.X seeds')
if __name__=='__main__':main()
