#!/usr/bin/env python3
"""Original MIT wire fixtures for W5.3 fuzzing; no third-party font data."""
import pathlib, struct, argparse

def vfont(endian='<'):
    data=bytearray(2573)
    struct.pack_into(endian+'5H',data,0,0o436,3,3,3,7)
    struct.pack_into(endian+'Hh4bh',data,10+65*10,0,3,2,1,1,2,5)
    struct.pack_into(endian+'h',data,10+32*10+8,3)
    data[2570:]=b'\xa0\x40\xe0'
    return bytes(data)

def fzx():
    return bytes([8,2,34,11,0,2,8,0xc0,0x18,9,0,0x21,10,0,0x80,0,0x40,0x80,0xc0,0,0x80,0x40])

def figlet(art=False):
    data=bytearray(b'flf2a$ 2 1 5 -1 1 0 0 3\nOriginal MIT fixture\n')
    for code in [*range(32,127),196,214,220,228,246,252,223]:
        data.extend((b'A$ @\n ##@@\n' if art else b'## @\n ##@@\n') if code==65 else b'@\n@@\n')
    data.extend(b'0 missing\n#@\n#@@\n0x1F600 face\n #@\n# @@\n-02 map\n#@\n @@\n')
    return bytes(data)

def vfont_full():
    data=bytearray(2570+256*16)
    struct.pack_into('<5H',data,0,0o436,4096,16,8,0)
    for code in range(256):
        struct.pack_into('<Hh4bh',data,10+code*10,code*16,16,6,2,1,15,18)
        data[2570+code*16:2570+(code+1)*16]=bytes(((code+i)*37&255 for i in range(16)))
    return bytes(data)

def fzx_full():
    data=bytearray([8,2,255])+bytearray(224*3+2)
    for index in range(224):
        at=3+index*3;width=1+index%16
        struct.pack_into('<H',data,at,len(data)-at|((index%4)<<14));data[at+2]=width-1
        for row in range(8):
            bits=((index+row)*7331)&((1<<width)-1)
            data.extend((bits<<((8-width%8)%8)).to_bytes((width+7)//8,'big'))
    struct.pack_into('<H',data,3+224*3,len(data)-(3+224*3))
    return bytes(data)

def figlet_full():
    data=bytearray(b'flf2a$ 8 6 18 -1 0 0 0 256\n')
    for code in [*range(32,127),196,214,220,228,246,252,223,*range(256,512)]:
        if code>=256:data.extend(f'{code}\n'.encode())
        width=0 if code==32 else 1+code%16
        for row in range(8):
            data.extend(bytes((35 if ((code*37+row*7)>>(col%8))&1 else 32 for col in range(width))))
            data.extend(b'@@\n'if row==7 else b'@\n')
    return bytes(data)

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--root',type=pathlib.Path,default=pathlib.Path(__file__).parent/'corpus');a=p.parse_args()
    for fmt,fixtures in [('vfont',{'little':vfont(),'big':vfont('>'),'full256':vfont_full()}),('fzx',{'proportional':fzx(),'full224':fzx_full()}),('figlet',{'canonical':figlet(),'art':figlet(True),'crlf':figlet().replace(b'\n',b'\r\n'),'full358':figlet_full()})]:
        directory=a.root/fmt;directory.mkdir(parents=True,exist_ok=True)
        for name,data in fixtures.items():(directory/name).write_bytes(data)
