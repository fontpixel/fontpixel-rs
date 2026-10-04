#!/usr/bin/env python3
"""Original MIT wire seeds for PK/GF/Plan 9. No third-party font bytes."""
import argparse
import struct
from pathlib import Path

def pk(d=14):
    head=b'\xf7\x59\x00'+struct.pack('>IIII',10<<20,0,65536,65536)
    if d==14:payload=b'\xff\x80'
    else:
        def packed(v):
            if 1<=v<=d:return [v]
            if v<=(13-d)*16+d:
                a,b=divmod(v-d-1,16);return [a+d+1,b]
            value=v-(13-d)*16-d+15
            digits=[int(c,16)for c in f'{value:x}']
            return [0]*(len(digits)-1)+digits
        ns=[14]+packed(2)+packed(3)
        if len(ns)%2:ns.append(0)
        payload=bytes((a<<4)|b for a,b in zip(ns[::2],ns[1::2]))
    return head+bytes([(d<<4)|(8 if d<14 else 0),8+len(payload),65,8,0,0,4,3,3,1,2])+payload+b'\xf5'

def gf():
    b=bytearray([247,131,0]);pointer=len(b);b+=bytes([68,65,3,3,2,2,0,3,74,1,1,1,74,3,69]);end=len(b);post=len(b);b+=bytes([248])+struct.pack('>9I',end,10<<20,0,65536,65536,0,3,0,2);b+=bytes([246,65,4])+struct.pack('>Ii',1<<19,pointer);b+=bytes([249])+struct.pack('>I',post)+bytes([131,223,223,223,223]);return bytes(b)

def plan9(compressed=False,old=False):
    field=lambda n:f'{n:>11} '.encode()
    raw=bytes([0xe0,0xa0,0xe0]);raw=bytes(v^255 for v in raw)if old else raw
    b=(b'compressed\n'if compressed else b'')+field('0'if old else 'k1')+b''.join(field(v)for v in [0,0,3,3])
    if compressed:b+=field(3)+field(6)+b''.join(bytes([128,v])for v in raw)
    else:b+=raw
    return b+b''.join(field(v)for v in [1,3,3])+bytes([0,0,0,3,255,4,3,0,0,0,0,0])

def main():
    p=argparse.ArgumentParser();p.add_argument('--out',type=Path,required=True);o=p.parse_args().out
    for name in ('pk','gf','plan9'):(o/name).mkdir(parents=True,exist_ok=True)
    for d in range(15):(o/'pk'/f'dynf-{d}').write_bytes(pk(d))
    (o/'gf'/'commands').write_bytes(gf())
    for comp in (False,True):
        for old in (False,True):
            b=plan9(comp,old);(o/'plan9'/f'image-{comp}-{old}').write_bytes(struct.pack('<I',len(b))+b)
    primary=b'3 3\n65 65 a\n66 66 b\n';(o/'plan9'/'manifest').write_bytes(struct.pack('<I',len(primary))+primary+plan9(True))
if __name__=='__main__':main()
