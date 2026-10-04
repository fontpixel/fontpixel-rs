#!/usr/bin/env python3
"""Original native image-font seeds; standard library only, no vendor fonts."""
import argparse,hashlib,json,struct,zlib
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
def chunk(k,v):return struct.pack('>I',len(v))+k+v+struct.pack('>I',zlib.crc32(k+v))
def png(w,h,p,rgba=False):
    step=w*(4 if rgba else 1)
    raw=b''.join(b'\0'+bytes(p[y*step:(y+1)*step])for y in range(h))
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,6 if rgba else 0,0,0,0))+chunk(b'IDAT',zlib.compress(raw))+chunk(b'IEND',b'')
def bundle(files):
    out=bytearray(b'FPSEED2\0'+bytes([len(files)]))
    for name,data in files.items():n=name.encode();out.extend(bytes([len(n)])+n+struct.pack('<I',len(data))+data)
    return out
def rfont_v2(raw):
    out=bytearray(raw[:8]);pos=8
    while pos<len(raw):
        n=struct.unpack_from('>I',raw,pos)[0];kind=raw[pos+4:pos+8];p=raw[pos+8:pos+8+n];pos+=12+n
        if kind==b'rfVM':p=b'\0\2'+b''.join(struct.pack('>H',v)for v in p[2:])
        elif kind==b'rfHM':
            ranges=struct.unpack_from('>I',p,2)[0];new=bytearray(b'\0\2'+p[2:6]);at=6
            for _ in range(ranges):
                start,count=struct.unpack_from('>II',p,at);new.extend(p[at:at+8]);at+=8
                for _ in range(count):
                    values=struct.unpack_from('>4B2H',p,at);at+=8;new.extend(struct.pack('>4H2I',*values))
            p=new
        out.extend(chunk(kind,p))
    return out

def main():
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--output-dir',type=Path,required=True);a=ap.parse_args();seeds={}
    b=bytearray(b'PILfont\n;;;;;;7;\nOriginal MIT info\nDATA\n');start=len(b);b.extend(bytes(5120));struct.pack_into('>10h',b,start+32*20,4,0,0,0,0,0,0,0,0,0);struct.pack_into('>10h',b,start+65*20,5,1,-1,-4,2,-2,0,0,3,2)
    mask=png(3,2,[0,17,255,64,128,240]);seeds['pilfont_bitmap']={'gray-and-blank':bundle({'font.pil':b,'font.pbm':mask}),'bare-metrics':b}
    images=bundle({'0041.png':mask,'0100.png':png(2,2,[255,0,0,255])});seeds['imageset_bitmap']={'two-labels':images,'duplicate':bundle({'41.png':mask,'0041.png':mask})}
    seeds['vlw_bitmap']={}
    for version in [8,9,10,11]:
        out=bytearray(struct.pack('>6i',3,version,9,123,7,2));pixels=[]
        for cp,h,w,adv,top,left,data in [(65,2,3,5,4,-2,[0,17,255,64,128,240]),(256,3,2,4,6,1,[255,0,1,254,128,0]),(0x20001,0,0,7,0,0,[])]:out.extend(struct.pack('>7i',cp,h,w,adv,top,left,456));pixels.extend(data)
        out.extend(pixels)
        if version>=10:
            for name in [b'Original\xc0\x80\xed\xa0\xbd\xed\xb9\x82',b'OriginalPS']:out.extend(struct.pack('>H',len(name))+name)
        if version==11:out.append(1)
        seeds['vlw_bitmap']['version'+str(version)]=out
    p=bytearray(10*4*4)
    for x in [0,1,4,5,8,9]:p[x*4:x*4+4]=bytes([255,0,255,255])
    for x,y,v in [(2,1,255),(3,2,17),(6,1,128),(7,3,255)]:i=(y*10+x)*4;p[i:i+4]=bytes([255,255,255,v])
    r=(ROOT/'crates/fontpixel/tests/fixtures/w5e2/rfont-original.png').read_bytes();seeds['sfont_bitmap']={'sfont-two':png(10,4,p,True),'rfont-v1':r,'rfont-v2':rfont_v2(r)}
    manifest={}
    for target,files in seeds.items():
        folder=a.output_dir/target;folder.mkdir(parents=True,exist_ok=True);first=next(iter(files.values()));files['truncated']=first[:len(first)//2];files['empty']=b''
        for name,data in files.items():path=folder/name;path.write_bytes(data);manifest[str(path)]={'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()}
    (a.output_dir/'w5e2-seeds.json').write_text(json.dumps(manifest,indent=2)+'\n');print(json.dumps({'targets':len(seeds),'seeds':len(manifest)}))
if __name__=='__main__':main()
