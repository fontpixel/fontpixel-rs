#!/usr/bin/env python3
"""Original artwork and independently assembled DAVIC/SSFN wire seeds (MIT)."""
from pathlib import Path
import argparse, struct, json, hashlib
ROWS = ["X..X...XX", ".X.....X.", "..XX..X..", "X....X...", ".X.X....X"]
def be(v,n): return (v & ((1<<(8*n))-1)).to_bytes(n,'big')
def pfr(compression=0, top=False, black=True, code=65, fractional=False, freetype=False, strikes=1):
    w,h=9,5; x,y=-2, (3 if top and not freetype else -1)
    if fractional:
        gps=bytes([(compression<<6)|0x2a])+be(-384,2)+be(y*256+64,2)+bytes([w,h])+be(8*256+128,2)
    else: gps=bytes([(compression<<6)|0x19])+bytes([x&255,y&255,w,h,11])
    pixels=[int(c=='X') for row in (ROWS if top else ROWS[::-1]) for c in row]
    if compression==0:
        raster=bytearray((len(pixels)+7)//8)
        for i,v in enumerate(pixels): raster[i//8] |= (v if black or freetype else 1-v)<<(7-i%8)
    else:
        raster=bytearray(); pos=0; limit=15 if compression==1 else 255
        while pos<len(pixels):
            white=0
            while pos<len(pixels) and pixels[pos]==0 and white<limit: white+=1;pos+=1
            ink=0
            while pos<len(pixels) and pixels[pos]==1 and ink<limit: ink+=1;pos+=1
            raster.extend(bytes([(white<<4)|ink]) if compression==1 else bytes([white,ink]))
    gps+=raster
    logical=bytes.fromhex('00010000000000000000010000')
    base=be(0,2)+be(12,2)+be(12,2)+be(-2,2)+be(-1,2)+be(9,2)+be(6,2)
    name=b'Original E2\0'; tail=bytes(10); chars=bytes([code])+be(11,2)+bytes(3)
    # Extra bitmap info 5 + 1+1+1+2+2+1 = 13 bytes.
    size=len(base)+2+2+(5+8*strikes)+2+len(name)+len(tail)+2+len(chars)
    bct=bytes([code,len(gps)])+bytes(2)
    info=be(len(bct)*strikes,3)+bytes([0,strikes])+b''.join(bytes([12+i,12+i,0])+be(len(bct),2)+be((0 if freetype else size)+i*len(bct),2)+bytes([1]) for i in range(strikes))
    bct*=strikes
    phy=base+bytes([0x84,2,len(info),1])+info+bytes([len(name),2])+name+tail+be(1,2)+chars
    assert len(phy)==size
    logoff=65; phyoff=logoff+len(logical)+5; gpsoff=phyoff+len(phy)+len(bct)
    logical+=be(len(phy),2)+be(phyoff,3)
    head=bytearray(58);head[:10]=b'PFR0\0\4\r\n\0:'
    fields=[(10,2,7),(12,2,58),(14,2,len(logical)),(16,3,len(logical)),(19,3,logoff),(22,2,len(phy)),(24,3,len(phy)+len(bct)),(27,3,phyoff),(30,2,len(gps)),(32,3,len(gps)),(35,3,gpsoff),(42,1,int(black)+2*int(top)),(43,3,len(bct)),(46,3,len(bct)),(49,3,len(bct)),(52,2,1),(56,2,1)]
    for off,n,v in fields:head[off:off+n]=be(v,n)
    out=bytes(head)+be(1,2)+be(len(logical),2)+be(logoff,3)+logical+phy+bct+gps
    return out+be(len(out)+8,3)+b'$PFR$'
def ssfn_ascii(gray=False):
    rows=ROWS
    if gray: rows=[' '.join('--------' if c=='.' else f'{(x*37+y*19)%254+1:02X}FFFFFF' for x,c in enumerate(row)) for y,row in enumerate(rows)]
    return ('# Scalable Screen Font #\n$glyphdim 9 5\n$type 3\n$style regular\n$baseline 4\n$underline 4\n$name "Original E2"\n$family "Original"\n$subfamily "Regular"\n$revision "1"\n$manufacturer "Fontpixel"\n$license "MIT"\n===U+000041===w9=h5=x11=y0=o2="A"===\n'+'\n'.join(rows)+'\n# End #\n').encode()
def ssfn():
    strings=b'Original E2\0Original\0Regular\01\0Fontpixel\0MIT\0'
    # Explicit list avoids octal escape ambiguity.
    strings=b'\0'.join([b'Original E2',b'Original',b'Regular',b'1',b'Fontpixel',b'MIT'])+b'\0'
    fragoff=32+len(strings); raster=bytearray()
    for row in ROWS:
        for start in [0,8]:raster.append(sum((c=='X')<<(x%8) for x,c in enumerate(row) if start<=x<start+8))
    fragment=bytes([0x81,4])+raster; chars=bytearray([0xc0,64,2,1,9,5,11,0,0,0])+fragoff.to_bytes(3,'little')
    remaining=0x110000-66
    while remaining>=65536:chars.append(255);remaining-=65536
    while remaining>64:
        n=min(remaining,16128)-1;chars.extend([192+(n>>8),n&255]);remaining-=n+1
    if remaining:chars.append(128+remaining-1)
    head=bytearray(32);head[:4]=b'SFN2';head[8:14]=bytes([3,0,9,5,4,4]);head[14:16]=fragoff.to_bytes(2,'little');head[16:20]=(fragoff+len(fragment)).to_bytes(4,'little')
    out=head+strings+fragment+chars+b'2NFS';out[4:8]=len(out).to_bytes(4,'little');return bytes(out)

def ssfn_pixmap(repeat=False):
    b=ssfn();fo=int.from_bytes(b[14:16],'little');co=int.from_bytes(b[16:20],'little');chars=b[co:-4]
    pixels=[(254 if repeat else (x*37+y*19)%254) if c=='X' else 255 for y,row in enumerate(ROWS) for x,c in enumerate(row)]
    if repeat:
        packed=bytearray();i=0
        while i<len(pixels):
            j=i+1
            while j<len(pixels) and pixels[j]==pixels[i] and j-i<128:j+=1
            packed.extend([128+(j-i)-1,pixels[i]]);i=j
    else:packed=bytes([len(pixels)-1])+bytes(pixels)
    fragment=bytes([0xa0+((len(packed)-1)>>8),(len(packed)-1)&255,8,4])+packed
    out=bytearray(b[:fo]+fragment+chars);out[16:20]=(fo+len(fragment)).to_bytes(4,'little')
    out[28:32]=len(out).to_bytes(4,'little');out+=b''.join(bytes([255,255,255,a]) for a in range(1,255));out+=b'2NFS';out[4:8]=len(out).to_bytes(4,'little');return bytes(out)

def main():
    p=argparse.ArgumentParser();p.add_argument('--out',type=Path,required=True);a=p.parse_args();a.out.mkdir(parents=True,exist_ok=True);files={}
    for mode in range(3):
        for top in (False,True):
            for black in (False,True):files[f'pfr-{mode}-{int(top)}-{int(black)}.pfr']=pfr(mode,top,black)
    files['pfr-fractional.pfr']=pfr(fractional=True)
    files['pfr-ft-multistrike.pfr']=pfr(freetype=True,strikes=2)
    files['ssfn-ligature.asc']=ssfn_ascii().replace(b'U+000041',b'U+00F000').replace(b'\"A\"',b'\"fi\"')
    for mode in range(3):
        for top in (False,True):
            for black in (False,True):files[f'pfr-ft-{mode}-{int(top)}-{int(black)}.pfr']=pfr(mode,top,black,freetype=True)
    files.update({'ssfn-original.sfn':ssfn(),'ssfn-original.asc':ssfn_ascii(),'ssfn-gray.asc':ssfn_ascii(True)})
    files['ssfn-gray.sfn']=ssfn_pixmap()
    files['ssfn-repeat.sfn']=ssfn_pixmap(True)
    files['ssfn-collection.sfn']=b'SFNC'+(8+len(ssfn())*2).to_bytes(4,'little')+ssfn()*2
    import gzip
    files['ssfn-gzip.sfn.gz']=gzip.compress(ssfn(),mtime=0)
    for name,data in files.items():(a.out/name).write_bytes(data)
    (a.out/'manifest.json').write_text(json.dumps({n:{'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()} for n,b in files.items()},indent=2)+'\n')
if __name__=='__main__':main()
