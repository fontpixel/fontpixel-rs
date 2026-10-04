#!/usr/bin/env python3
"""W3.2 independent bytes, monobit bidirectional pixels, FreeType and catalogue boundary."""
import argparse,importlib.metadata,json,struct,subprocess,sys,tempfile
from pathlib import Path
sys.dont_write_bytecode=True
import monobit
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'tools'))
from gen_windows_cases import fnt,pe
REFERENCE=ROOT.parent/'reference/fontpixel/pipeline'
sys.path.insert(0,str(REFERENCE))
from opf.parsers.bdf import parse_bdf

def convert(binary,src,dst,*options):
    p=subprocess.run([binary,'convert',str(src),'-o',str(dst),'--report','json',*options],capture_output=True)
    assert p.returncode==0,(src.name,dst.name,p.stderr.decode(),p.stdout.decode())
    return json.loads(p.stdout)

def signature(font):
    return [(bytes(g.codepoint),g.as_matrix(),g.advance_width,g.left_bearing,g.shift_up) for g in font.glyphs]

def main():
    p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--output',required=True,type=Path);a=p.parse_args();results=[]
    with tempfile.TemporaryDirectory(prefix='fontpixel-w32-') as d:
        d=Path(d)
        for version in (1,2,3):
            src=d/f'original-v{version}.fnt';src.write_bytes(fnt(version));expected=monobit.load(src,format='win')[0]
            rust=d/f'rust-v{version}.fnt';convert(a.binary,src,rust,'--to','fnt','--strict');assert signature(monobit.load(rust,format='win')[0])==signature(expected)
            external=d/f'monobit-v{version}.fnt'
            g=monobit.Glyph.from_matrix([[1,0,1],[0,1,0],[1,0,1]],inklevels=(0,1),codepoint=65,shift_up=-1)
            generated=monobit.Font([g,g.modify(codepoint=66)],family='External',encoding='windows-1252',ascent=2,descent=1,point_size=3,dpi=(72,72),spacing='character-cell')
            monobit.save(generated,external,format='win',version=version,overwrite=True)
            separate=d/f'external-roundtrip-v{version}.fnt'
            convert(a.binary,external,separate,'--to','fnt','--strict');assert signature(monobit.load(separate,format='win')[0])==signature(monobit.load(external,format="win")[0])
            bdf=d/f'v{version}.bdf';convert(a.binary,rust,bdf,'--to','bdf');catalogue=parse_bdf(bdf,'windows-test',remap_charset=False)
            for glyph in expected.glyphs:
                cp=int.from_bytes(bytes(glyph.codepoint),'big');got=next(g for g in catalogue.glyphs if g.cp==cp)
                assert got.rows==glyph.as_bytes(),(version,cp,got.rows,glyph.as_bytes())
                assert (got.dwidth,got.bbw,got.bbh,got.bbx,got.bby)==(glyph.advance_width,glyph.width,glyph.height,glyph.left_bearing,glyph.shift_up)
            results.append({'case':f'FNT v{version}','result':'original wire + bidirectional monobit + catalogue BDF pixels/metrics pass'})
        external=d/'external.fon';pack=[monobit.load(d/f'original-v{v}.fnt',format='win')[0] for v in (2,3)]
        monobit.save(pack,external,format='mzfon',version=3,overwrite=True)
        external_pack=monobit.load(external,format="mzfon")
        rust=d/'rust.fon';convert(a.binary,external,rust,'--to','fon','--strict');back=monobit.load(rust,format='mzfon');assert list(map(signature,back))==list(map(signature,external_pack));results.append({'case':'multi-font NE','result':'bidirectional monobit fonts/pixels/metrics pass'})
        multi=d/'multi.fon';multi.write_bytes(rust.read_bytes())
        # Independent PE32 and PE32+ tree/RVA builders, then NE writer validated by monobit.
        for plus in (False,True):
            src=d/f'pe{64 if plus else 32}.fon';src.write_bytes(pe(fnt(2),plus));convert(a.binary,src,rust,'--to','fon','--strict');assert signature(monobit.load(rust,format='mzfon')[0])==signature(pack[0]);results.append({'case':f'PE{64 if plus else 32}','result':'independent tree/section fixture to NE pass'})
        # FreeType is an independent real font loader; it does not support Windows v1.
        try:
            import freetype
            for path,index in ((d/'rust-v2.fnt',0),(d/'rust-v3.fnt',0),(rust,0),(multi,0),(multi,1)):
                face=freetype.Face(str(path),index=index);face.select_size(0);face.set_charmap(face.charmaps[0])
                expected=monobit.load(path,format='mzfon' if path.suffix=='.fon' else 'win')[index]
                for g in expected.glyphs:
                    code=int.from_bytes(bytes(g.codepoint),'big');face.load_char(code,freetype.FT_LOAD_RENDER|freetype.FT_LOAD_MONOCHROME)
                    slot=face.glyph;assert slot.bitmap.width==g.width and slot.bitmap.rows==g.height
                    rows=bytes(slot.bitmap.buffer);stride=(g.width+7)//8;packed=b''.join(rows[y*abs(slot.bitmap.pitch):y*abs(slot.bitmap.pitch)+stride] for y in range(g.height));assert packed==g.as_bytes()
                    assert slot.advance.x//64==g.advance_width
            results.append({'case':'FreeType','result':'FNT2/FNT3 and NE glyph pixels/advances pass'})
        except ImportError:results.append({'case':'FreeType','result':'unavailable'})
        mini=REFERENCE/'tests/fixtures/mini.bdf';before=parse_bdf(mini,'mini')
        source_a=next(g for g in before.glyphs if g.cp==65)
        for version in (1,2,3):
            mid=d/f'mini-v{version}.fnt';back=d/f'mini-v{version}.bdf'
            report=convert(a.binary,mini,mid,'--to','fnt','--fnt-version',str(version))
            assert any(e['location']['codepoint']==27704 and e['impact']=='loss' for e in report['report']['entries'])
            convert(a.binary,mid,back,'--to','bdf');after=parse_bdf(back,'mini',remap_charset=False);assert [g.cp for g in after.glyphs]==[65]
            target=after.glyphs[0];assert (target.dwidth,target.bbw,target.bbh,target.bbx,target.bby)==(8,8,16,0,-2)
            assert target.rows==bytes(4)+source_a.rows+bytes(2)
        results.append({'case':'catalogue mini.bdf to FNT1/2/3 and back','result':'actual importer pixels/normalized metrics; CJK and unencoded omission reported'})
        # No direct FNT/FON importer exists in supplied ingest/run.py; its real BDF parser above is the supported boundary.
        run=(REFERENCE/'opf/ingest/run.py').read_text();assert 'convert_fnt(' not in run and 'convert_fon(' not in run
    result={'result':'pass','monobit_version':importlib.metadata.version('monobit'),'checks':results,'catalogue_direct_importer':'absent; actual supplied BDF parser exercised','scope':'PE fixtures independent; no claim of Windows OS installation test','catalogue_limitation':'Supplied Python parser rejects MICROSOFT/CP1252 BDF charset spelling; raw legacy-code mode is used explicitly at the BDF boundary. Unicode mapping is independently checked by Rust and monobit.'}
    a.output.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
if __name__=='__main__':main()
