#!/usr/bin/env python3
"""WOFF interoperability: independent fontTools/Brotli, OTS, FreeType and HarfBuzz.

Generate Tong reference fonts outside protected sources, then check native
WRITERS with independent readers and native READERS with independent writers.
No external fonts are copied into the repository. Use --keep for fuzz/bench seeds.
"""
import argparse, hashlib, importlib.metadata, io, json, subprocess, tempfile
from pathlib import Path
import freetype, ots
from fontTools.ttLib import TTFont
from fontTools.ttLib.woff2 import WOFF2FlavorData
from fontTools.ttLib.tables._g_l_y_f import Glyph, GlyphComponent
from fontTools.ttLib.tables.ttProgram import Program
from fontTools.pens.ttGlyphPen import TTGlyphPen
from glyf import load_reference, parse_bdf, bitmap, shape, fonttools_contours
ROOT=Path(__file__).resolve().parents[2]

def invoke(binary,*args):
    return subprocess.check_output([str(binary),*map(str,args)],text=True)

def equivalent(original,result,label,pixels=True):
    left,right=TTFont(original),TTFont(result)
    right.ensureDecompiled()
    assert right.getGlyphOrder()==left.getGlyphOrder(),(label,'order')
    assert right.getBestCmap()==left.getBestCmap(),(label,'cmap')
    assert set(left.keys())==set(right.keys()),(label,'tables')
    for tag in left.keys():
        if tag in ['GlyphOrder','head','glyf','loca']:continue
        assert left.reader[tag]==right.reader[tag],(label,tag)
    lh,rh=left['head'],right['head']
    # Required WOFF2 transformation bit, checksum adjustment and reconstruction
    # offsets are wire differences; every other head field remains unchanged.
    for k,v in vars(lh).items():
        if k in ['checkSumAdjustment','flags','indexToLocFormat','tableTag']:continue
        assert getattr(rh,k)==v,(label,'head',k)
    assert rh.flags&~0x800==lh.flags&~0x800
    if 'glyf' in left:
        for name in left.getGlyphOrder():
            a,b=left['glyf'][name],right['glyf'][name]
            assert a.numberOfContours==b.numberOfContours,(label,name,'contours')
            if a.numberOfContours:
                assert (a.xMin,a.yMin,a.xMax,a.yMax)==(b.xMin,b.yMin,b.xMax,b.yMax),(label,name,'bbox')
                assert getattr(a,'program',None)==getattr(b,'program',None),(label,name,'instructions')
                if a.isComposite():
                    assert [vars(c) for c in a.components]==[vars(c) for c in b.components],(label,name,'components')
                else:
                    assert list(a.coordinates)==list(b.coordinates),(label,name,'points')
                    assert list(a.flags)==list(b.flags),(label,name,'flags')
                    assert list(a.endPtsOfContours)==list(b.endPtsOfContours),(label,name,'ends')
    if pixels:
        af,bf=freetype.Face(str(original)),freetype.Face(str(result))
        for gid in range(len(left.getGlyphOrder())):assert bitmap(af,gid)==bitmap(bf,gid),(label,gid,'FreeType')
    # hb.Face cannot open WOFF itself. Check the reconstructed sfnt separately.
    return len(left.getGlyphOrder())

def verify(binary,source,tmp,label,check_ots=True):
    results=[]
    for kind,q,threads,transform in [('woff',9,1,True),('woff2',11,1,True),('woff2',5,4,True),('woff2',5,1,False)]:
        name=f'{label}-{kind}-q{q}-t{threads}-x{int(transform)}'
        web=tmp/(name+'.'+kind);out=tmp/(name+'.sfnt')
        invoke(binary,'encode',kind,source,web,q,threads,*([] if transform else ['null']))
        equivalent(source,web,name,check_ots)
        if kind=='woff':
            def physical(path):
                font=TTFont(path);return [tag for tag,_ in sorted(font.reader.tables.items(),key=lambda item:(item[1].offset,getattr(item[1],'origLength',item[1].length),item[0]))]
            assert physical(web)==physical(source),(name,'WOFF1 physical table order')
        if check_ots:
            sanitized=tmp/(name+'-ots.sfnt');status=ots.sanitize(str(web),str(sanitized),capture_output=True)
            assert status.returncode==0,(name,status.stderr.decode(),status.stdout.decode())
            # OTS deliberately drops some non-web tables; its own clean sfnt is
            # used only for raster equivalence, not byte-preservation claims.
            a,b=freetype.Face(str(source)),freetype.Face(str(sanitized))
            for gid in range(a.num_glyphs):assert bitmap(a,gid)==bitmap(b,gid),(name,gid,'OTS')
        invoke(binary,'decode',kind,web,out);count=equivalent(source,out,name,check_ots)
        if kind=='woff':assert source.read_bytes()==out.read_bytes(),(name,'WOFF1 exact sfnt reconstruction')
        if check_ots:
            cmap=TTFont(source).getBestCmap();text=''.join(chr(cp) for cp in list(cmap)[:100])
            for lang in ['zh-Hans','zh-Hant','zh-HK','ja','ko','en']:
                assert shape(source.read_bytes(),text,lang)==shape(out.read_bytes(),text,lang),(name,'HarfBuzz',lang)
        results.append({'case':name,'glyphs':count,'bytes':web.stat().st_size,'sha256':hashlib.sha256(web.read_bytes()).hexdigest(),'fontTools':'pass','native_roundtrip':'pass','OTS_FreeType_HarfBuzz':'pass' if check_ots else 'not applicable to bitmap-only sfnt'})
    for kind,transforms in [('woff',None),('woff2',None),('woff2',set()),('woff2',{'glyf','loca','hmtx'})]:
        f=TTFont(source,recalcTimestamp=False,recalcBBoxes=False);f.flavor=kind
        if kind=='woff2' and transforms is not None:f.flavorData=WOFF2FlavorData(transformedTables=transforms)
        name=label+'-external-'+kind+str(transforms);web=tmp/(hashlib.sha256(name.encode()).hexdigest()[:12]+'.'+kind);out=web.with_suffix('.sfnt')
        f.save(web);invoke(binary,'decode',kind,web,out);equivalent(source,out,name,check_ots)
    return results

def main():
    p=argparse.ArgumentParser();p.add_argument('--package',type=Path,default=ROOT.parent);p.add_argument('--binary',type=Path,default=ROOT/'target/release/examples/woff_check');p.add_argument('--output',type=Path);p.add_argument('--keep',type=Path);p.add_argument('--cjk-count',type=int,default=256);p.add_argument('--font');a=p.parse_args()
    ref=load_reference(a.package);results=[]
    with tempfile.TemporaryDirectory(prefix='fontpixel-woff-') as temp:
        tmp=a.keep or Path(temp);tmp.mkdir(parents=True,exist_ok=True)
        for path in sorted((a.package/'samples/tong-pixel-sans').glob('*.bdf')):
            if a.font and path.name!=a.font:continue
            source=parse_bdf(path);cps=sorted(source)
            if 'Latin' not in path.name and a.cjk_count<len(cps):cps=sorted({cps[i*(len(cps)-1)//(a.cjk_count-1)] for i in range(a.cjk_count)})
            records={ref.gname(cp):source[cp] for cp in cps};order=['.notdef']+list(records);cmap={cp:ref.gname(cp) for cp in cps}
            size='Small' if 'Small' in path.name else 'Large';spacing='mono' if 'Mono' in path.name else 'prop';ref.set_size(size)
            for kind,builder in [('ttf',ref.build_ttf),('otf',ref.build_otf)]:
                for s in ['square','dot']:
                    font=builder(spacing,s,'test',(order,records,cmap,{}),size);label=path.stem+'-'+kind+'-'+s;fp=tmp/(label+'.'+kind);font.save(fp)
                    results.extend(verify(a.binary,fp,tmp,label));print(label,'PASS',len(order),flush=True)
                    if a.output:a.output.write_text(json.dumps({'partial':True,'results':results},indent=2)+'\n')
        records={'A':{'rows':['#'],'x':0,'y':0,'adv':5},'B':{'rows':['#'],'x':0,'y':0,'adv':6}}
        special=ref.build_ttf('prop','square','test',(['.notdef','A','B'],records,{65:'A',66:'B'},{}),'Large')
        pen=TTGlyphPen(None);pen.moveTo((-4000,-4000));pen.lineTo((4000,-4000));pen.qCurveTo((4095,4095),(64,65));pen.lineTo((-65,768));pen.closePath()
        g=pen.glyph();g.program=Program();g.program.fromBytecode([0xb0,0,0x21]);g.flags[0]|=0x40;special['glyf']['A']=g
        g=Glyph();g.numberOfContours=-1;c=GlyphComponent();c.glyphName='A';c.x=0;c.y=0;c.flags=0x1204;c.transform=[[0.5,0.25],[-0.25,0.5]];g.components=[c];g.program=Program();g.program.fromBytecode([0xb0,0,0x21]);special['glyf']['B']=g
        special.save(tmp/'special-base.ttf');special=TTFont(tmp/'special-base.ttf',recalcTimestamp=False,recalcBBoxes=False)
        special['glyf']['A'].xMin-=1;special['glyf']['B'].xMax+=1
        special.save(tmp/'special.ttf');results.extend(verify(a.binary,tmp/'special.ttf',tmp,'hints-overlap-explicit-bbox-affine'))
        for path in sorted((ROOT/'tests/data/embedded-bitmap').glob('*.ttf')):
            results.extend(verify(a.binary,path,tmp,path.stem,False))
        output={'tools':{n:importlib.metadata.version(n) for n in ['fonttools','brotli','freetype-py','uharfbuzz','opentype-sanitizer']},'cjk_count':a.cjk_count,'results':results,
        'scope':'Standalone WOFF envelopes, actual transforms and preserved glyph semantics. Native locl creation and TTC containers remain later work items.'}
        if a.output:a.output.write_text(json.dumps(output,indent=2)+'\n')
        print(json.dumps({'cases':len(results),'glyph_checks':sum(r['glyphs'] for r in results)}))
if __name__=='__main__':main()
