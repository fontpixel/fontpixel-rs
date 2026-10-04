#!/usr/bin/env python3
"""Every catalogue artifact: explicit metadata, metrics, physical pixels, notices.

Requires freshly generated original and corrected native outputs. No historical
SHA/summary substitutes for these decoded checks. Remaining policy differences
are listed, never blanket sfnt equality. No timed operation is performed here.
"""
import argparse
import gzip
import io
import json
from pathlib import Path
import sys
import tempfile
import zipfile
from phase6_catalogue_metadata import POLICIES, bitmap_properties, sfnt_metadata
from phase6_support import digest


def pixels(g):
    stride=(g.bbw+7)//8; data=bytearray(g.rows)
    if g.bbw%8:
        mask=(0xff << (8-g.bbw%8)) & 0xff
        for row in range(g.bbh): data[row*stride+stride-1]&=mask
    return bytes(data).hex()


def glyphs(font):
    return {g.cp:(g.bbw,g.bbh,g.bbx,g.bby,g.dwidth,pixels(g)) for g in font.glyphs}


def alternates(font):
    return [(g.name,g.bbw,g.bbh,g.bbx,g.bby,g.dwidth,pixels(g)) for g in font.unencoded_glyphs]


def raster(face, cp, ppem):
    import freetype
    face.set_pixel_sizes(0,ppem)
    face.load_char(cp,freetype.FT_LOAD_RENDER | freetype.FT_LOAD_TARGET_MONO | freetype.FT_LOAD_NO_HINTING)
    slot=face.glyph; bitmap=slot.bitmap; data=bytes(bitmap.buffer)
    ink=[(slot.bitmap_left+x,slot.bitmap_top-y-1) for y in range(bitmap.rows) for x in range(bitmap.width)
         if data[y*abs(bitmap.pitch)+x//8] & (0x80>>(x%8))]
    return int(slot.advance.x),ink


def main():
    p=argparse.ArgumentParser(description=__doc__)
    for name in ['reference','plan','native','python','output']:p.add_argument('--'+name,type=Path,required=True)
    a=p.parse_args();a.output.parent.mkdir(parents=True,exist_ok=True)
    protected=Path(__file__).resolve().parents[2]/'reference'
    if a.reference.resolve().is_relative_to(protected.resolve()):raise ValueError('use copied reference code')
    sys.dont_write_bytecode=True;sys.path.insert(0,str(a.reference/'fontpixel/pipeline'))
    from opf.parsers.bdf import parse_bdf
    from opf.parsers.pcf import parse_pcf
    from fontTools.ttLib import TTFont
    from fontTools.ttLib.woff2 import WOFF2GlyfTable
    import freetype,ots
    original=WOFF2GlyfTable._decodeComponents
    def no_tail_copy(self,glyph):
        self.compositeStream=memoryview(self.compositeStream);return original(self,glyph)
    WOFF2GlyfTable._decodeComponents=no_tail_copy
    plan=json.loads(a.plan.read_text());assert plan.get('version')==2,'version2 corrected plan required'
    tasks=plan['tasks'];assert len(tasks)==173
    expected={t['file'] for t in tasks}|{t['slug']+'.zip' for t in tasks}
    assert {p.name for p in a.native.iterdir() if p.is_file()}==expected
    assert {p.name for p in a.python.iterdir() if p.is_file()}==expected
    result={'partial':True,'results':[],'wire_differences':[],'policy_definitions':POLICIES}
    def save():a.output.write_text(json.dumps(result,indent=2)+'\n')
    for task in tasks:
        name=task['file'];left,right=a.native/name,a.python/name;kind=task['kind']
        record={'file':name,'kind':kind,'native_sha256':digest(left),'python_sha256':digest(right)}
        if record['native_sha256']!=record['python_sha256']:result['wire_differences'].append(name)
        if kind in ['bdf','pcf']:
            parser=parse_bdf if kind=='bdf' else parse_pcf
            n,r=parser(left,task['slug']),parser(right,task['slug'])
            record['metadata']=bitmap_properties(task,n,r)
            assert glyphs(n)==glyphs(r),(name,'complete encoded glyph records')
            assert alternates(n)==alternates(r),(name,'complete unencoded glyph records')
            record.update(glyphs=len(glyphs(n)),unencoded_glyphs=len(alternates(n)),complete_glyph_records_equal=True)
        else:
            n,r=TTFont(left),TTFont(right);n.ensureDecompiled();r.ensureDecompiled()
            record['metadata']=sfnt_metadata(task,n,r)
            cps=set(n.getBestCmap());assert cps==set(r.getBestCmap()),(name,'cmap')
            def face(path,font):
                if font.flavor:
                    flavor=font.flavor;font.flavor=None;data=io.BytesIO();font.save(data);font.flavor=flavor
                    return freetype.Face(io.BytesIO(data.getvalue()))
                return freetype.Face(str(path))
            nf,rf=face(left,n),face(right,r)
            presence=None
            if kind=='ttf':
                def strikes(font):
                    mapping=font.getBestCmap()
                    return {strike.bitmapSizeTable.ppemY:{cp for cp,glyph in mapping.items() if glyph in font['EBDT'].strikeData[index]}
                            for index,strike in enumerate(font['EBLC'].strikes)}
                np,rp=strikes(n),strikes(r);assert np==rp,(name,'physical strike presence')
                ppems=sorted(rp);presence={ppem:len(rp[ppem]) for ppem in ppems}
            else:ppems=[task['pixel_size'],task['pixel_size']*2]
            checks=0
            for ppem in ppems:
                for cp in sorted(rp[ppem] if kind=='ttf' else cps):
                    assert raster(nf,cp,ppem)==raster(rf,cp,ppem),(name,cp,ppem,'pixels/advance');checks+=1
            if kind!='ttf':
                with tempfile.TemporaryDirectory(prefix='ots-',dir=a.output.parent) as temporary:
                    status=ots.sanitize(str(left),str(Path(temporary)/'font.sfnt'),capture_output=True)
                    assert status.returncode==0,(name,'OTS',status.stderr)
                record['ots']=True
            record.update(glyphs=len(cps),ppems=ppems,physical_strike_presence=presence,render_checks=checks,
                          native_sfnt_glyphs=n['maxp'].numGlyphs,python_sfnt_glyphs=r['maxp'].numGlyphs,
                          scope='all encoded pixels/advances and explicit app metadata/common metrics; unencoded sfnt identities/metrics are inventoried separately')
            n.close();r.close()
        result['results'].append(record);save();print(name,'PASS',flush=True)
    for slug in sorted({t['slug'] for t in tasks}):
        with zipfile.ZipFile(a.native/(slug+'.zip')) as n,zipfile.ZipFile(a.python/(slug+'.zip')) as r:
            assert n.namelist()==r.namelist(),(slug,'archive inventory')
            for name in n.namelist():
                if not name.endswith('.bdf'):assert n.read(name)==r.read(name),(slug,name,'license/provenance')
                else:
                    task=next(t for t in tasks if t['slug']==slug and t['kind']=='bdf' and t['variant']+'.bdf'==name)
                    assert n.read(name)==gzip.decompress((a.native/task['file']).read_bytes()),(slug,name,'native ZIP BDF identity')
                    assert r.read(name)==gzip.decompress((a.python/task['file']).read_bytes()),(slug,name,'reference ZIP BDF identity')
    result.update(partial=False,status='pass',font_files=173,archives=8,explicit_metadata_and_common_metrics=True,encoded_pixels_and_advances=True,archive_licenses_notices=True)
    save();print(json.dumps({'complete':True,'font_files':173,'archives':8,'wire_difference_files':len(result['wire_differences'])}))

if __name__=='__main__':main()
