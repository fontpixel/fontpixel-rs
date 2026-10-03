#!/usr/bin/env python3
"""W2.1 independent common-table validation; no new outline codec is claimed.

Build: cargo build -p fontpixel --features compat-bdfparser --example sfnt_check
Run: python tests/crosscheck/sfnt.py --output tests/crosscheck/sfnt-results.json
Dependencies: fonttools, freetype-py, uharfbuzz, opentype-sanitizer (PyPI).
All generated fonts and external-tool rewrites stay in a temporary directory.
"""
import argparse, hashlib, importlib.metadata, json, subprocess, tempfile
from pathlib import Path
import freetype
import uharfbuzz as hb
import ots
from fontTools.ttLib import TTFont
from fontTools.ttLib.tables.O_S_2f_2 import intersectUnicodeRanges
from fontTools.ttLib.tables._c_m_a_p import CmapSubtable
ROOT=Path(__file__).resolve().parents[2]
SOURCE=ROOT/'tests/data/compat/mini-square-1.0-0.ttf'

def bitmap(face,gid):
    face.set_pixel_sizes(0,8)
    face.load_glyph(gid,freetype.FT_LOAD_RENDER|freetype.FT_LOAD_TARGET_MONO|freetype.FT_LOAD_NO_HINTING)
    slot=face.glyph;b=slot.bitmap;raw=bytes(b.buffer)
    pixels=tuple(tuple((raw[y*abs(b.pitch)+x//8]>>(7-x%8))&1 for x in range(b.width)) for y in range(b.rows))
    return (b.width,b.rows,slot.bitmap_left,slot.bitmap_top,slot.advance.x,pixels)

def shape(path,text,direction='ltr'):
    face=hb.Face(path.read_bytes());font=hb.Font(face);font.scale=(face.upem,face.upem)
    b=hb.Buffer();b.add_str(text);b.guess_segment_properties();b.direction=direction;hb.shape(font,b)
    return [(i.codepoint,p.x_advance,p.y_advance,p.x_offset,p.y_offset) for i,p in zip(b.glyph_infos,b.glyph_positions)]

def inspect(binary,path):
    return json.loads(subprocess.check_output([str(binary),'--inspect',str(path)],text=True))

def check_projection(font,actual):
    mapping=font.getBestCmap();order=font.getGlyphOrder()
    expected={str(cp):order.index(name) for cp,name in mapping.items()}
    assert actual['cmap']==expected, 'Rust nominal cmap differs from fontTools'
    assert actual['glyphs']==len(order)
    assert actual['hmtx']==[list(font['hmtx'].metrics[n]) for n in order]
    assert actual['vmtx']==[list(font['vmtx'].metrics[n]) for n in order]
    assert actual['post_names']==order
    assert actual['gasp_ranges']==[list(r) for r in sorted(font['gasp'].gaspRange.items())]
    fields=[getattr(font['OS/2'],'ulUnicodeRange'+str(i)) for i in range(1,5)]
    assert actual['unicode_ranges']==fields
    for platform,encoding,language,name_id,text in actual['names']:
        record=font['name'].getName(name_id,platform,encoding,language)
        if text is not None:assert record.toUnicode()==text

def main():
    p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,default=ROOT/'target/debug/examples/sfnt_check');p.add_argument('--output',type=Path);a=p.parse_args()
    results=[]
    with tempfile.TemporaryDirectory(prefix='fontpixel-sfnt-') as tmp:
        tmp=Path(tmp);subprocess.run([str(a.binary),str(tmp)],check=True)
        source=TTFont(SOURCE);source.ensureDecompiled();source_ft=freetype.Face(str(SOURCE))
        expected_pixels={gid:bitmap(source_ft,gid) for gid in range(len(source.getGlyphOrder()))}
        # Literal BDF ink rows, independently of the pre-existing TTF golden.
        assert expected_pixels[2][-1]==tuple(tuple(int(c) for c in row) for row in ['00100','01010','10001','11111','10001','10001','10001'])
        assert expected_pixels[3][-1]==tuple(tuple(int(c) for c in row) for row in ['1110','1001','1001','1110','1001','1001','1110'])
        for label in ['common','fresh-common','dense','sparse']:
            path=tmp/f'{label}.ttf';f=TTFont(path,checkChecksums=2);f.ensureDecompiled()
            f.saveXML(tmp/f'{label}.ttx')
            actual=inspect(a.binary,path);check_projection(f,actual)
            mapping=f.getBestCmap();bits=intersectUnicodeRanges(mapping)
            expected=[sum(1<<(bit%32) for bit in bits if bit//32==word) for word in range(4)]
            assert actual['unicode_ranges']==expected,'Unicode-range bits differ from fontTools'
            if label in ['common','fresh-common']:
                assert actual['cmap']=={'32':1,'65':2,'66':3,'65535':2,'65536':2,'128512':3}
            elif label=='dense':assert actual['cmap']=={str(cp):1+cp%3 for cp in range(1000)}
            else:assert actual['cmap']=={str(cp):2 for cp in range(0,0xd800,2)}
            sanitized=tmp/f'{label}-ots.ttf';result=ots.sanitize(str(path),str(sanitized),capture_output=True)
            assert result.returncode==0,result.stderr.decode()+result.stdout.decode()
            checked=TTFont(sanitized);checked.ensureDecompiled()
            assert checked.getBestCmap()==mapping
            ft=freetype.Face(str(path));san_ft=freetype.Face(str(sanitized))
            for gid,expected in expected_pixels.items():
                assert bitmap(ft,gid)==expected,('FreeType source pixels/metrics',label,gid)
                assert bitmap(san_ft,gid)==expected,('OTS preserved pixels/metrics',label,gid)
            if label in ['common','fresh-common']:
                assert shape(path,'AB')==shape(SOURCE,'AB')
                assert [(x[0],x[1]) for x in shape(path,'\U00010000\U0001f600')]==[(2,384),(3,384)]
                vertical=shape(path,'AB','ttb');assert [(x[0],x[2]) for x in vertical]==[(2,-512),(3,-512)]
            # Independent fontTools writer -> Rust reader after a real edit.
            f['name'].setName('Independent writer',1,3,1,0x409)
            f['hmtx'].metrics['A']=(400,-7)
            external=tmp/f'{label}-fonttools.ttf';f.save(external)
            back=TTFont(external);back.ensureDecompiled();check_projection(back,inspect(a.binary,external))
            results.append(dict(fixture=label,sha256=hashlib.sha256(path.read_bytes()).hexdigest(),bytes=path.stat().st_size,glyphs=actual['glyphs'],mappings=len(mapping),cmap_formats=sorted({t.format for t in checked['cmap'].tables}),fonttools_full_ttx='pass',fonttools_edited_readback='pass',ots=result.stdout.decode().strip(),freetype_native_pixels_all_glyphs='pass',harfbuzz_horizontal_supplementary_vertical='pass' if label in ['common','fresh-common'] else 'not applicable to stress map'))
        # Exercise Unicode format 13 produced by an independent writer.
        f=TTFont(tmp/'common.ttf');table=CmapSubtable.newSubtable(13);table.platformID=3;table.platEncID=10;table.language=0;table.cmap={cp:'A' for cp in range(0x10000,0x10100)};f['cmap'].tables=[table]
        external=tmp/'format13.ttf';f.save(external);back=TTFont(external);check_projection(back,inspect(a.binary,external));results.append(dict(fixture='fontTools format13',mappings=256,rust_read='pass'))
    result=dict(scope='W2.1 common tables; uses prior compatibility outlines; no locl, CFF, embedded bitmaps or complete phase-2 claim',versions={name:importlib.metadata.version(name) for name in ['fonttools','freetype-py','uharfbuzz','opentype-sanitizer']},freetype_version=freetype.version(),harfbuzz_version=hb.version_string(),checks=results)
    text=json.dumps(result,ensure_ascii=False,indent=2)+'\n';print(text)
    if a.output:a.output.write_text(text)
if __name__=='__main__':main()
