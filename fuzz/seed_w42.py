#!/usr/bin/env python3
"""Generate original, deterministic W4.2 wire seeds; no upstream font bytes."""
import argparse
import hashlib
import json
import struct
from pathlib import Path


def seeds():
    def section(tag, data):
        return tag+struct.pack('>I', len(data))+data
    pf2 = section(b'FILE', b'PFF2')+section(b'NAME', b'W42\0')+section(b'PTSZ', b'\0\x08')+section(b'ASCE', b'\0\x06')+section(b'DESC', b'\0\x02')
    offset=len(pf2)+8+18+8
    pf2 += section(b'CHIX', struct.pack('>IBIIBI',65,0,offset,66,0,offset))+b'DATA\xff\xff\xff\xff'+struct.pack('>HHhhh',3,3,-1,-1,4)+b'\xaa\x80'
    yield 'pf2', 'authored-aliases', pf2
    yield 'pf2', 'authored-truncated', pf2[:-1]
    yield 'pf2', 'authored-padding', pf2[:-1]+b'\xff'
    index = pf2.index(b'CHIX') + 8
    joined = bytearray(pf2)
    joined[index:index+4] = struct.pack('>I', 0xc0000628)
    yield 'pf2', 'authored-joining-variant', bytes(joined)
    joined[index:index+4] = struct.pack('>I', 0xc000d800)
    yield 'pf2', 'authored-invalid-joining-base', bytes(joined)
    reserved = bytearray(pf2)
    reserved[index+4] = 1
    yield 'pf2', 'authored-reserved-storage', bytes(reserved)
    sb = b'FONTX2W42wire '+bytes([8,2,0])+bytes((i*37)&255 for i in range(512))
    db = b'FONTX2W42wire '+bytes([9,2,1,2])+struct.pack('<HHHH',0x8140,0x8141,0x82a0,0x82a0)+b'\xaa\x80\x55\0'*3
    yield 'fontx2','authored-single',sb
    yield 'fontx2','authored-double',db
    yield 'fontx2','authored-truncated',db[:-1]
    overlap = bytearray(db)
    overlap[22:26] = struct.pack('<HH',0x8141,0x8141)
    yield 'fontx2','authored-overlapping-blocks',bytes(overlap)
    for selector in [0,7,23,31,64,95,128,159,192,223,255]:
        w,h=selector%31+1,selector//8+1
        ww,hh=(h,w) if selector&128 else (w,h)
        stride=(ww+7)//8
        raw=bytearray((i*37)&255 for i in range(stride*hh*3))
        if ww%8:
            for i in range(stride-1,len(raw),stride):
                raw[i]&=255<<(8-ww%8)
        yield 'hzk',f'authored-geometry-{selector}',bytes([selector])+raw


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir',type=Path,required=True)
    args=parser.parse_args()
    result=[]
    for fmt,name,data in seeds():
        path=args.output_dir/fmt/name
        path.parent.mkdir(parents=True,exist_ok=True)
        if path.exists():
            assert path.read_bytes()==data,path
        else:
            path.write_bytes(data)
        result.append({'format':fmt,'seed':name,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()})
    print(json.dumps(result,indent=2))

if __name__=='__main__':
    main()
