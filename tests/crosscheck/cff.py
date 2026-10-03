#!/usr/bin/env python3
"""CFF1 native encoder/reader vs Tong, fontTools, OTS, FreeType and HarfBuzz.

Reads protected input BDFs; references are generated in temporary directories.
Supports --font and --cjk-count for full-font validation, keeping paths streamed.
"""
import argparse, hashlib, importlib.metadata, json, subprocess, tempfile
from pathlib import Path
import freetype, ots
from fontTools.ttLib import TTFont
from fontTools.pens.recordingPen import RecordingPen
from fontTools.misc.psCharStrings import T2CharString
from fontTools.pens.t2CharStringPen import T2CharStringPen
from fontTools.cffLib import SubrsIndex, FDArrayIndex, FontDict, FDSelect, PrivateDict
from glyf import load_reference, parse_bdf, bitmap, expected_ink, actual_ink, shape
ROOT=Path(__file__).resolve().parents[2]

def commands(font):
    gs=font.getGlyphSet()
    for name in font.getGlyphOrder():
        pen=RecordingPen();gs[name].draw(pen)
        yield [[op,*points] for op,points in pen.value]

def canon(cmds):
    paths=[];path=[]
    for c in cmds:
        op=c[0];p=[tuple(float(v) for v in point) for point in c[1:]]
        if op=='moveTo':path=[(op,*p)]
        elif op in ('lineTo','curveTo'):path.append((op,*p))
        elif op=='closePath':
            if len(path)>1 and path[-1][0]=='lineTo' and path[-1][1]==path[0][1]:path.pop()
            if all(x[0] in ('moveTo','lineTo') for x in path):
                pts=[x[1] for x in path];path=min(tuple(pts[i:]+pts[:i]) for i in range(len(pts)))
            else:path=tuple(path)
            paths.append(path);path=[]
        else:raise AssertionError(('unexpected operation',op))
    assert not path
    return sorted(paths,key=lambda p:(bool(p and isinstance(p[0][0],str)),p))

def inspect(binary,path,expected):
    p=subprocess.Popen([str(binary),'--inspect-stream',str(path)],stdout=subprocess.PIPE,text=True)
    try:
        for gid,cs in enumerate(commands(expected)):
            line=p.stdout.readline();assert line,('missing glyph',gid)
            g=json.loads(line);assert g['gid']==gid
            assert canon(g['commands'])==canon(cs),('reader geometry',gid,g['commands'][:5],cs[:5])
            # CFF advance and hmtx agree in our generated independent references.
            n=expected.getGlyphOrder()[gid];assert g['advance']==expected['hmtx'][n][0],('width',gid,g['advance'],expected['hmtx'][n])
        assert not p.stdout.read();assert p.wait()==0
    finally:
        if p.poll() is None:p.kill();p.wait()

def verify(binary,reference,records,shape_name,tmp,label):
    rp=tmp/(label+'-reference.otf');out=tmp/(label+'-native.otf');jp=tmp/(label+'.json')
    if callable(reference):reference=reference()
    reference.save(rp);reference=TTFont(rp);order=reference.getGlyphOrder()
    def progress(step):
        if len(order)>10000:print(label,step,flush=True)
    jp.write_text(json.dumps({'glyphs':[records.get(n) for n in order]}))
    subprocess.run([str(binary),'--generate',str(rp),str(jp),shape_name,str(out)],check=True)
    actual=TTFont(out,checkChecksums=2);actual.ensureDecompiled();actual.saveXML(tmp/(label+'.ttx'))
    assert actual.getGlyphOrder()==order;assert actual.getBestCmap()==reference.getBestCmap()
    for tag in ['hmtx','hhea','OS/2','name','post','cmap','GSUB']:
        if tag in reference:assert actual.getTableData(tag)==reference.getTableData(tag),(label,tag)
    progress('checking all writer paths')
    for gid,(a,b) in enumerate(zip(commands(actual),commands(reference))):assert canon(a)==canon(b),(label,gid,'writer paths',a[:4],b[:4])
    top=actual['CFF '].cff.topDictIndex[0];assert len(top.CharStrings.charStrings)==len(order)
    assert len(actual['CFF '].cff.GlobalSubrs)==int(shape_name=='dot')
    if shape_name=='dot':
        for n,g in records.items():
            cs=top.CharStrings[n];cs.decompile();assert cs.program.count('callgsubr')==sum(row.count('#') for row in g['rows'])
    progress('checking native reads of both fonts')
    inspect(binary,out,actual);inspect(binary,rp,reference)
    progress('checking OTS and all FreeType source pixels')
    sanitized=tmp/(label+'-ots.otf');status=ots.sanitize(str(out),str(sanitized),capture_output=True)
    assert status.returncode==0,(label,status.stderr.decode(),status.stdout.decode())
    ft=freetype.Face(str(out));rf=freetype.Face(str(rp));sf=freetype.Face(str(sanitized))
    for i,n in enumerate(order):
        b=bitmap(ft,i);assert bitmap(rf,i)==b,(label,n,'FreeType reference');assert bitmap(sf,i)==b,(label,n,'OTS raster change')
        if n in records:assert actual_ink(b)==expected_ink(records[n]),(label,n,'source pixel mismatch');assert b[4]==records[n]['adv']*64
    text=''.join(chr(c) for c in list(reference.getBestCmap())[:200])
    for language in ['zh-Hans','zh-Hant','zh-HK','ja','ko','en']:assert shape(out.read_bytes(),text,language)==shape(rp.read_bytes(),text,language),(label,language)
    progress('normalizing external font and checking all paths')
    del actual,top
    repack=tmp/(label+'-repack.otf');subprocess.run([str(binary),'--repack',str(rp),str(repack)],check=True);again=TTFont(repack);again.ensureDecompiled()
    for a,b in zip(commands(again),commands(reference)):assert canon(a)==canon(b),(label,'repack')
    inspect(binary,repack,again)
    progress('all checks passed')
    return {'case':label,'glyphs':len(order),'bytes':out.stat().st_size,'sha256':hashlib.sha256(out.read_bytes()).hexdigest(),
        'fontTools':'pass','OTS':'pass','FreeType_source_and_reference':'pass','HarfBuzz_6_languages':'pass','native_reader_external_and_own':'pass','external_read_reencode':'pass'}

def special_cases(binary,ref,tmp):
    records={'A':{'rows':['#'],'adv':4,'x':0,'y':0},'B':{'rows':['#'],'adv':4,'x':0,'y':0}}
    font=ref.build_otf('prop','square','test',(['.notdef','A','B'],records,{65:'A',66:'B'},{}),'Large')
    top=font['CFF '].cff.topDictIndex[0];private=top.Private
    private.defaultWidthX=400;private.nominalWidthX=100
    private.Subrs=SubrsIndex();private.Subrs.append(T2CharString(program=[100,0,0,100,-100,0,'rlineto','return'],private=private))
    # Width defaults, local calls, 1-byte hint masks, hv/vh/hh/vv curves and flex.
    top.CharStrings['A']=T2CharString(program=[10,20,'hstem',30,20,'vstem','hintmask',b'\xc0',0,0,'rmoveto',-107,'callsubr','endchar'],private=private)
    top.CharStrings['B']=T2CharString(program=[300,0,0,'rmoveto',20,20,20,20,'hvcurveto',20,20,20,20,'vhcurveto',20,20,20,20,'hhcurveto',20,20,20,20,'vvcurveto',5,5,5,5,5,5,5,'hflex',5,5,5,5,5,5,5,5,5,'hflex1',1,1,1,1,1,1,1,1,1,1,1,'flex1',1,1,1,1,1,1,1,1,1,1,1,1,50,'flex','endchar'],private=private)
    # Existing .notdef was authored with nominal 0; compensate to retain 700.
    top.CharStrings['.notdef'].decompile();top.CharStrings['.notdef'].program[0]-=100
    p=tmp/'external-operators.otf';font.save(p);font=TTFont(p);inspect(binary,p,font)
    out=tmp/'external-operators-repack.otf';subprocess.run([str(binary),'--repack',str(p),str(out)],check=True)
    again=TTFont(out)
    for a,b in zip(commands(again),commands(font)):assert canon(a)==canon(b)
    scaled=TTFont(p);scaled['CFF '].cff.topDictIndex[0].FontMatrix=[2/1400,0,0,2/1400,0,0]
    mp=tmp/'external-fontmatrix.otf';scaled.save(mp)
    got=[json.loads(line) for line in subprocess.check_output([str(binary),'--inspect-stream',str(mp)],text=True).splitlines()]
    for gid,cs in enumerate(commands(TTFont(p))):
        expected=[[c[0],*[[v*2 for v in point] for point in c[1:]]] for c in cs]
        assert canon(got[gid]['commands'])==canon(expected),('FontMatrix',gid)
        assert got[gid]['advance']==scaled['hmtx'][scaled.getGlyphOrder()[gid]][0]*2
    # CID-keyed input with two FDs, distinct local subroutines and a custom matrix.
    font=TTFont(p);cff=font['CFF '].cff;top=cff.topDictIndex[0];top.ROS=('Adobe','Identity',0);top.CIDCount=3
    order=['.notdef','cid00001','cid00002'];top.charset=order
    top.FDArray=FDArrayIndex();top.FDArray.strings=cff.strings;top.FDArray.GlobalSubrs=cff.GlobalSubrs
    for i in range(2):
        fd=FontDict();fd.FontName='FontPixel-FD'+str(i);fd.Private=PrivateDict();fd.Private.defaultWidthX=400;fd.Private.nominalWidthX=0;fd.Private.Subrs=SubrsIndex()
        fd.Private.Subrs.append(T2CharString(program=[100+i*20,0,0,100,-100-i*20,0,'rlineto','return'],private=fd.Private))
        top.FDArray.append(fd)
    top.FDSelect=FDSelect(format=3);top.FDSelect.gidArray=[0,0,1]
    old=top.CharStrings;old.charStrings={n:i for i,n in enumerate(order)};old.charStringsIndex.items=[]
    for i in range(3):
        fd=top.FDArray[top.FDSelect.gidArray[i]]
        old.charStringsIndex.items.append(T2CharString(program=([700] if i==0 else [])+[0,0,'rmoveto',-107,'callsubr','endchar'],private=fd.Private,globalSubrs=cff.GlobalSubrs))
    old.fdArray=top.FDArray;old.fdSelect=top.FDSelect;old.charStringsIndex.fdArray=top.FDArray;old.charStringsIndex.fdSelect=top.FDSelect
    if hasattr(top,'Private'):del top.Private
    if hasattr(top,'Encoding'):del top.Encoding
    font.setGlyphOrder(order);font['hmtx'].metrics={order[i]:v for i,v in enumerate(font['hmtx'].metrics.values())};font['cmap'].tables[0].cmap={65:'cid00001',66:'cid00002'}
    for table in font['cmap'].tables:table.cmap={65:'cid00001',66:'cid00002'}
    p=tmp/'external-cid.otf';font.save(p);font=TTFont(p);inspect(binary,p,font)
    font['CFF '].cff.topDictIndex[0].FDSelect.format=0
    p=tmp/'external-cid-format0.otf';font.save(p);zero=TTFont(p);assert zero['CFF '].cff.topDictIndex[0].FDSelect.format==0;inspect(binary,p,zero)
    return [{'case':'external-FontMatrix-scale','glyphs':3,'fontTools_and_native':'pass'}, {'case':'external-CID-format0','glyphs':3,'fontTools_and_native':'pass'}, {'case':'external-local-hints-flex-widths','glyphs':3,'fontTools_and_native':'pass'}, {'case':'external-CID-two-FDs','glyphs':3,'fontTools_and_native':'pass'}]

def main():
    p=argparse.ArgumentParser();p.add_argument('--package',type=Path,default=ROOT.parent);p.add_argument('--binary',type=Path,default=ROOT/'target/release/examples/cff_check');p.add_argument('--output',type=Path);p.add_argument('--cjk-count',type=int,default=1024);p.add_argument('--font');p.add_argument('--seeds',type=Path);a=p.parse_args()
    ref=load_reference(a.package);results=[]
    with tempfile.TemporaryDirectory(prefix='fontpixel-cff-') as temp:
        tmp=Path(temp)
        for path in sorted((a.package/'samples/tong-pixel-sans').glob('*.bdf')):
            if a.font and path.name!=a.font:continue
            source=parse_bdf(path);cps=sorted(source)
            if 'Latin' not in path.name and a.cjk_count<len(cps):cps=sorted({cps[i*(len(cps)-1)//(a.cjk_count-1)] for i in range(a.cjk_count)})
            records={ref.gname(cp):source[cp] for cp in cps};order=['.notdef']+list(records);cmap={cp:ref.gname(cp) for cp in cps}
            size='Small' if 'Small' in path.name else 'Large';spacing='mono' if 'Mono' in path.name else 'prop';ref.set_size(size)
            for sh in ['square','dot']:
                font=lambda:ref.build_otf(spacing,sh,'test',(order,records,cmap,{}),size);label=path.stem+'-'+sh
                results.append(verify(a.binary,font,records,sh,tmp,label));print(label,'PASS',len(order),flush=True)
        ref.set_size('Large')
        records={'A':{'rows':['###','#.#','###'],'adv':4,'x':-1,'y':-2},'B':{'rows':['#..','.#.','..#'],'adv':4,'x':0,'y':0}}
        font=ref.build_otf('prop','square','test',(['.notdef','A','B'],records,{65:'A',66:'B'},{}),'Large')
        top=font['CFF '].cff.topDictIndex[0]
        for n,g in records.items():
            pen=T2CharStringPen(g['adv']*100,None)
            for x,y in ref.dots(g):pen.moveTo((x+5,y+5));pen.lineTo((x+5,y+95));pen.lineTo((x+95,y+95));pen.lineTo((x+95,y+5));pen.closePath()
            top.CharStrings[n]=pen.getCharString(private=top.Private)
        results.append(verify(a.binary,font,records,'separate',tmp,'separate-holes-offsets'))
        font=ref.build_otf('prop','dot','test',(['.notdef','A','B'],records,{65:'A',66:'B'},{}),'Large')
        results.append(verify(a.binary,font,records,'dot',tmp,'dot-holes-offsets'))
        results+=special_cases(a.binary,ref,tmp)
        if a.seeds:
            a.seeds.mkdir(parents=True,exist_ok=True)
            for path in tmp.glob('*.otf'):
                data=TTFont(path).getTableData('CFF ')
                if len(data)<=65536:(a.seeds/(path.stem+'.cff')).write_bytes(data)
    report={'tools':{n:importlib.metadata.version(n) for n in ['fonttools','freetype-py','uharfbuzz','opentype-sanitizer']},'selection':{'font':a.font,'cjk_count':a.cjk_count,'Latin':'all glyphs'},'results':results,'scope':'CFF1 outline equivalence and existing mappings; variants/locl creation, bitmap strikes and WOFF2 remain later work items.'}
    if a.output:a.output.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'cases':len(results),'glyph_checks':sum(r['glyphs'] for r in results)}))
if __name__=='__main__':main()
