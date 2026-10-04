#!/usr/bin/env python3
"""Original MIT byte-authored C1 seeds, independent of native encoders."""
import argparse,pathlib,struct

def daisy(version='v3',dense=False,part=0):
    codes=[c for c in range(32,125) if c not in (96,123)]
    out=bytearray(b'DAISY-DOT NLQ FONT\x9b' if version=='v2' else b'B\x9b' if version=='mag' else b'3\x9b')
    rows=16 if version=='v2' else 32
    for cp in codes:
        if cp==32 and version!='v2':continue
        out.append(2+(64 if rows==32 else 0))
        raw=bytearray(rows//8*2)
        if cp==65 or dense:
            raw[0]=0x80;raw[1]=0x40;raw[-2]=0x20 if part==0 else 0x10
        out+=raw
        if version=='v2':out.append(155)
    if version!='v2':out+=bytes((63 if version=='mag' and not part else 31,30,2))
    return bytes(out)

def polyprint(dense=False):
    out=bytearray(12224);struct.pack_into('<H',out,0,0x1234);out[2]=25;out[3:28]=b'ORIGINAL C1'.ljust(25,b' ');out[29]=32
    for cp in range(32,176):
        at=128+(cp-32)*84;out[at]=1;out[at+21]=3;out[at+42]=1
        if cp==65 or dense:out[at+1]=0x80;out[at+2]=0x40;out[at+43]=0x20
    return bytes(out)

def printshop(dense=False):
    header=bytearray(601);header[0:2]=bytes((9,3));header[581:590]=b'C1 PNF   ';header[592]=3
    data=bytearray()
    for cp in range(32,127):
        i=cp-32;struct.pack_into('<H',header,194+i*2,len(data))
        if cp==65 or dense:header[4+i]=9;header[99+i]=3;data+=bytes((0x80,0x80,0x40,0,0xff,0x80))
    return bytes(header+data)

def tasprint(version='48k',dense=False):
    count,cols,first={'48k':(96,10,32),'+3':(96,16,32),'cpc':(256,10,0),'pcw':(128,16,1),'pcw127':(127,16,1)}[version]
    data=bytearray(count*cols*2)
    for cp in range(first,first+count):
        if cp==65 or dense:
            at=(cp-first)*cols*2;data[at]=0x80;data[at+1]=0x40;data[at+cols+2]=0x20
    if version=='+3':
        header=bytearray(130);header[:9]=b'PLUS3DOS\x1a';header[9]=1;struct.pack_into('<IBHHH',header,11,3202,3,3074,30000,21506);struct.pack_into('<H',header,128,16);header[127]=sum(header[:127])%256;data=header+data
    if version=='cpc':
        header=bytearray(128);header[1:12]=b'C1      DAT';header[18]=2;struct.pack_into('<H',header,24,5120);struct.pack_into('<H',header,64,5120);struct.pack_into('<H',header,67,sum(header[:67]));data=header+data
    return bytes(data)

def pxl(version=1001,dense=False):
    raster=bytearray();directory=bytearray(2048);unit=4 if version==1001 else 1
    for cp in range(128):
        if cp==65 or dense:
            ptr=(4+len(raster))//unit
            raw=bytes((0xa0,0,0,0,0x40,0,0,0,0xe0,0,0,0)) if unit==4 else bytes((0xa0,0x40,0xe0))
            raster+=raw;struct.pack_into('>HHhhII',directory,cp*16,3,3,1,2,ptr,1<<19)
    offset=(4+len(raster))//unit
    return struct.pack('>I',version)+raster+directory+struct.pack('>IIIII',0x12345678,1000,10<<20,offset,version)

def tfm():
    counts=[6+2+128+2+3,2,0,127,2,1,1,1,0,0,0,0]
    b=bytearray(struct.pack('>12H',*counts)+struct.pack('>II',0x12345678,10<<20)+bytes(512)+struct.pack('>iiIII',0,1<<19,0,0,0));b[32+65*4]=1;return bytes(b)

def signum(version='e24',dense=False):
    magic,baseline={'e24':(b'eset0001',18),'p9':(b'ps090001',36),'p24':(b'ps240001',58),'l30':(b'ls300001',48)}[version]
    header=bytearray(652);header[:8]=magic;struct.pack_into('>I',header,8,128);data=bytearray()
    for cp in range(127):
        struct.pack_into('>I',header,144+cp*4,len(data))
        if cp==65 or dense:
            data+=bytes((baseline-3,3,5 if version=='e24' else 1,0));data+=bytes((0xa0,0,0x40,0,0xe0,0)) if version=='e24' else bytes((0xa0,0x40,0xe0,0))
        else:data+=b'\0'*4
    struct.pack_into('>I',header,140,len(data));return bytes(header+data)

def cases():
    for version in ['v2','v3','mag']:yield 'daisy',version,daisy(version)
    for fmt,fn in [('polyprint',polyprint),('printshop',printshop)]:
        for dense in [False,True]:yield fmt,'dense' if dense else 'small',fn(dense)
    for v in ['48k','+3','cpc','pcw','pcw127']:yield 'tasprint',v,tasprint(v)
    for v in [1001,1002]:
        for dense in [False,True]:yield 'pxl',str(v)+('-dense' if dense else ''),pxl(v,dense)
    for v in ['e24','p9','p24','l30']:yield 'signum',v,signum(v)
    yield 'daisy','v3-dense',daisy('v3',True)
    yield 'tasprint','48k-dense',tasprint('48k',True)
    yield 'signum','e24-dense',signum('e24',True)
    yield 'signum','p24-dense',signum('p24',True)

def main():
    p=argparse.ArgumentParser();p.add_argument('output',type=pathlib.Path);a=p.parse_args()
    for fmt,label,data in cases():
        out=a.output/fmt;out.mkdir(parents=True,exist_ok=True);(out/label).write_bytes(data)
    px=pxl();(a.output/'pxl'/'tfm-frame').write_bytes(b'TFM!'+struct.pack('>I',len(px))+px+tfm())
if __name__=='__main__':main()
