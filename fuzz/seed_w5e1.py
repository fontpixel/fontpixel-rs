#!/usr/bin/env python3
"""Generate original device-font seeds without third-party packages or fonts."""
import argparse
import hashlib
import json
from pathlib import Path
import struct

ROOT=Path(__file__).resolve().parents[1]
def put16(b,p,n):struct.pack_into('<H',b,p,n)
def put32(b,p,n):struct.pack_into('<I',b,p,n)
def rockbox(old=False,gray=False,fixed=False):
    head=324 if old else 4;w,h,n=9,10,3
    size=((w+15)//16*2*h) if old else ((w*h+1)//2 if gray else w*((h+7)//8))
    if fixed:size=(size+1)//2*2
    bits=size*n;os=4 if old else 2;table=(head+32+bits+os-1)//os*os
    b=bytearray(table+(0 if fixed else n*os));b[:4]=b'RB11' if old else b'RB12'
    if old:b[4:12]=b'Original';b[68:71]=b'MIT'
    struct.pack_into('<4H6I',b,head,w,h,8,int(gray),65,65,n,bits//2 if old else bits,0 if fixed else n,0)
    for i in range(n):
        for y in range(h):
            for x in range(w):
                base=head+32+i*size
                if gray:
                    p=y*w+x;b[base+p//2]|=((x*3+y*5+i)%16)<<((p%2)*4)
                elif (x*3+y*5+i)%7<2:
                    if old:b[base+y*2+int(x<8)]|=128>>(x%8)
                    else:b[base+(y//8)*w+x]|=1<<(y%8)
        if not fixed:(put32 if old else put16)(b,table+i*os,i*size//2 if old else i*size)
    return b

def main():
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--output-dir',type=Path,required=True);a=ap.parse_args();seeds={}
    seeds['rockbox_bitmap']={'rb11':rockbox(True),'rb12':rockbox(),'gray4':rockbox(gray=True),'fixed':rockbox(fixed=True)}
    b=bytearray(84);b[:4]=b'HMZK';b[4:14]=b'\x01'+b'\xff'*9;put16(b,14,4);put16(b,16,65);put16(b,18,66)
    for i in range(64):b[20+i]=(i*37)&255
    seeds['hmzk_bitmap']={'two-cells':b}
    b=bytearray(2052);b[:8]=bytes([0x10,0xb1,0x7e,0x5c,0,7,5,2]);b[65*8:66*8]=bytes([0,8,2,2,254,3,10,7]);b[66*8:67*8]=bytes([4,8,0,0,0,0,5,0]);b[2048:]=bytes([0xaa,128,0x55,0]);seeds['sbf_bitmap']={'sparse-signed-blank':b}
    b=bytearray([1,35,9,7])
    for i in range(35):b.extend([0,5,4,0] if i==0 else [1,1,9,2,0xaa,128,0x55,0] if i==33 else [0,2,3,2,0xa0,0x60] if i==34 else [0,0,1,1,0])
    seeds['cybiko_bitmap']={'space-and-sparse':b}
    b=bytearray(780);b[:4]=b'U8/M';b[4]=3;b[5:8]=b'MIT';put16(b,124,42);b[126]=1;b[127]=8
    for p,v in [(128,2),(130,3),(132,1),(134,4),(138,1),(146,1),(240,2)]:put16(b,p,v)
    b[252:256]=bytes([6,2,1,9])
    for i,p in [(1,272),(2,276),(3,280)]:put32(b,256+i*4,p|1<<24)
    b[272:284]=bytes([1,1,1,0,32,32,3,0,0,0,1,0]);put32(b,512,4<<24);put32(b,516,768|10<<24);put32(b,520,775|4<<24)
    b[768:775]=bytes([253,255,2,9,0xaa,0xa9,0x40]);b[775:780]=bytes([254,2,1,2,128]);seeds['u8m_bitmap']={'aliases-radix-pages':b,'load-address':b'\0\x80'+b}
    folder=ROOT/'crates/fontpixel/tests/fixtures/playdate'
    seeds['playdate_bitmap']={p.name:p.read_bytes() for p in folder.iterdir() if p.suffix in ['.pft','.fnt']}
    manifest={}
    for target,files in seeds.items():
        dest=a.output_dir/target;dest.mkdir(parents=True,exist_ok=True)
        original=next(iter(files.values()));files['truncated']=original[:len(original)//2];files['empty']=b''
        for name,content in files.items():
            (dest/name).write_bytes(content);manifest[str(dest/name)]={'sha256':hashlib.sha256(content).hexdigest(),'bytes':len(content)}
    (a.output_dir/'w5e1-seeds.json').write_text(json.dumps(manifest,indent=2)+'\n');print(json.dumps({'seeds':len(manifest),'targets':len(seeds)}))
if __name__=='__main__':main()
