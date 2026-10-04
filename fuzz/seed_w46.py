#!/usr/bin/env python3
"""Original MIT wire seeds for W4.6; writes only to the requested corpus root."""
import argparse,pathlib,struct

def p16(b,at,v,e='>'):struct.pack_into(e+'H',b,at,v)
def p32(b,at,v,e='>'):struct.pack_into(e+'I',b,at,v)
def gem(e,swapped=False,compressed=False):
    if compressed:
        b=bytearray(158)
        for at,v in [(2,3),(36,65),(38,65),(40,1),(42,1),(46,1),(48,1),(62,0x5555),(64,0x5555),(66,36),(80,2),(82,3)]:p16(b,at,v,e)
        p32(b,72,152,e);p32(b,76,156,e);p16(b,154,3,e);b[156:]=b'\x18\0'if e=='>'else b'\0\x18';return b
    b=bytearray(108);b[4:8]=b'Wire'
    for at,v in [(2,3),(36,65),(38,67),(40,1),(42,1),(44,1),(46,1),(48,1),(50,5),(52,5),(58,1),(60,1),(62,0x5555),(64,0x5555),(66,2|(0 if swapped else 4)),(80,2),(82,3)]:p16(b,at,v,e)
    for at,v in [(68,88),(72,94),(76,102)]:p32(b,at,v,e)
    b[88]=1;b[89]=255
    for i,v in enumerate([0,3,8,8]):p16(b,94+i*2,v,e)
    b[102:]=b'\xa7\0\x5b\0\xe1\0'
    if swapped:
        for i in range(102,108,2):b[i],b[i+1]=b[i+1],b[i]
    return b

def amiga():
    b=bytearray(172)
    for at,v in [(0,0x3f3),(8,1),(20,34),(24,0x3e9),(28,34),(32,0x70ff4e75),(168,0x3f2)]:p32(b,at,v)
    for at,v in [(18,0xf80),(78,3),(82,4),(84,1),(86,1),(96,2),(116,0),(118,3),(120,3),(122,0),(124,4),(126,0),(128,65535)]:p16(b,32+at,v)
    b[58:62]=b'Wire';b[113]=0x62;b[122]=b[123]=65
    for at,v in [(92,110),(98,116),(102,124),(106,128)]:p32(b,32+at,v)
    b[142:148]=b'\xa0\0\x40\0\xe0\0';return b

def snf(e,pad):
    name=b'_FONTPIXEL_SNF_BITMAP_FORMAT\0';bitmap=3*pad;props=124+(bitmap+3)//4*4;b=bytearray(props+12+len(name))
    for at,v in [(0,4),(4,1),(28,65),(32,65),(44,1),(48,len(name)),(52,65),(56,1),(60,2),(96,1),(100,1),(104,4)]:p32(b,at,v,e)
    for at in [64,80,108]:
        for i,v in enumerate([65535,2,4,2,1,3]):p16(b,at+i*2,v,e)
    p32(b,120,1<<24 if e=='<'else 128,e);p32(b,92,bitmap if e=='<'else bitmap<<8,e)
    for row,val in enumerate([0xa0,0x40,0xe0]):b[124+row*pad]=val
    p32(b,props+4,pad|0x100,e);b[props+12:]=name;return b

def main():
    p=argparse.ArgumentParser();p.add_argument('--output',type=pathlib.Path,required=True);a=p.parse_args()
    files={}
    for e,label in [('<','le'),('>','be')]:
        for swapped in [False,True]:files[f'gem/{label}-{int(swapped)}']=gem(e,swapped)
        files[f'gem/{label}-compressed']=gem(e,compressed=True)
        for pad in [1,4]:files[f'snf/{label}-pad{pad}']=snf(e,pad)
    files['amiga/hunk']=amiga();contents=bytearray(264);p16(contents,0,0xf00);p16(contents,2,1);contents[4:10]=b'wire/3';p16(contents,260,3);contents[263]=0x62;files['amiga/contents-with-sidecar']=contents+amiga()
    for name,b in files.items():path=a.output/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(b)
    print(f'{len(files)} original wire seeds written to {a.output}')
if __name__=='__main__':main()
