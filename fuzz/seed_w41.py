#!/usr/bin/env python3
"""Generate independently authored draw/raw/CPI/CPX fuzz inputs (MIT)."""
import argparse
from pathlib import Path
import struct
import subprocess
import zlib


def cp(page=437,height=8,count=256):
    return bytearray(struct.pack('<HIH8sH6sIHHHBBBBH',28,0,1,b'EGA     ',page,bytes(6),28,1,1,6+height*count,height,8,0,0,count)+bytes((c*37+y*11)&255 for c in range(count) for y in range(height)))


def cpi():
    body=cp()
    body[24:28]=struct.pack('<I',53)
    return b'\xffFONT   '+bytes(8)+struct.pack('<HBIH',1,1,23,1)+body


def cpx(data):
    output=bytearray()
    at=used=0
    def bit(one):
        nonlocal at,used
        if used==0:
            at=len(output)
            output.extend(b'\0\0')
        n=int.from_bytes(output[at:at+2],'little')
        if one:
            n|=1<<(15-used)
        output[at:at+2]=n.to_bytes(2,'little')
        used=(used+1)%16
    def number(n):
        for shift in range(n.bit_length()-2,-1,-1):
            bit((n>>shift)&1)
            bit(shift==0)
    pos=0
    while pos<len(data):
        count=0
        if pos:
            while pos+count<len(data) and data[pos+count]==data[pos-1] and count<1024:
                count+=1
        if count>=2:
            bit(0)
            number(2)  # repeat the previous match distance (initially one)
            if count<=4:
                bit((count-1)>>1)
                bit((count-1)&1)
            else:
                bit(0)
                bit(0)
                number(count-3)
            pos+=count
        else:
            bit(1)
            output.append(data[pos])
            pos+=1
    bit(0)
    for shift in range(23,-1,-1):
        bit((0x1000002>>shift)&1)
        bit(shift==0)
    output.append(255)
    header=bytearray(b'UPX!'+bytes([11,1,4,1])+struct.pack('<IIHHB',zlib.adler32(data),zlib.adler32(output),len(data),len(output),0))
    header.append(sum(header[4:])%251)
    return header+output


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('output',type=Path)
    p.add_argument('--writer',type=Path,help='optional w41_timing executable to add a native executable CPX seed')
    a=p.parse_args()
    for fmt in ['draw','raw','cpi']:
        (a.output/fmt).mkdir(parents=True,exist_ok=True)
    for name,data in [('ordinary',b'% generated\n0041:\t--##----\n\t-#--#---\n\n'),('supplementary',b'1F600:\n###\n-#-\n\n'),('malformed',b'D800:\n#-\n'),('comment',b'% comment\n0042:\t-#\n\t#-\n')]:
        (a.output/'draw'/name).write_bytes(data)
    for w,h,padding,offset in [(8,8,0,0),(9,3,3,2),(16,16,16,0),(1,1,8,3)]:
        stride=(w+7)//8+(padding&7)
        data=bytes([w-1,h-1,padding,offset])+bytes(offset)+bytes((i*37+11)&255 for i in range(stride*h*8))
        (a.output/'raw'/f'{w}-{h}-{padding}').write_bytes(data)
    (a.output/'cpi'/'cp').write_bytes(cp())
    (a.output/'cpi'/'font').write_bytes(cpi())
    compressible=bytearray(cpi());compressible[65:]=bytes(len(compressible)-65)
    (a.output/'cpi'/'font-compressible').write_bytes(compressible)
    (a.output/'cpi'/'cpx-literals').write_bytes(cpx(cpi()))
    nt=bytearray(cpi());nt[1:8]=b'FONT.NT';nt[49:53]=struct.pack('<I',28)
    (a.output/'cpi'/'font-nt').write_bytes(nt)
    # DRFONT: one size, nonidentity reverse mapping into the shared table.
    dr=bytearray(b'\x7fDRFONT '+bytes(8)+struct.pack('<HBI',1,1,29))
    dr+=bytes([1,8])+struct.pack('<I',31+28+6+6+512)+struct.pack('<H',1)
    body=cp();body[24:28]=struct.pack('<I',59);body[28:34]=struct.pack('<HHH',2,1,6)
    dr+=body[:40]+b''.join(struct.pack('<H',255-i) for i in range(256))+body[40:]
    (a.output/'cpi'/'drfont').write_bytes(dr)
    if a.writer:
        result=subprocess.run([str(a.writer.resolve()),'convert','cpi','cpx',str(a.output/'cpi'/'font-compressible'),str(a.output/'cpi'/'native-cpx')],capture_output=True)
        if result.returncode:
            raise RuntimeError(result.stderr.decode()[-1024:])


if __name__=='__main__':
    main()
