#!/usr/bin/env python3
"""Original Apple/Qt fixtures, authored from public field descriptions (MIT)."""
import argparse, binascii, hashlib, json, math, struct
from pathlib import Path
U16=lambda n:struct.pack('>H',n)
U32=lambda n:struct.pack('>I',n)
def nfnt():
    return bytes.fromhex('9000 0041 0042 0003 ffff ffff 0003 0003 000c 0002 0001 0001 0001 b800 c800 b800 0000 0002 0003 0005 0003 0102 0003 ffff')
def fork(kind,payload):
    data=U32(len(payload))+payload
    h=b''.join(map(U32,[256,256+len(data),len(data),50]))
    index=h+bytes(8)+U16(28)+U16(50)+U16(0)+kind+U16(0)+U16(10)+U16(128)+U16(65535)+bytes(8)
    return h+bytes(240)+data+index
def macbinary(payload):
    name=b'fbit-collision.bin';header=bytearray(128);header[1]=len(name);header[2:2+len(name)]=name;header[65:73]=b'FFILDMOV';header[87:91]=U32(len(payload));header[122:124]=bytes([129,129]);header[124:126]=U16(binascii.crc_hqx(header[:124],0))
    return bytes(header)+payload+bytes(-len(payload)%128)
def hqx(decoded):
    alphabet=b'!"#$%&\'()*+,-012345689@ABCDEFGHIJKLMNPQRSTUVXYZ[`abcdefhijklmpqr'
    escaped=b''.join(bytes([b,0]) if b==0x90 else bytes([b]) for b in decoded)
    bits=''.join(f'{b:08b}' for b in escaped)
    text=bytes(alphabet[int(bits[i:i+6].ljust(6,'0'),2)] for i in range(0,len(bits),6))
    return b'(This file must be converted with BinHex 4.0)\n\n:'+text+b':\n'
def binhex():
    data=b'\x90\x90\x90\0\xff';resource=fork(b'NFNT',nfnt())
    header=b'\x04Test\0FFILABCD\xa5\x5a'+U32(len(data))+U32(len(resource))
    return hqx(header+U16(binascii.crc_hqx(header,0))+data+U16(binascii.crc_hqx(data,0))+resource+U16(binascii.crc_hqx(resource,0)))
def fbit(width=3,height=3,count=2):
    b=bytearray(44);b[2:6]=b'fbit';b[6:8]=U16(128);b[24:28]=U16(height)+U16(width);b[32:36]=U16(0x817e)+U16(0x8180);b[36:44]=U16(1)+U16(62)+U16(61+count)+U16(count)
    cell=math.ceil(width*height/8)
    return bytes(b)+bytes([0xaf,0x80,0x72,0]) if (width,height,count)==(3,3,2) else bytes(b)+bytes((i*71+37)&255 for i in range(cell*count))
def hfnt():return U16(1)+U16(2)+U16(3)+U16(4)+U16(4)+U16(65)+U16(0x8140)+U16(0)+bytes.fromhex('8142241818244281 ff00ff00ff00ff00')
def lisa(compressed=False):
    raster=nfnt()[:-2]
    if compressed:
        raw=bytearray(raster);raw[30]^=raw[28];raw[28]^=raw[26];body=raw[2:];length=len(body);body+=bytes(-len(body)%8)
        payload=bytearray()
        for i in range(0,len(body),8):
            mask=0
            for bit,value in enumerate(body[i:i+8]):
                if i<24 and value==0:mask|=0x80>>bit
                else:payload.append(value)
            payload.append(mask)
        raster=b'\x90\x80'+U32(len(payload))+U32(length)+payload
    end=math.ceil(len(raster)/2)
    return U16(11)+U16(1)+b'\x03F.F'+b''.join(map(U32,[0,end,end,end]))+bytes(2)+raster+bytes(len(raster)%2)
def mgtk(double=True):return bytes([0x80,1,2,7,10,0x41,0x2a,0,5,0x22,0x55,0,2]) if double else bytes([0,1,2,7,3,0x41,5,0x22,2])
def qpf(gray=False):
    if gray:return bytes([2,0,0,0,3,0,1,0,0,0,0,65,0,65,0,4,3,2,0,0,3,2,0,0,127,255,9,1,128,254,8])
    return bytes([3,1,255,0,4,255,0,1,1,7,0,66,0,66,3,0,65,0,65,0,0xd8,0,0xd8,0,0,2,3,2,9,255,4,3,8,1,2,1,0,0,2,1,0,1,0,3,0,0,1,0,0,0xa0,0x77,0x60,0x88,0xc0,0x11,0x22,0x33])
def qpf2(supplementary=False):
    tags=[(0,b'Original test'),*[(t,struct.pack('>i',v)) for t,v in [(5,192),(6,64),(7,-64),(11,64),(14,64)]],(15,b'\x08'),(16,b'\x04'),(99,b'opaque'),(19,b'')]
    header=b''.join(U16(t)+U16(len(v))+v for t,v in tags);header+=bytes(-len(header)%4)
    codes=[65,90,0x1f600] if supplementary else [65,90]
    cmap=U16(0)+U16(1)+U16(3)+U16(10)+U32(12)+U16(12)+U16(0)+U32(16+12*len(codes))+U32(0)+U32(len(codes))+b''.join(U32(cp)+U32(cp)+U32(1) for cp in codes)
    blocks=[(0,cmap),(1,U32(0xffffffff)+U32(0)+U32(14)),(2,bytes([3,2,4,255,253,252,0,127,255,9,1,128,254,8,0,3,0,0,0,0])),(25,b'RAW!')]
    return b'QPF2\xff\xff\xff\xff\x02\0'+U16(len(header))+header+b''.join(U16(t)+U16(0)+U32(len(v))+v for t,v in blocks)
def fixtures():
    return {'fbit':{'inline':fbit(),'hfnt':hfnt(),'resource-fork':fork(b'fbit',fbit()),'macbinary-name':macbinary(fork(b'fbit',fbit())),'large':fbit(16,16,256)},'lisa':{'plain':lisa(),'compressed':lisa(True)},'mgtk':{'single':mgtk(False),'double':mgtk(),'zero-width':bytes([0,0,16,0])+bytes(16),'large':bytes([0x80,255,16])+bytes([14])*256+bytes((i*37)&127 for i in range(8192))},'qpf':{'mono':qpf(),'gray':qpf(True)},'qpf2':{'gray':qpf2(),'supplementary':qpf2(True)},'nfnt':{'binhex':binhex()}}
def benchmark_fixtures():
    # Separate from mutation seeds: original fixed workload sizes for scaling.
    out={}
    for count in [256,2048]:
        out[f'qpf-{count}']=bytes([16,0,0,0,8,0,0,1,1,0])+U16(256)+U16(255+count)+bytes(1)+bytes([1,8,16,0,0,8,16,0])*count+bytes((i*37)&255 for i in range(16*count))
        tags=[(0,b'Original benchmark'),(5,struct.pack('>i',1024)),(6,bytes(4)),(7,bytes(4)),(11,struct.pack('>i',64)),(14,struct.pack('>i',64)),(15,b'\x08'),(16,b'\x10'),(19,b'')]
        header=b''.join(U16(t)+U16(len(v))+v for t,v in tags);header+=bytes(-len(header)%4)
        cmap=U16(0)+U16(1)+U16(3)+U16(10)+U32(12)+U16(12)+U16(0)+U32(28)+U32(0)+U32(1)+U32(256)+U32(255+count)+U32(1)
        glyph=bytes([8,16,8,0,240,8])+bytes((i*37)&255 for i in range(128))
        blocks=[(0,cmap),(1,U32(0xffffffff)+b''.join(U32(i*len(glyph)) for i in range(count))),(2,glyph*count)]
        out[f'qpf2-{count}']=b'QPF2\xff\xff\xff\xff\x02\0'+U16(len(header))+header+b''.join(U16(t)+U16(0)+U32(len(v))+v for t,v in blocks)
    count=256;height=16;row_words=128
    pixels=bytes((i*37)&255 for i in range(row_words*2*height))
    locations=b''.join(U16(i*8) for i in range(count+1))
    ow=(26+len(pixels)+len(locations)-16)//2
    raster=b''.join(U16(n) for n in [0x9000,0,254,8,0,0xffff,8,height,ow,16,0,0,row_words])+pixels+locations+U16(8)*count+U16(0xffff)
    end=len(raster)//2
    out['lisa-256']=U16(11)+U16(1)+b'\x03F.F'+b''.join(map(U32,[0,end,end,end]))+bytes(2)+raster
    out['fbit-256']=fbit(16,16,256)
    out['mgtk-256']=fixtures()['mgtk']['large']
    return out
def main():
    p=argparse.ArgumentParser();p.add_argument('output',type=Path);p.add_argument('--manifest',type=Path);args=p.parse_args();records=[]
    for fmt,items in fixtures().items():
        for name,data in items.items():
            path=args.output/fmt/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(data);records.append({'format':fmt,'name':name,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()})
    for name,data in benchmark_fixtures().items():
        path=args.output/'benchmark'/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(data);records.append({'format':name.split('-')[0],'name':name,'scope':'benchmark','bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()})
    text=json.dumps({'license':'MIT; original field-based fixtures; no third-party artwork','seeds':records},indent=2)+'\n'
    if args.manifest:args.manifest.write_text(text)
    else:print(text,end='')
if __name__=='__main__':main()
