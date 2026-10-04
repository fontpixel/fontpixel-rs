#!/usr/bin/env python3
"""PF2 independent wire/GRUB, FONTX2 monobit, and HZK raw/transpose checks.

Production code never calls these tools. All fixtures are generated or read from
licensed source files; no upstream font bytes are committed. Misaki downloads
are pinned to the author's release and verified before safe member extraction.
"""
import argparse
import hashlib
import json
import struct
import subprocess
import sys
import urllib.request
import zipfile
from pathlib import Path
import monobit

ROOT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT.parent / 'reference/fontpixel/pipeline'))
from opf.parsers.bdf import parse_bdf

MISAKI_URL = 'https://littlelimit.net/arc/misaki/misaki_fontx_2012-06-03.zip'
MISAKI_SHA = '30453aec50689f24fc4eb87a2cd3e89c62fcc7e8fc4f18916cfafeb908afc760'


def pf2_wire(path):
    """Independent, deliberately bit-oriented PF2 decoder from public manual."""
    b = path.read_bytes()
    assert b[:12] == b'FILE\0\0\0\x04PFF2'
    pos, index, sections = 12, [], {}
    while True:
        tag, size = struct.unpack_from('>4sI', b, pos)
        pos += 8
        if tag == b'DATA':
            assert size == 0xffffffff
            break
        body = b[pos:pos+size]
        assert len(body) == size
        sections[tag.decode('ascii')] = body.hex()
        if tag == b'CHIX':
            assert size % 9 == 0
            index = list(struct.iter_unpack('>IBI', body))
        pos += size
    result = []
    for cp, flag, off in index:
        assert flag == 0 and off >= pos
        w, h, x, y, adv = struct.unpack_from('>HHhhh', b, off)
        size = (w*h+7)//8
        packed = b[off+10:off+10+size]
        assert len(packed) == size
        rows = bytearray(((w+7)//8)*h)
        for iy in range(h):
            for ix in range(w):
                bit = iy*w+ix
                if packed[bit//8] & (128 >> (bit % 8)):
                    rows[iy*((w+7)//8)+ix//8] |= 128 >> (ix % 8)
        result.append((cp, w, h, x, y, adv, bytes(rows)))
    return sorted(result), sections


def native_sig(data):
    return sorted((cp, g['width'], g['height'], g['x'], g['y'],
                   g['advance'][0], bytes(g['rows']))
                  for g in data['glyphs'] for _, cp in g['codes'])


def mono_sig(font):
    return sorted((int.from_bytes(bytes(g.codepoint), 'big'), g.width, g.height,
                   g.left_bearing, g.shift_up, g.advance_width, g.as_bytes())
                  for g in font.glyphs)


def source_sig(path):
    return sorted((g.cp, g.bbw, g.bbh, g.bbx, g.bby, g.dwidth,
                   bytes(g.rows))
                  for g in parse_bdf(path, 'w42', remap_charset=False).glyphs if g.cp is not None and 0 <= g.cp <= 0x10ffff)


def fontx_fixture(path, double):
    w, h = (9, 3) if double else (8, 16)
    codes = [0x8140, 0x8141, 0x82a0] if double else list(range(256))
    b = bytearray(b'FONTX2W42-wire' + bytes([w, h, double]))
    if double:
        b += bytes([2]) + struct.pack('<HHHH', 0x8140, 0x8141, 0x82a0, 0x82a0)
    for i in range(len(codes)):
        for y in range(h):
            for x in range((w+7)//8):
                v = (i*37+y*19+x*71) & 255
                if x == (w+7)//8-1 and w % 8:
                    v &= 255 << (8-w % 8)
                b.append(v)
    path.write_bytes(b)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=Path, required=True, help='w42_timing example executable')
    p.add_argument('--output-dir', type=Path, required=True)
    p.add_argument('--real-font-dir', type=Path)
    p.add_argument('--fetch-misaki', action='store_true')
    args = p.parse_args()
    out = args.output_dir.resolve()
    out.mkdir(parents=True, exist_ok=False)
    binary = str(args.binary.resolve())
    checks, provenance = [], []

    def run(*a):
        result = subprocess.run([binary, *map(str, a)], capture_output=True)
        assert result.returncode == 0, (a, result.stderr.decode()[:6000])
        return result

    def inspect(fmt, file, preset=None):
        return json.loads(run('inspect', fmt, file, *([preset] if preset else [])).stdout)

    def fontx_check(path, name):
        native = native_sig(inspect('fontx2', path))
        oracle = monobit.load(path, format='fontx')[0]
        assert native == mono_sig(oracle), name
        rust = out / f'{name}-rust.fnt'
        run('convert', 'fontx2', 'fontx2', path, rust)
        assert native == mono_sig(monobit.load(rust, format='fontx')[0]), name
        other = out / f'{name}-monobit.fnt'
        monobit.save(oracle, other, format='fontx')
        # monobit fills gaps within each high-byte page; compare its own output.
        assert native_sig(inspect('fontx2', other)) == mono_sig(monobit.load(other, format='fontx')[0]), name
        bdf = out / f'{name}.bdf'
        run('convert', 'fontx2', 'bdf', path, bdf)
        assert source_sig(bdf) == native, name
        checks.append({'case':name,'format':'fontx2','glyphs':len(native),'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'checks':['monobit reads native','native reads monobit','independent BDF catalogue parser']})

    for double in (False, True):
        path = out / f'wire-{int(double)}.fnt'
        fontx_fixture(path, double)
        fontx_check(path, f'wire-{int(double)}')

    source_dir = args.real_font_dir
    if args.fetch_misaki:
        source_dir = out / 'misaki'
        source_dir.mkdir()
        payload = urllib.request.urlopen(MISAKI_URL, timeout=30).read(1000000)
        assert hashlib.sha256(payload).hexdigest() == MISAKI_SHA
        archive = source_dir / 'source.zip'
        archive.write_bytes(payload)
        with zipfile.ZipFile(archive) as z:
            for name in ['4X8.FNT','MISAKI.FNT','MISAKIMN.FNT','misaki.txt','readme.txt']:
                (source_dir / name).write_bytes(z.read(name))
        provenance.append({'url':MISAKI_URL,'sha256':MISAKI_SHA,'license':'M+ FONTS unrestricted use/copy/modify/distribute','license_url':'https://littlelimit.net/font.htm#license'})
    if source_dir:
        for path in sorted(source_dir.glob('*.FNT')):
            fontx_check(path, path.stem.lower())

    presets = [('hzk12',12,12,False,0xa1a1),('hzk14',14,14,False,0xa1a1),('hzk16',16,16,False,0xa1a1),('hzk24s',24,24,True,0xb0a1),('hzk24t',24,24,True,0xa1a1),('hzk32',32,32,False,0xa1a1),('hzk40',40,40,False,0xa1a1),('hzk48',48,48,False,0xa1a1),('asc12',8,12,False,0),('asc16',8,16,False,0),('asc24',12,24,False,0),('asc32',16,32,False,0),('asc40',20,40,False,0),('asc48',24,48,False,0)]
    for preset,w,h,transposed,first in presets:
        ww,hh = (h,w) if transposed else (w,h)
        stride = (ww+7)//8
        raw = bytearray((i*37+113)&255 for i in range(stride*hh*100))
        if ww % 8:
            for i in range(stride-1,len(raw),stride):
                raw[i] &= 255 << (8-ww % 8)
        path = out / preset
        path.write_bytes(raw)
        source = native_sig(inspect('hzk', path, preset))
        mono = monobit.load(path, format='raw', cell=(ww,hh),count=100)[0]
        raster = [g.as_bytes() for g in mono.glyphs]
        for i,g in enumerate(source):
            expected_code = first+i if first <256 else (((first>>8)+(first%256-0xa1+i)//94)<<8) + 0xa1 + (first%256-0xa1+i)%94
            assert g[0] == expected_code
            bm = raster[i]
            if transposed:
                result=bytearray(((w+7)//8)*h)
                for y in range(h):
                    for x in range(w):
                        if bm[x*((h+7)//8)+y//8] & (128>>(y%8)):
                            result[y*((w+7)//8)+x//8] |=128>>(x%8)
                bm=bytes(result)
            assert g[1:] == (w,h,0,0,w,bm)
        dest=out/f'{preset}-native'
        run('convert','hzk','hzk',path,dest,preset)
        assert dest.read_bytes()==raw
        if not transposed:
            oracle_out=out/f'{preset}-monobit.bin'
            monobit.save(mono,oracle_out,format='raw')
            assert native_sig(inspect('hzk',oracle_out,preset))==source
        bdf=out/f'{preset}.bdf'
        run('convert','hzk','bdf',path,bdf,preset)
        assert source_sig(bdf)==source
        checks.append({'case':preset,'format':'hzk','glyphs':100,'checks':['independent grid','monobit raw raster','native bytes roundtrip','catalogue BDF parser']})

    for path in sorted(Path('/usr/share/grub').glob('*.pf2')):
        native=native_sig(inspect('pf2',path))
        wire,sections=pf2_wire(path)
        assert native==wire,path
        dest=out/path.name
        run('convert','pf2','pf2',path,dest)
        assert pf2_wire(dest)[0]==wire
        checks.append({'case':str(path),'format':'pf2','glyphs':len(wire),'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'checks':['independent PF2 wire','native roundtrip']})

    catalogue=ROOT.parent/'samples/fontpixel-catalogue'
    for relative in ['tom-thumb/tom-thumb.bdf','terminus/ter-u12n.bdf']:
        path=catalogue/relative
        native=source_sig(path)
        dest=out/f'{path.stem}.pf2'
        run('convert','bdf','pf2',path,dest)
        assert pf2_wire(dest)[0]==native,path
        again=out/f'{path.stem}-again.bdf'
        run('convert','pf2','bdf',dest,again)
        assert source_sig(again)==native
        oracle=out/f'{path.stem}-grub.pf2'
        result=subprocess.run(['grub-mkfont','-o',str(oracle),str(path)],capture_output=True)
        assert result.returncode==0,result.stderr.decode()
        assert native_sig(inspect('pf2',oracle))==pf2_wire(oracle)[0]
        # grub-mkfont trims glyphs; compare occupied font-space pixels and advances.
        def ink(sig):
            return {cp:(adv,{(x+ix,y+h-1-iy)for iy in range(h)for ix in range(w)if rows[iy*((w+7)//8)+ix//8]&(128>>(ix%8))})for cp,w,h,x,y,adv,rows in sig}
        assert ink(pf2_wire(oracle)[0])==ink(native)
        checks.append({'case':relative,'format':'pf2','glyphs':len(native),'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'checks':['catalogue BDF parser','native PF2 wire','grub-mkfont ink equivalence']})

    # Full supplied OFL CJK font: actual Unicode conversion, omitted non-CJK codes,
    # fixed-cell clipping, and raw library slot placement against CPython tables.
    tong=ROOT.parent/'samples/tong-pixel-sans/TongPixelSans14Large-SC.bdf'
    original=parse_bdf(tong,'w42',remap_charset=False)
    for fmt,encoding,width,height,preset in [('hzk','gb2312',16,16,'hzk16'),('fontx2','shift_jis',original.bbox[0],original.bbox[1],None)]:
        path=out/f'tong-{fmt}.bin'
        run('convert','bdf',fmt,tong,path,*([preset]if preset else []))
        found=native_sig(inspect(fmt,path,preset))
        expected={}
        for g in original.glyphs:
            try:
                c=chr(g.cp);encoded=c.encode(encoding)
                if encoded.decode(encoding)!=c:continue
            except (UnicodeError,ValueError):continue
            cp=int.from_bytes(encoded,'big')
            if fmt=='hzk' and (len(encoded)!=2 or not all(0xa1<=v<=0xfe for v in encoded)):continue
            bitmap=bytearray(((width+7)//8)*height)
            for y in range(g.bbh):
                yy=g.bby+g.bbh-1-y
                if not 0<=yy<height:continue
                for x in range(g.bbw):
                    xx=g.bbx+x
                    if 0<=xx<width and g.rows[y*((g.bbw+7)//8)+x//8]&(128>>(x%8)):
                        bitmap[(height-1-yy)*((width+7)//8)+xx//8]|=128>>(xx%8)
            expected[cp]=bytes(bitmap)
        assert expected
        assert set(expected)<=set(g[0]for g in found)
        for cp,w,h,x,y,advance,bitmap in found:
            assert (w,h,x,y,advance)==(width,height,0,0,width)
            assert bitmap==expected.get(cp,bytes(((width+7)//8)*height)),(fmt,cp)
        if fmt=='fontx2':assert mono_sig(monobit.load(path,format='fontx')[0])==found
        checks.append({'case':'TongPixelSans14Large-SC.bdf','format':fmt,'glyphs':len(found),'mapped_source_glyphs':len(expected),'sha256':hashlib.sha256(tong.read_bytes()).hexdigest(),'checks':['full OFL source','CPython strict charset conversion','independent font-space clipping','all emitted slots']})

    result={'checks':checks,'provenance':provenance,'monobit':monobit.__version__,'limitations':['monobit has no PF2/HZK plugin; PF2 uses public-spec decoder and grub-mkfont, HZK uses raw decoder and independent transpose','No proprietary UCDOS binaries redistributed or used as licensed fixtures','Formal fuzz and five-round performance are coordinated after source freeze']}
    (out/'results.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({'passed':len(checks),'glyphs':sum(c['glyphs']for c in checks),'report':str(out/'results.json')}))

if __name__=='__main__':
    main()
