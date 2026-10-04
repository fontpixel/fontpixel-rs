#!/usr/bin/env python3
"""Reproducible BMFont monobit and SFD/Kbits Bits'N'Picas crosschecks.

Only original fixtures and existing licensed local sources are used. The optional
Bits'N'Picas binary is downloaded from its pinned public release and SHA-256
verified. No upstream implementation code is incorporated into production.
"""
import argparse
import base64
import hashlib
import json
import os
import struct
import subprocess
import sys
import urllib.request
from pathlib import Path
import monobit

ROOT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT.parent / 'reference/fontpixel/pipeline'))
from opf.ingest.kbitx import convert_kbitx
from opf.parsers.bdf import parse_bdf
JAR_URL = 'https://github.com/kreativekorp/bitsnpicas/releases/download/v2.2.2/BitsNPicas.jar'
JAR_SHA = '22685a52bc1a7f26680f80197548e74dc61a9f58eb083d6f700220a48a1165a6'


def digest(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


def kbits_sig(path):
    """Independent field-oriented decoder of the published 1.x wire layout."""
    data = path.read_bytes()
    assert data[:8] == b'KBnPbits'
    header = struct.unpack_from('>7i', data, 8)
    assert header[0] == 1
    pos, glyphs = 36, []
    while True:
        tag = data[pos:pos+4]
        pos += 4
        if tag == b'fin.':
            assert pos == len(data)
            return sorted(glyphs), header[1:]
        if tag == b'name':
            version, ident, size = struct.unpack_from('>iiH', data, pos)
            assert version == 1
            pos += 10 + size
        elif tag == b'char':
            version, cp, advance, x, top, height = struct.unpack_from('>6i', data, pos)
            assert version == 1
            pos += 24
            rows = []
            for _ in range(height):
                width, = struct.unpack_from('>i', data, pos)
                pos += 4
                rows.append(data[pos:pos+width])
                pos += width
            width = max(map(len, rows), default=0)
            pixels = b''.join(row.ljust(width, b'\0') for row in rows)
            glyphs.append((cp, width, height, x, top-height, advance, pixels))
        else:
            raise AssertionError(tag)


def bdf_sig(path):
    f = parse_bdf(path, 'w44', remap_charset=False)
    out = []
    for g in f.glyphs:
        if g.cp is None:
            continue
        packed = bytes(g.rows)
        stride = (g.bbw+7)//8
        pixels = bytes(255 if packed[y*stride+x//8] & (128 >> (x % 8)) else 0
                       for y in range(g.bbh) for x in range(g.bbw))
        out.append((g.cp, g.bbw, g.bbh, g.bbx, g.bby, g.dwidth, pixels))
    return sorted(out)


def fixture(path):
    text = 'STARTFONT 2.1\nFONT W44-Original\nSIZE 12 72 72\nFONTBOUNDINGBOX 9 5 -2 -1\nSTARTPROPERTIES 6\nCHARSET_REGISTRY "ISO10646"\nCHARSET_ENCODING "1"\nFONT_ASCENT 9\nFONT_DESCENT 3\nFAMILY_NAME "W44 Original"\nPIXEL_SIZE 12\nENDPROPERTIES\nCHARS 3\n'
    for cp, rows in [(65, ['FF80','8080','AA80','8080','FF80']), (66,['8080','4080','2080','1080','FF80']), (0x20000,['FF80','AA80','5580','AA80','FF80'])]:
        text += f'STARTCHAR uni{cp:X}\nENCODING {cp}\nSWIDTH 833 0\nDWIDTH 10 0\nBBX 9 5 -2 -1\nBITMAP\n'+'\n'.join(rows)+'\nENDCHAR\n'
    path.write_text(text+'ENDFONT\n')


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--binary', type=Path, required=True)
    ap.add_argument('--output-dir', type=Path, required=True)
    ap.add_argument('--jar', type=Path)
    ap.add_argument('--source', type=Path, action='append', default=[])
    ap.add_argument('--fontforge-root', type=Path)
    a = ap.parse_args()
    out = a.output_dir.resolve()
    out.mkdir(parents=True, exist_ok=False)
    jar = a.jar.resolve() if a.jar else out / 'BitsNPicas.jar'
    if not jar.exists():
        jar.write_bytes(urllib.request.urlopen(JAR_URL).read())
    assert digest(jar) == JAR_SHA
    binary = a.binary.resolve()
    checks = []

    def run(*argv):
        p = subprocess.run(list(map(str, argv)), capture_output=True)
        assert p.returncode == 0, (argv, p.stderr.decode(errors='replace'))
        return p

    def convert(src, dest, target, *options):
        run(binary, 'convert', src, '-o', dest, '--to', target, *options)
        assert dest.exists()

    def native_sig(src, label):
        dest = out / (label+'.kbits')
        convert(src, dest, 'kbits')
        return kbits_sig(dest)[0]

    source = out / 'original.bdf'
    fixture(source)
    expected = bdf_sig(source)
    for encoding in ['text','xml','binary']:
        desc = out / f'rust-{encoding}.fnt'
        convert(source, desc, 'bmfont', '--bmfont-encoding', encoding,
                '--bmfont-page-width', '12', '--bmfont-page-height', '6')
        assert native_sig(desc, f'rust-{encoding}-read') == expected
        mono = monobit.load(desc, format='bmfont')[0]
        saved = out / f'monobit-{encoding}.bdf'
        monobit.save(mono, saved, format='bdf')
        observed = bdf_sig(saved)
        # monobit 0.48 ignores common.base on read and uses maximum bitmap height.
        # Keep the discrepancy explicit while checking every pixel and metric.
        expected_monobit = [(cp,w,h,x,y-4,advance,pix) for cp,w,h,x,y,advance,pix in expected]
        assert observed == expected_monobit, (observed, expected_monobit)
        reverse = out / f'monobit-{encoding}.fnt'
        monobit.save(monobit.load(source, format='bdf'), reverse,
                     format='bmfont', descriptor=encoding, image_size=(32, 16))
        assert native_sig(reverse, f'monobit-{encoding}-read') == expected
        checks.append({'oracle':'monobit','format':'bmfont','encoding':encoding,'directions':2,'glyphs':len(expected),'known_difference':'monobit BMFont reader ignores base=9 and uses raster.top=5; exact y difference -4 independently asserted'})
    for idx, source in enumerate([source, *a.source]):
        expected = bdf_sig(source)
        for fmt in ['sfd','kbits']:
            native = out / f'source-{idx}.{fmt}'
            convert(source, native, fmt)
            # Independent application reads Rust's native file.
            decoded = out / f'bits-source-{idx}-{fmt}.kbits'
            run('java','-Djava.awt.headless=true','-jar',jar,'convertbitmap','-f','kbits','-o',decoded,native)
            assert decoded.exists(), 'BitsNPicas returned success without an output'
            assert kbits_sig(decoded)[0] == expected
            assert native_sig(decoded, f'bits-source-{idx}-{fmt}-read') == expected
            # Frozen MIT catalogue consumes our KBITX bridge.
            xml = out / f'catalogue-source-{idx}-{fmt}.kbitx'
            convert(native, xml, 'kbitx')
            catalogue = convert_kbitx(xml, 'w44')
            assert [(g.cp,g.bbw,g.bbh,g.bbx,g.bby,g.dwidth,bytes(g.rows)) for g in sorted(catalogue.glyphs,key=lambda g:g.cp)] == [(cp,w,h,x,y,adv,bytes(sum((128>>(xx%8)) for xx in range(k*8,min(k*8+8,w)) if pix[yy*w+xx]>=128) for yy in range(h) for k in range((w+7)//8))) for cp,w,h,x,y,adv,pix in expected]
            checks.append({'oracle':'BitsNPicas 2.2.2 + frozen catalogue','format':fmt,'source':str(source),'sha256':digest(source),'glyphs':len(expected)})
    # Independently encoded grayscale SFD strikes exercise each documented depth.
    for depth in [2,4,8]:
        maximum=(1<<depth)-1
        samples=[0,maximum,maximum//2,1]
        packed=bytearray((len(samples)*depth+7)//8)
        for i,v in enumerate(samples):
            packed[i*depth//8] |= v << (8-depth-i*depth%8)
        encoded=base64.a85encode(bytes(packed).ljust((len(packed)+3)//4*4,b'\0')).decode()
        sfd=out/f'depth-{depth}.sfd'
        sfd.write_text('SplineFontDB: 3.0\nFontName: Original\nAscent: 9\nDescent: 3\nBeginChars: 1 1\nStartChar: A\nEncoding: 65 65 0\nWidth: 5\nEndChar\nEndChars\n'+f'BitmapFont: 12 1 9 3 {depth}\nBDFChar: 0 65 5 -1 2 -2 -2\n{encoded}\nEndBitmapFont\nEndSplineFont\n')
        actual=native_sig(sfd,f'depth-{depth}')
        assert actual == [(65,4,1,-1,-2,5,bytes(v*255//maximum for v in samples))]
        checks.append({'oracle':'independent Python ASCII85/bit packer','format':'sfd','depth':depth})
    if a.fontforge_root:
        ffroot = a.fontforge_root.resolve()
        env = dict(os.environ, LD_LIBRARY_PATH=str(ffroot/'usr/lib/x86_64-linux-gnu'))
        code = 'import fontforge,sys; f=fontforge.open(sys.argv[1]); f.save(sys.argv[2]); f.generate(sys.argv[3])'
        ff_saved=out/'fontforge-resaved.sfd'
        ff_bdf=out/'fontforge-export.bdf'
        proc=subprocess.run([str(ffroot/'usr/bin/fontforge'),'-lang=py','-c',code,str(out/'source-0.sfd'),str(ff_saved),str(ff_bdf)],env=env,capture_output=True)
        assert proc.returncode==0,proc.stderr.decode()
        original_expected=bdf_sig(out/'original.bdf')
        assert native_sig(ff_saved,'fontforge-resaved-read')==original_expected
        assert bdf_sig(out/'fontforge-export-12.bdf')==original_expected
        # FontForge's BDF import is UCS-2. Use an independent BMP original for
        # this direction; non-BMP is covered above through its SFD reader/writer.
        bmp=out/'fontforge-original-bmp.bdf'
        bmp.write_text((out/'original.bdf').read_text().replace('uni20000','uni43').replace('ENCODING 131072','ENCODING 67'))
        reverse=out/'fontforge-from-bdf.sfd'
        code='import fontforge,sys; f=fontforge.open(sys.argv[1]); f.save(sys.argv[2])'
        proc=subprocess.run([str(ffroot/'usr/bin/fontforge'),'-lang=py','-c',code,str(bmp),str(reverse)],env=env,capture_output=True)
        assert proc.returncode==0,proc.stderr.decode()
        assert native_sig(reverse,'fontforge-from-bdf-read')==bdf_sig(bmp)
        checks.append({'oracle':'FontForge 20230101 Ubuntu official binary','format':'sfd','directions':2,'forward_glyphs':3,'reverse_glyphs':3,'fontforge_sha256':digest(ffroot/'usr/bin/fontforge'),'known_difference':'BDF importer uses UCS-2; reverse fixture is BMP, forward SFD includes U+20000'})
    evidence={'checks':checks,'jar_url':JAR_URL,'jar_sha256':digest(jar),'binary_sha256':digest(binary),'python':sys.version,'monobit':getattr(monobit,'__version__','unknown'),'limitations':['monobit has no SFD/Kbits handler','BitsNPicas has no SFD writer; reverse SFD wire fixtures are original Python ASCII85',*(['FontForge not requested'] if not a.fontforge_root else [])],'artifacts':{str(p.relative_to(out)):{'bytes':p.stat().st_size,'sha256':digest(p)}for p in out.rglob('*')if p.is_file()}}
    (out/'results.json').write_text(json.dumps(evidence,indent=2)+'\n')
    print(json.dumps(checks,indent=2))


if __name__=='__main__':
    main()
