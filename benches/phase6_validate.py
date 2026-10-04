#!/usr/bin/env python3
"""Independent complete-data phase-6 gates. No timing or historical pass reuse.

Each mode consumes actual generated files. Every success records current hashes.
Requires the pinned independent fontTools/FreeType/HarfBuzz/OTS/monobit packages.
"""
import argparse
import gc
import importlib.metadata
import io
import json
from pathlib import Path
import sys
import tempfile
from phase6_reference import family_plan, load_tong, parse_bdf, single_plan
from phase6_support import digest, inventory, invoke
from phase6_tong import expected_fonts

LANGUAGES=['zh-Hans','zh-Hant','zh-HK','ja','ko','en']
SCRIPTS=['DFLT','hani','kana','hang','bopo','latn','grek','cyrl']


def minimal_rotation(sequence):
    """Linear-time lexicographically minimal cyclic rotation, no winding reversal."""
    n=len(sequence)
    if not n:return ()
    doubled=sequence+sequence;i=0;j=1;k=0
    while i<n and j<n and k<n:
        a,b=doubled[i+k],doubled[j+k]
        if a==b:k+=1;continue
        if a>b:i=i+k+1;i+=i==j
        else:j=j+k+1;j+=i==j
        k=0
    start=min(i,j);return tuple(doubled[start:start+n])


def outlines(font):
    """Yield every fully decomposed exact contour, allowing start/order changes."""
    from fontTools.pens.basePen import BasePen
    glyphset=font.getGlyphSet()
    class Pen(BasePen):
        def __init__(self):super().__init__(glyphset);self.contours=[];self.segments=[];self.first=None
        def _moveTo(self,p):self.first=tuple(p);self.segments=[]
        def _lineTo(self,p):self.segments.append(('L',tuple(self._getCurrentPoint()),tuple(p)))
        def _qCurveToOne(self,p1,p2):self.segments.append(('Q',tuple(self._getCurrentPoint()),tuple(p1),tuple(p2)))
        def _curveToOne(self,p1,p2,p3):self.segments.append(('C',tuple(self._getCurrentPoint()),tuple(p1),tuple(p2),tuple(p3)))
        def _closePath(self):
            end=self._getCurrentPoint()
            if end is not None and self.first is not None and tuple(end)!=self.first:self.segments.append(('L',tuple(end),self.first))
            self.contours.append(('closed',minimal_rotation(self.segments)));self.segments=[]
        def _endPath(self):self.contours.append(('open',tuple(self.segments)));self.segments=[]
    for name in font.getGlyphOrder():
        pen=Pen();glyphset[name].draw(pen)
        assert not pen.segments,(name,'unterminated outline')
        yield sorted(pen.contours)


def sfnt_data(font):
    flavor=font.flavor;font.flavor=None
    target=io.BytesIO();font.save(target);font.flavor=flavor
    return target.getvalue()


def ft_face(path,font=None):
    import freetype
    return freetype.Face(io.BytesIO(sfnt_data(font))) if font is not None and font.flavor else freetype.Face(str(path))


def glyph_raster(face,gid,ppem=14):
    import freetype
    face.set_pixel_sizes(0,ppem);face.load_glyph(gid,freetype.FT_LOAD_RENDER | freetype.FT_LOAD_TARGET_MONO | freetype.FT_LOAD_NO_HINTING)
    slot=face.glyph;bm=slot.bitmap;data=bytes(bm.buffer)
    ink=tuple((slot.bitmap_left+x,slot.bitmap_top-y-1) for y in range(bm.rows) for x in range(bm.width)
              if data[y*abs(bm.pitch)+x//8] & (0x80>>(x%8)))
    return (slot.advance.x,slot.advance.y,ink)


def shaping(data,text,language,script,upm):
    import uharfbuzz as hb
    font=hb.Font(hb.Face(data));font.scale=(upm,upm)
    buffer=hb.Buffer();buffer.add_str(text);buffer.direction='ltr';buffer.script=script;buffer.language=language
    hb.shape(font,buffer)
    return [(g.codepoint,p.x_advance,p.y_advance,p.x_offset,p.y_offset) for g,p in zip(buffer.glyph_infos,buffer.glyph_positions)]


def all_shaping(left,right,cps,upm):
    text=''.join(map(chr,sorted(cps)));checks=0
    # Every codepoint in every declared script/language, not a sampled string.
    for script in SCRIPTS:
        for language in LANGUAGES:
            for start in range(0,len(text),2048):
                part=text[start:start+2048]
                assert shaping(left,part,language,script,upm)==shaping(right,part,language,script,upm),(script,language,start,'locl')
                checks+=1
    return checks


def optimize_woff_reader():
    from fontTools.ttLib.woff2 import WOFF2GlyfTable
    old=WOFF2GlyfTable._decodeComponents
    def without_tail_copy(self,glyph):
        self.compositeStream=memoryview(self.compositeStream);return old(self,glyph)
    WOFF2GlyfTable._decodeComponents=without_tail_copy


def pair_vector(native,reference,plan,shape_name,work):
    from fontTools.ttLib import TTFont
    import freetype,ots
    a,b=TTFont(native),TTFont(reference);a.ensureDecompiled();b.ensureDecompiled()
    ac,bc=a.getBestCmap(),b.getBestCmap();ao,bo=a.getGlyphOrder(),b.getGlyphOrder()
    expected=len(plan[0])+int(shape_name=='dot' and 'glyf' in a)
    assert len(ao)==len(bo)==expected,(native,'glyph count',len(ao),len(bo),expected)
    assert {cp:a.getGlyphID(n) for cp,n in ac.items()}=={cp:b.getGlyphID(n) for cp,n in bc.items()},(native,'cmap IDs')
    assert [a['hmtx'].metrics[n] for n in ao]==[b['hmtx'].metrics[n] for n in bo],(native,'all glyph metrics')
    assert a['head'].unitsPerEm==b['head'].unitsPerEm
    for tag,fields in [('hhea',['ascent','descent','lineGap','advanceWidthMax','minLeftSideBearing','minRightSideBearing','xMaxExtent','numberOfHMetrics']),
                       ('OS/2',['xAvgCharWidth','usWeightClass','usWidthClass','fsType','achVendID','fsSelection','sTypoAscender','sTypoDescender','sTypoLineGap','usWinAscent','usWinDescent','ulUnicodeRange1','ulUnicodeRange2','ulUnicodeRange3','ulUnicodeRange4','usMaxContext'])]:
        for field in fields:assert getattr(a[tag],field)==getattr(b[tag],field),(native,tag,field)
    names=lambda f:sorted((n.platformID,n.platEncID,n.langID,n.nameID,n.toUnicode()) for n in f['name'].names)
    assert names(a)==names(b),(native,'names')
    for gid,(left,right) in enumerate(zip(outlines(a),outlines(b),strict=True)):
        assert left==right,(native,gid,'exact decomposed outline geometry')
    with tempfile.TemporaryDirectory(prefix='vector-ots-',dir=work) as temporary:
        cleaned=Path(temporary)/'font.sfnt';status=ots.sanitize(str(native),str(cleaned),capture_output=True)
        assert status.returncode==0,(native,'OTS',status.stderr)
        faces=[ft_face(native,a),ft_face(reference,b),freetype.Face(str(cleaned))]
        for gid in range(expected):
            images=[glyph_raster(face,gid) for face in faces]
            assert images[0]==images[1]==images[2],(native,gid,'all-glyph FreeType/OTS')
            if gid<len(plan[0]) and plan[0][gid] in plan[1]:
                g=plan[1][plan[0][gid]]
                expected_ink={(g['x']+x,g['y']+len(g['rows'])-y-1) for y,row in enumerate(g['rows']) for x,v in enumerate(row) if v=='#'}
                assert set(images[0][2])==expected_ink,(native,gid,'source ink')
    checks=all_shaping(sfnt_data(a),sfnt_data(b),ac,a['head'].unitsPerEm)
    result={'native_sha256':digest(native),'reference_sha256':digest(reference),'glyphs':expected,'complete_outline_geometry':True,
            'all_glyph_metrics_pixels':True,'locl_batches':checks,'glyph_name_differences':sum(x!=y for x,y in zip(ao,bo))}
    a.close();b.close();return result


def web_equivalent(source,web,work,reference=False):
    from fontTools.ttLib import TTFont
    import freetype,ots
    a,b=TTFont(source),TTFont(web);b.ensureDecompiled()
    assert a.getGlyphOrder()==b.getGlyphOrder();assert a.getBestCmap()==b.getBestCmap()
    assert set(a.keys())==set(b.keys()),(web,'table inventory')
    for tag in a.keys():
        if tag not in ['GlyphOrder','head','glyf','loca']:assert a.reader[tag]==b.reader[tag],(web,tag)
    for key,value in vars(a['head']).items():
        if key not in ['checkSumAdjustment','flags','indexToLocFormat','tableTag']:assert getattr(b['head'],key)==value,(web,'head',key)
    assert a['head'].flags & ~0x800 == b['head'].flags & ~0x800
    for gid,(left,right) in enumerate(zip(outlines(a),outlines(b),strict=True)):assert left==right,(web,gid,'outline')
    raw_a,raw_b=sfnt_data(a),sfnt_data(b)
    faces=[freetype.Face(io.BytesIO(raw_a)),freetype.Face(io.BytesIO(raw_b))]
    count=len(a.getGlyphOrder())
    for gid in range(count):assert glyph_raster(faces[0],gid)==glyph_raster(faces[1],gid),(web,gid,'decoded pixels')
    rejection=None
    with tempfile.TemporaryDirectory(prefix='web-ots-',dir=work) as temporary:
        cleaned=Path(temporary)/'font.sfnt';status=ots.sanitize(str(web),str(cleaned),capture_output=True)
        if status.returncode:
            assert reference and b'DefaultLangSys is NULL' in status.stderr,(web,'OTS',status.stderr)
            dflt=next(r.Script for r in a['GSUB'].table.ScriptList.ScriptRecord if r.ScriptTag=='DFLT')
            assert dflt.DefaultLangSys is None
            base_status=ots.sanitize(str(source),str(cleaned),capture_output=True)
            assert base_status.returncode and b'DefaultLangSys is NULL' in base_status.stderr
            rejection=status.stderr.decode()
        else:
            sanitized=freetype.Face(str(cleaned))
            for gid in range(count):assert glyph_raster(faces[0],gid)==glyph_raster(sanitized,gid),(web,gid,'sanitized pixels')
    checks=all_shaping(raw_a,raw_b,a.getBestCmap(),a['head'].unitsPerEm)
    a.close();b.close()
    return {'source_sha256':digest(source),'web_sha256':digest(web),'glyphs':count,'complete_outlines_pixels_tables':True,
            'shaping_batches':checks,'original_reference_ots_rejection':rejection}


def bitmap_pair(source,native,reference,kind):
    import freetype
    records=parse_bdf(source);expected={cp for cp in records if kind=='otb' or cp<=0xffff}
    faces=[freetype.Face(str(p)) for p in [source,native,reference]]
    coverage=[{cp for cp,gid in f.get_chars() if gid} for f in faces]
    assert coverage[0]==set(records);assert coverage[1]==coverage[2]==expected,(native,'coverage')
    for cp in sorted(expected):
        rasters=[glyph_raster(f,f.get_char_index(cp)) for f in faces]
        assert rasters[0]==rasters[1]==rasters[2],(native,cp,'pixels/advance')
    return {'source_sha256':digest(source),'native_sha256':digest(native),'reference_sha256':digest(reference),
            'glyphs_checked':len(expected),'format_unrepresentable_supplementary':len(records)-len(expected)}


def main():
    p=argparse.ArgumentParser(description=__doc__);sub=p.add_subparsers(dest='mode',required=True)
    q=sub.add_parser('tong')
    for key in ['native','reference','reference-root','bdf','output']:q.add_argument('--'+key,type=Path,required=True)
    q=sub.add_parser('codec')
    for key in ['native','compat','python','node','source','output']:q.add_argument('--'+key,type=Path,required=True)
    q=sub.add_parser('merge')
    for key in ['reference','latin','unifont','merged','output']:q.add_argument('--'+key,type=Path,required=True)
    q=sub.add_parser('vector')
    for key in ['native','reference','reference-root','source','output']:q.add_argument('--'+key,type=Path,required=True)
    q=sub.add_parser('web')
    for key in ['source','web','output']:q.add_argument('--'+key,type=Path,required=True)
    a=p.parse_args();a.output.parent.mkdir(parents=True,exist_ok=True)
    with a.output.open('x') as f:json.dump({'partial':True},f)
    result={'partial':True,'mode':a.mode,'results':[]}
    def save():a.output.write_text(json.dumps(result,indent=2)+'\n')
    if a.mode in ['tong','vector','web']:optimize_woff_reader()
    if a.mode=='tong':
        ref=load_tong(a.reference_root)
        for directory in [a.native,a.reference]:
            files={str(p.relative_to(directory)) for kind in ['bdf','pcf','otb','ttf','otf','woff2'] for p in (directory/kind).glob('*') if p.is_file()}
            assert files==expected_fonts(),(directory,'complete84')
        for source in sorted(a.bdf.glob('*.bdf')):
            assert source.read_bytes()==(a.native/'bdf'/source.name).read_bytes()==(a.reference/'bdf'/source.name).read_bytes()
            for kind in ['pcf','otb']:
                name=source.stem+'.'+kind;result['results'].append(bitmap_pair(source,a.native/kind/name,a.reference/kind/name,kind));save()
        for size in ['Large','Small']:
            for spacing in ['prop','mono']:
                plan=family_plan(ref,a.bdf,size,spacing)
                stem='TongPixelSans'+('Mono' if spacing=='mono' else '')+'14'+size
                for shape in ['square','dot']:
                    for kind in ['ttf','otf']:
                        name=stem+'-'+shape+'.'+kind
                        result['results'].append(pair_vector(a.native/kind/name,a.reference/kind/name,plan,shape,a.output.parent));save();gc.collect()
                    kind='otf' if shape=='square' else 'ttf'
                    for engine,directory in [('native',a.native),('reference',a.reference)]:
                        name=stem+'-'+shape
                        result['results'].append(web_equivalent(directory/kind/(name+'.'+kind),directory/'woff2'/(name+'.woff2'),a.output.parent,engine=='reference'));save();gc.collect()
        result['output_identities']={'native':inventory(a.native),'reference':inventory(a.reference)}
    elif a.mode=='vector':
        ref=load_tong(a.reference_root);plan=single_plan(ref,parse_bdf(a.source))
        result['results'].append(pair_vector(a.native,a.reference,plan,'square',a.output.parent))
    elif a.mode=='web':result['results'].append(web_equivalent(a.source,a.web,a.output.parent))
    elif a.mode=='merge':
        protected=Path(__file__).resolve().parents[2]/'reference'
        if a.reference.resolve().is_relative_to(protected.resolve()):raise ValueError('use copied bdfparser reference')
        sys.dont_write_bytecode=True;sys.path.insert(0,str(a.reference/'src'))
        from bdfparser import Font
        latin=Font(a.latin);unifont=Font(a.unifont);merged=Font(a.merged)
        assert len(latin.glyphs)==3645 and len(unifont.glyphs)==57086
        expected=dict(unifont.glyphs);expected.update(latin.glyphs)
        assert len(expected)==len(merged.glyphs)==57106
        assert merged.glyphs==expected,'all complete raw glyph metadata/rows must match western-first union'
        result['results'].append({'glyphs':57106,'latin_glyphs':3645,'unifont_contribution':53461,'merged_sha256':digest(a.merged),
            'latin_sha256':digest(a.latin),'unifont_sha256':digest(a.unifont),'complete_glyph_records':True})
    else:
        artifacts=['compat.bdf','compat.pcf','compat.ttf','screen.raster','drawall.raster','crop.raster','concat.raster']
        for name in artifacts:
            hashes=[digest(directory/name) for directory in [a.compat,a.python,a.node]]
            assert len(set(hashes))==1,(name,'Rust/Python/Node full byte parity')
            result['results'].append({'file':name,'sha256':hashes[0],'engines':3})
        for name in ['screen.raster','drawall.raster','crop.raster','concat.raster']:
            assert (a.native/name).read_bytes()==(a.compat/name).read_bytes(),(name,'native raster')
        assert (a.native/'native.bdf').read_bytes()==(a.compat/'compat.bdf').read_bytes()
        assert (a.native/'native.pcf').read_bytes()==(a.compat/'compat.pcf').read_bytes()
        import monobit,freetype
        source=monobit.load(a.source,format='bdf')[0];yaff=monobit.load(a.native/'converted.yaff',format='yaff')[0]
        def signature(font):
            def char(g):return g.char.value if g.char else chr(int(g.codepoint)) if g.codepoint and font.encoding in ('unicode','iso10646-1') else ''
            return [(g.as_matrix(),g.advance_width,g.left_bearing,g.shift_up,char(g)) for g in font.glyphs]
        assert len(source.glyphs)==38653;assert signature(source)==signature(yaff),'complete independent YAFF glyph signature'
        records=parse_bdf(a.source)
        source_face=freetype.Face(str(a.source))
        for kind in ['pcf','otb','ttf']:
            target=a.native/('converted.'+kind);face=freetype.Face(str(target))
            cps=set(records) if kind!='pcf' else {cp for cp in records if cp<=0xffff}
            assert {cp for cp,gid in face.get_chars() if gid}==cps
            for cp in sorted(cps):assert glyph_raster(source_face,source_face.get_char_index(cp))==glyph_raster(face,face.get_char_index(cp)),(kind,cp,'source glyph pixels/advance')
            result['results'].append({'file':str(target),'sha256':digest(target),'glyphs':len(cps),'independent_freetype':True})
        result['results'].append({'file':'converted.yaff','sha256':digest(a.native/'converted.yaff'),'independent_monobit_glyphs':38653})
    result.update(partial=False,status='pass');save();print(json.dumps({'status':'pass','checks':len(result['results'])}))

if __name__=='__main__':main()
