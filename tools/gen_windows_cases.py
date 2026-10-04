#!/usr/bin/env python3
"""Generate original independent FNT/NE/PE seeds; no reference fonts are used."""
from pathlib import Path
import struct
ROOT=Path(__file__).resolve().parents[1]
def u16(b,p,v):struct.pack_into('<H',b,p,v)
def u32(b,p,v):struct.pack_into('<I',b,p,v)
def fnt(version=2):
    header={1:117,2:118,3:148}[version];entry={1:2,2:4,3:6}[version]
    bits=header+3*entry;data=bytes.fromhex('80e040007f20400000604000') if version==1 else bytes.fromhex('807f00800000c040c0808080000000000000')
    b=bytearray(bits+len(data)+5);u16(b,0,version*256);u32(b,2,len(b));u16(b,68,3);u16(b,70,72);u16(b,72,72);u16(b,74,2);u16(b,83,400);u16(b,88,3);b[90]=1;u16(b,91,9);u16(b,93,9);b[95:97]=bytes([65,66]);u16(b,99,4);u32(b,105,bits+len(data));u32(b,113,bits)
    for i in range(3):
        if version==1:u16(b,header+i*entry,9*i)
        else:
            u16(b,header+i*entry,9)
            (u16 if version==2 else u32)(b,header+i*entry+2,bits+6*i)
    b[bits:bits+len(data)]=data;b[bits+len(data):]=b'Test\0';return b

def pe(payload,plus=False):
    opt=240 if plus else 224;b=bytearray(1024+len(payload));b[:2]=b'MZ';u32(b,60,128);b[128:132]=b'PE\0\0';u16(b,134,1);u16(b,148,opt);o=152;u16(b,o,0x20b if plus else 0x10b);dirs=112 if plus else 96;u32(b,o+dirs-4,16);u32(b,o+dirs+16,0x2000);u32(b,o+dirs+20,88);s=o+opt;b[s:s+5]=b'.rsrc';u32(b,s+8,len(b)-512);u32(b,s+12,0x2000);u32(b,s+16,len(b)-512);u32(b,s+20,512)
    for p in [526,550,574]:u16(b,p,1)
    for p,v in [(528,8),(532,0x80000018),(552,1),(556,0x80000030),(576,0),(580,72),(584,0x2200),(588,len(payload))]:u32(b,p,v)
    b[1024:]=payload;return b

def ne(payload):
    b=bytearray(256+((len(payload)+15)//16)*16);b[:2]=b'MZ';u32(b,60,128);b[128:130]=b'NE';u16(b,164,64);u16(b,166,90);b[182]=2;u16(b,192,4);u16(b,194,0x8008);u16(b,196,1);u16(b,202,16);u16(b,204,(len(payload)+15)//16);u16(b,208,0x8001);b[256:256+len(payload)]=payload;return b

def main():
    for name in ['fnt','fon']:(ROOT/'fuzz/corpus'/name).mkdir(parents=True,exist_ok=True)
    for version in [1,2,3]:
        payload=fnt(version);(ROOT/'fuzz/corpus/fnt'/f'v{version}').write_bytes(payload)
        (ROOT/'fuzz/corpus/fon'/f'ne-v{version}').write_bytes(ne(payload))
        for plus in [False,True]:(ROOT/'fuzz/corpus/fon'/f'pe{64 if plus else 32}-v{version}').write_bytes(pe(payload,plus))
if __name__=='__main__':main()
