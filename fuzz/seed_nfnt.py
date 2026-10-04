#!/usr/bin/env python3
"""Original MIT NFNT wire fixtures; no third-party font data in fuzz seeds."""
import binascii,pathlib,random,struct,sys

def raster(seed=0,depth=1):
    rng=random.Random(seed);first=32;count=1+seed%96;height=1+seed%24
    widths=[rng.randrange(1,13) for _ in range(count+1)]
    loc=[0]
    for w in widths: loc.append(loc[-1]+w)
    row=((loc[-1]*depth+15)//16)*2
    pixels=bytearray(row*height)
    for y in range(height):
        for x in range(loc[-1]):
            sample=rng.randrange(1<<depth);bit=x*depth
            pixels[y*row+bit//8]|=sample<<(8-depth-bit%8)
    kind=0x9000|{1:0,2:4,4:8,8:12}[depth]
    ow=26+len(pixels)+len(loc)*2
    h=struct.pack('>4Hh8H',kind,first,first+count-1,max(widths)+1,-1,0xffff,max(widths)+1,height,(ow-16)//2,height-1,1,0,row//2)
    return h+pixels+struct.pack('>'+'H'*len(loc),*loc)+b''.join(bytes((1,w+1)) for w in widths)+b'\xff\xff'

def resource(payload):
    # A deliberately hand-assembled single-NFNT resource fork, without FOND.
    data=struct.pack('>I',len(payload))+payload
    ma=256+len(data);ml=50
    header=struct.pack('>4I',256,ma,len(data),ml)
    m=header+bytes(8)+struct.pack('>3H',28,50,0)+b'NFNT'+struct.pack('>2H',0,10)+struct.pack('>HHII',128,0xffff,0,0)
    return header+bytes(240)+data+m

def indexed_palette():
    nfnt=struct.pack('>4Hh8H',0x9284,65,65,4,0,0,4,1,9,1,0,0,1)+bytes([0x1b,0])+struct.pack('>6H',0,4,4,4,0,65535)
    rgb=[(65535,65535,65535),(65535,0,0),(0,65535,0),(0,0,65535)]
    palette=struct.pack('>IHH',1024,0,3)+b''.join(struct.pack('>4H',i,*color) for i,color in enumerate(rgb))
    data=struct.pack('>I',len(nfnt))+nfnt+struct.pack('>I',len(palette))+palette
    header=struct.pack('>4I',256,256+len(data),len(data),70)
    refs=b''.join(struct.pack('>HHII',128,65535,offset,0) for offset in [0,4+len(nfnt)])
    table=header+bytes(8)+struct.pack('>3H',28,70,1)+b'NFNT'+struct.pack('>2H',0,18)+b'fctb'+struct.pack('>2H',0,30)+refs
    return header+bytes(240)+data+table,palette

def classic_font():
    payload=raster(9)
    data=bytes(4)+struct.pack('>I',len(payload))+payload
    header=struct.pack('>4I',256,256+len(data),len(data),70)
    table=header+bytes(8)+struct.pack('>3H',28,62,0)+b'FONT'+struct.pack('>2H',1,10)
    table+=struct.pack('>HHII',200*128,0,0,0)+struct.pack('>HHII',200*128+10,65535,4,0)+b'\x07Griffin'
    return header+bytes(240)+data+table

def multi_resource():
    payloads=[raster(1),raster(31)]
    data=b'';offsets=[]
    for payload in payloads:
        offsets.append(len(data));data+=struct.pack('>I',len(payload))+payload
    header=struct.pack('>4I',256,256+len(data),len(data),62)
    table=header+bytes(8)+struct.pack('>3H',28,62,0)+b'NFNT'+struct.pack('>2H',1,10)
    table+=b''.join(struct.pack('>HHII',128+i,65535,at,0) for i,at in enumerate(offsets))
    return header+bytes(240)+data+table

def macbinary(fork,version=129):
    h=bytearray(128);h[1]=4;h[2:6]=b'font';h[65:69]=b'FFIL';h[69:73]=b'DMOV'
    h[87:91]=struct.pack('>I',len(fork));h[122]=version;h[123]=129 if version else 0
    if version==130:h[102:106]=b'mBIN'
    if version:h[124:126]=struct.pack('>H',binascii.crc_hqx(h[:124],0))
    return h+fork+bytes((-len(fork))%128)

def seeds():
    # A formerly plausible MacBinary header with no actual font resource map.
    unrelated=bytearray(158);unrelated[1]=1;unrelated[87:91]=struct.pack('>I',30)
    yield 'not-a-macbinary-font',unrelated
    yield 'indexed-color.dfont',indexed_palette()[0]
    yield 'classic-font.suit',classic_font()
    yield 'multi-strike.dfont',multi_resource()
    for i in [0,1,7,31,95,191]: yield f'original-{i}.nfnt',raster(i)
    for depth in [2,4,8]:yield f'gray-{depth}.nfnt',raster(13,depth)
    fork=resource(raster(31));yield 'original.dfont',fork
    for version in [0,129,130]:yield f'macbinary-{version}',macbinary(fork,version)
    for magic in [0x51600,0x51607]:yield f'apple-{magic:x}',struct.pack('>II16sHIII',magic,0x20000,bytes(16),1,2,38,len(fork))+fork
    for magic in [0x51600,0x51607]:yield f'apple-v1-{magic:x}',struct.pack('>II16sHIII',magic,0x10000,b'Macintosh       ',1,2,38,len(fork))+fork
    yield 'apple-data',struct.pack('>II16sHIII',0x51600,0x20000,bytes(16),1,1,38,len(fork))+fork

if __name__=='__main__':
    out=pathlib.Path(sys.argv[1] if len(sys.argv)>1 else 'fuzz/corpus/nfnt');out.mkdir(parents=True,exist_ok=True)
    for name,b in seeds():(out/name).write_bytes(b)
