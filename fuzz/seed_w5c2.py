#!/usr/bin/env python3
"""Original MIT C2 byte-authored seeds from primary format field definitions."""
import argparse,pathlib,struct

def p16(b,a,n,e='<'):struct.pack_into(e+'H',b,a,n&65535)
def p32(b,a,n,e='<'):struct.pack_into(e+'I',b,a,n&0xffffffff)
def catalyst(data=False,dense=False):
    b=bytearray(387);raw=bytearray()
    for cp in range(128):
        p16(b,cp*2,len(raw)+(0 if data else 387))
        if cp in [65,66] or dense:
            b[256+cp]=9;raw+=bytes((0x81,1,2,0,0x7f,1))
    b[384:387]=bytes((3,2,1));b+=raw
    return bytes((1,))+struct.pack('<H',len(b))+b

def chiwriter(v=4,layout='contiguous',stride=2,dense=False):
    count=94 if dense else 2;first=33 if dense else 65
    if v==3 and layout!='contiguous' and dense:count=80
    b=bytearray(344+count*stride*3);b[0]=16 if v==3 else 17;b[1:14]=b'ORIGINAL.SET ';b[15]=2;b[16]=count;b[17]=first-32;b[19]=9;b[20]=3;b[22]=3
    if v==3:b[24]=3
    for i in range(count):
        b[(250 if v==3 else 88+first)+i]=9
        if layout!='contiguous':p16(b,61+i*2,344+i*stride*3 if layout=='absolute' else i*stride*3)
        for y,row in enumerate([bytes((128,128)),bytes((64,0)),bytes((255,128))]):b[344+(i*3+y)*stride:346+(i*3+y)*stride]=row
    return bytes(b)

def command(prefix,data):return b'\x1b'+prefix+str(len(data)).encode()+b'W'+data
def hppcl(rle=False,legacy=False,dense=False):
    h=bytearray(64);h[1]=64;h[3]=2;h[7]=3;h[9]=9;h[11]=4;h[13]=1;h[19]=16;h[33]=16;h[37]=65;h[39]=65;h[48:52]=b'TEST'
    out=bytearray(command(b')s',h))
    for cp in range(32,127) if dense else [65]:
        d=bytearray((4,0,16 if legacy else 14,2 if rle else 1,0,0,255,255,0,1,0,9,0,3,0,40))
        d+=bytes((0,0,1,7,1,0,1,1,7,0,0,9)) if rle else bytes((128,128,64,0,255,128))
        out+=b'\x1b*c'+str(cp).encode()+b'E'+command(b'(s',d)
    return bytes(out)

def dashen(v4=False,dense=False):
    b=bytearray(1070);b[4]=14;b[6]=9;b[8]=4 if v4 else 3;b[12]=3;b[14:1038]=b'\xff'*1024;raw=bytearray()
    for cp in range(32,127) if dense else [65,66]:
        p32(b,14+cp*4,len(raw));raw+=bytes((9,1,3,128,64,32,0,0,0,0,0,128))
    b+=raw
    if v4:b+=bytes(range(256))+hppcl(dense=dense)
    return bytes(b)

def prebuilt(depth=1,little=False,vertical=False,dense=False):
    e='<' if little else '>';names=[f'g{i:03}' for i in range(96)] if dense else ['A'];n=len(names);nt='\0'.join(names).encode()+b'\0';na=60;wa=na+len(nt);ma=wa+8*n;ga=ma+28;va=ga+12*n;da=va+(12*n if vertical else 0);row=(3*depth+7)//8;b=bytearray(da+n*2*row)
    for a,x in [(0,0 if little else 1),(2,3),(16,n),(18,1)]:p16(b,a,x,e)
    for a,x in [(4,42),(8,52),(12,57),(20,wa),(24,na),(28,len(nt)),(32,ma),(36,ga),(40,va if vertical else 0),(44,va+8*n if vertical else 0),(48,n*2*row)]:p32(b,a,x,e)
    b[52:60]=b'TEST\0CS\0';b[na:wa]=nt
    for a,x in [(ma,12*65536),(ma+12,12*65536),(ma+24,depth)]:p32(b,a,x,e)
    for i in range(n):
        p32(b,wa+i*8,32768,e);b[ga+i*12:ga+i*12+6]=bytes((3,2,1,1,4,1));p32(b,ga+i*12+8,da+i*row*2,e)
        if vertical:p32(b,va+i*8+4,-65536,e);b[va+8*n+i*4:va+8*n+i*4+4]=bytes((2,3,0,252))
        for y,values in enumerate([[0,1,1],[1,0,0]]):
            for x,v in enumerate(values):
                bit=x*depth;shift=8-depth-bit%8;b[da+i*row*2+y*row+bit//8]|=(v*((1<<depth)-1))<<shift
    return bytes(b)

def riscos(version=6,packed=False,dense=False,phases=False,wide=False):
    directory=64 if version==8 else 16;first=100 if version==8 else 64;nf=16 if phases else 1;flag=4 if version>=7 else 0
    chunks=[]
    for ci in range(8):
        glyphs=[cp for cp in (range(32,127) if dense else [65]) if cp//32==ci]
        if not glyphs:chunks.append(b'');continue
        chunk=bytearray(flag+128*nf)
        if flag:p32(chunk,0,0x80000000+(3 if phases else 0))
        for phase in range(nf):
            for cp in glyphs:
                p32(chunk,flag+phase*128+(cp%32)*4,len(chunk)-flag)
                if packed:record=bytes((0xc6,255,255,9,3,0x19,0x71,0x71,0x01)) # runs from bottom:9 black,1 white,1 black,7 white,1 black,7 white,1 black
                elif wide:record=bytes((3,0x7f,0xef,0xf7,0x82,0x30,0))+bytes([1])+bytes(48)
                else:record=bytes((2,255,255,9,3,255,5,4,4))
                chunk+=record;chunk+=bytes((-len(chunk))%4)
        chunks.append(bytes(chunk))
    b=bytearray(first);b[:4]=b'FONT';b[4]=1;b[5]=version;b[6]=(64 if flag else 0)+(3 if phases else 0)
    for a,x in [(8,65535),(10,65535),(12,9),(14,3),(52,10),(54,192),(56,72),(58,192),(60,72)]:p16(b,a,x)
    if version==8:p32(b,16,64);p32(b,20,8)
    for ci,chunk in enumerate(chunks):p32(b,directory+ci*4,len(b));b+=chunk
    p32(b,directory+32,len(b));return bytes(b)

def xy():
    b=bytearray(560);b[:4]=bytes((12,4,90,45));p32(b,8,20);p32(b,12,540)
    for a,x in [(20,12*16*90),(24,12*16*45),(28,90),(32,45)]:p32(b,a,x)
    b[36:40]=bytes((255,255,2,1));p16(b,40+65*2,512);b[552:559]=bytes((255,255,3,2,0x0f,0,0xff));return bytes(b)

def alto():
    b=bytearray(152);p16(b,0,5,'>');b[2:4]=bytes((132,10))
    for cp in range(66):p16(b,4+cp*2,(138-4-cp*2)//2,'>')
    p16(b,138,1,'>');p16(b,134,7,'>');b[142:148]=bytes((128,128,64,0,255,128));p16(b,148,21,'>');b[150:]=bytes((1,3));return bytes(b)

def bitblt(dense=False):
    if dense:
        count=95;row=(count*9+1+15)//16*2;size=10+row*3+(count+2)*2;b=bytearray(8+size)
        for a,x in [(0,0x8000),(2,32),(4,126),(6,9),(8,size//2),(10,2),(12,1),(16,row//2)]:p16(b,a,x,'>')
        for y in range(3):
            for cp in range(count):
                for x in ([0,8] if y==0 else [1] if y==1 else range(9)):
                    bit=cp*9+x;b[18+y*row+bit//8]|=128>>(bit%8)
        for i in range(count+1):p16(b,18+row*3+i*2,i*9,'>')
        p16(b,18+row*3+(count+1)*2,count*9+1,'>');return bytes(b)
    b=bytearray(30)
    for a,x in [(0,0x8000),(2,65),(4,65),(6,9),(8,11),(10,2),(12,1),(16,1),(24,0),(26,9),(28,10)]:p16(b,a,x,'>')
    b[18:24]=bytes((128,192,64,64,255,192));return bytes(b)

def prepress():
    b=bytearray(76)
    for a,x in [(0,0x100c),(2,1),(24,0x300b),(30,423),(42,720),(44,720),(46,1),(56,65535),(58,65535),(60,3),(62,3),(68,0x0403)]:p16(b,a,x,'>')
    for a,x in [(34,24),(38,14),(48,4*65536),(52,-65536),(64,2)]:p32(b,a,x,'>')
    b[4]=4;b[5:9]=b'TEST';b[26]=1;b[28:30]=bytes((65,65));b[70:]=bytes((0xa0,0,0xc0,0,0x80,0));return bytes(b)

def intmetrics():
    n=66;b=bytearray(54+n*12);b[:40]=b'TEST'+bytes([13])*36;p32(b,40,16);p32(b,44,16);b[48]=n;b[49]=2;b[50]=32;p16(b,54+n*8+65*2,500);return bytes(b)

def cases():
    for dense in [False,True]:
        yield 'catalyst','dense' if dense else 'small',catalyst(dense=dense)
        yield 'dashen','dense' if dense else 'v3',dashen(dense=dense)
        yield 'hppcl','dense' if dense else 'raw',hppcl(dense=dense)
        if dense:yield 'prebuilt','dense',prebuilt(dense=True)
    yield 'catalyst','data',catalyst(True)
    for v in [3,4]:yield 'chiwriter',f'v{v}',chiwriter(v)
    yield 'chiwriter','dense',chiwriter(3,dense=True)
    for layout in ['absolute','relative']:yield 'chiwriter',layout+'-stride3',chiwriter(3,layout,3)
    yield 'dashen','v4',dashen(True)
    yield 'hppcl','rle',hppcl(True)
    yield 'hppcl','legacy',hppcl(legacy=True)
    for little in [False,True]:
        for depth in [1,2,4,8]:yield 'prebuilt',('le' if little else 'be')+f'-{depth}',prebuilt(depth,little)
    yield 'prebuilt','vertical',prebuilt(vertical=True)
    gray=bytearray(prebuilt(8));gray[-6:]=bytes((0,17,255,128,33,1));yield 'prebuilt','be-8-gray',bytes(gray)
    for v in [4,5,6,7,8]:yield 'riscos',f'v{v}',riscos(v)
    yield 'riscos','dense',riscos(dense=True)
    yield 'riscos','packed',riscos(packed=True)
    yield 'riscos','phases',riscos(7,phases=True)
    yield 'riscos','wide',riscos(8,wide=True)
    yield 'riscos','xy',xy()
    gray=bytearray(xy());gray[556:559]=bytes((0x27,0x15,0xb9));yield 'riscos','xy-gray',bytes(gray)
    yield 'xerox','alto',alto()
    yield 'xerox','bitblt',bitblt()
    yield 'xerox','bitblt-dense',bitblt(True)
    yield 'xerox','prepress',prepress()

def main():
    p=argparse.ArgumentParser();p.add_argument('output',type=pathlib.Path);a=p.parse_args()
    for fmt,label,data in cases():
        d=a.output/fmt;d.mkdir(parents=True,exist_ok=True);(d/label).write_bytes(data)
    raw=riscos();(a.output/'riscos'/'metrics-frame').write_bytes(b'METR'+struct.pack('>I',len(raw))+raw+intmetrics())
if __name__=='__main__':main()
