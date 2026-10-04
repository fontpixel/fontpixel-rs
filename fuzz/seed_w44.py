#!/usr/bin/env python3
"""Original W4.4 corpus; BMFont input is u32LE descriptor size + descriptor + PNG."""
import argparse
import base64
import struct
import zlib
from pathlib import Path

def png():
    def chunk(tag,b):return struct.pack('>I',len(b))+tag+b+struct.pack('>I',zlib.crc32(tag+b))
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>2I5B',3,2,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(b'\0'+bytes([0,127,255,255,255,0,128,128,255,255,0,1])+b'\0'+bytes([255,0,0,255,0,255,0,128,0,0,255,0])))+chunk(b'IEND',b'')

def main():
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('output',type=Path);a=ap.parse_args()
    for name in ['bmfont','sfd','kbits']:(a.output/name).mkdir(parents=True,exist_ok=True)
    info='info face="Original" size=8 bold=0 italic=0 charset="" unicode=1 stretchH=100 smooth=1 aa=1 padding=0,0,0,0 spacing=0,0 outline=0'
    common='common lineHeight=10 base=7 scaleW=3 scaleH=2 pages=1 packed=1 alphaChnl=0 redChnl=0 greenChnl=0 blueChnl=0'
    for mask in [1,2,4,8,15]:
        desc=f'{info}\n{common}\npage id=0 file="page.png"\nchars count=1\nchar id=65 x=0 y=0 width=3 height=2 xoffset=-1 yoffset=2 xadvance=4 page=0 chnl={mask}\nkernings count=1\nkerning first=65 second=65 amount=-1\n'.encode()
        (a.output/'bmfont'/f'text-{mask}').write_bytes(struct.pack('<I',len(desc))+desc+png())
    desc=b'<font><info face="Original &amp; Gray" size="8" unicode="True"/><common lineHeight="10" base="7" scaleW="3" scaleH="2" pages="1"/><pages><page id="0" file="page.png"/></pages><chars count="1"><char id="65" x="0" y="0" width="3" height="2" xoffset="-1" yoffset="2" xadvance="4" page="0" chnl="8"/></chars></font>'
    (a.output/'bmfont'/'xml').write_bytes(struct.pack('<I',len(desc))+desc+png())
    def block(tag,b):return bytes([tag])+struct.pack('<I',len(b))+b
    desc=b'BMF\3'+block(1,struct.pack('<hBBH8B',8,3,0,100,1,0,0,0,0,0,0,0)+b'Original\0')+block(2,struct.pack('<5H5B',10,7,3,2,1,128,0,0,0,0))+block(3,b'page.png\0')+block(4,struct.pack('<I4H3h2B',65,0,0,3,2,-1,2,4,0,8))+block(5,struct.pack('<IIh',65,65,-1))
    (a.output/'bmfont'/'binary').write_bytes(struct.pack('<I',len(desc))+desc+png())
    for depth in [1,2,4,8]:
        pixels=bytes([128,64,255,0]) if depth!=8 else bytes([0,127,128,255])
        width=8//depth
        desc=f'SplineFontDB: 3.0\nFontName: Original\nAscent: 7\nDescent: 3\nBeginChars: 1 1\nStartChar: A\nEncoding: 65 65 0\nWidth: 9\nEndChar\nEndChars\nBitmapFont: 10 1 7 3 {depth}\nBDFChar: 0 65 9 0 {width-1} -1 2\n'+base64.a85encode(pixels).decode()+'\nEndBitmapFont\nEndSplineFont\n'
        (a.output/'sfd'/f'depth-{depth}').write_text(desc)
    b=b'KBnPbits'+struct.pack('>7i',1,7,3,8,4,2,5)+b'name'+struct.pack('>iiH',1,1,9)+b'A\xc0\x80\xed\xa0\xbd\xed\xb8\x80'+b'char'+struct.pack('>7i',1,65,5,-2,4,2,3)+bytes([0,127,255])+struct.pack('>i',3)+bytes([1,128,254])+b'fin.'
    (a.output/'kbits'/'gray-names').write_bytes(b)
    print(a.output)
if __name__=='__main__':main()
