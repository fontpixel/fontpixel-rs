#!/usr/bin/env python3
"""W2.2 native outlines vs frozen Tong/fontTools, OTS, FreeType and HarfBuzz.

Uses all 3 Latin BDF samples and deterministic 1024-glyph selections from each
of the four regional samples. No protected reference/sample files are modified.
Full locl/variants family assembly remains W2.6, not a claimed W2.2 capability.
"""
import argparse, hashlib, importlib.util, importlib.metadata, json, subprocess, sys, tempfile
from pathlib import Path
import freetype, uharfbuzz as hb, ots
from fontTools.ttLib import TTFont
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib.tables._g_l_y_f import Glyph, GlyphComponent
ROOT=Path(__file__).resolve().parents[2]

def load_reference(package):
    sys.dont_write_bytecode=True
    path=package/'reference/tong-pixel-sans/tools/export-rs/python_reference.py'
    spec=importlib.util.spec_from_file_location('tong_reference',path)
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module

def parse_bdf(path):
    records={};g=None;bitmap=False
    for line in path.read_text().splitlines():
        key,_,value=line.partition(' ')
        if key=='STARTCHAR':g={};bitmap=False
        elif key=='ENCODING':g['cp']=int(value)
        elif key=='DWIDTH':g['adv']=int(value.split()[0])
        elif key=='BBX':g['w'],g['h'],g['x'],g['y']=map(int,value.split())
        elif key=='BITMAP':bitmap=True;g['rows']=[]
        elif key=='ENDCHAR':
            if g['cp']>=0:records[g['cp']]={k:g[k] for k in ('adv','x','y','rows')}
            g=None;bitmap=False
        elif bitmap:
            bits=bin(int(line,16))[2:].zfill(len(line)*4)[:g['w']]
            g['rows'].append(''.join('#' if c=='1' else '.' for c in bits))
    return records

def bitmap(face,gid):
    face.set_pixel_sizes(0,14)
    face.load_glyph(gid,freetype.FT_LOAD_RENDER|freetype.FT_LOAD_TARGET_MONO|freetype.FT_LOAD_NO_HINTING)
    s=face.glyph;b=s.bitmap;data=bytes(b.buffer)
    pixels=tuple(tuple((data[y*abs(b.pitch)+x//8]>>(7-x%8))&1 for x in range(b.width)) for y in range(b.rows))
    return b.width,b.rows,s.bitmap_left,s.bitmap_top,s.advance.x,pixels

def expected_ink(g):
    pixels={(g['x']+x,g['y']+len(g['rows'])-y-1) for y,r in enumerate(g['rows']) for x,v in enumerate(r) if v=='#'}
    return pixels

def actual_ink(b):
    w,h,x,top,adv,rows=b
    return {(x+xx,top-yy-1) for yy,row in enumerate(rows) for xx,v in enumerate(row) if v}

def shape(font,text,language):
    f=hb.Font(hb.Face(font));f.scale=(1400,1400);buf=hb.Buffer();buf.add_str(text);buf.guess_segment_properties();buf.language=language;hb.shape(f,buf)
    return [(g.codepoint,p.x_advance,p.y_advance,p.x_offset,p.y_offset) for g,p in zip(buf.glyph_infos,buf.glyph_positions)]

def canon(contours):
    # Preserve winding and curve markers; only rotate starts and sort contours.
    out=[]
    for c in contours:
        c=[tuple(p) for p in c]
        if c:out.append(min(tuple(c[i:]+c[:i]) for i in range(len(c))))
    return sorted(out)

def fonttools_contours(font):
    gs=font['glyf']
    for name in font.getGlyphOrder():
        coords,ends,flags=gs[name].getCoordinates(gs);start=0;cs=[]
        for end in ends:
            cs.append([(float(coords[i][0]),float(coords[i][1]),bool(flags[i]&1)) for i in range(start,end+1)]);start=end+1
        yield cs

def inspect(binary,path):
    return json.loads(subprocess.check_output([str(binary),'--inspect',str(path)],text=True))

def verify_case(binary,reference,records,shape_name,tmp,label):
    rp=tmp/(label+'-reference.ttf');out=tmp/(label+'-native.ttf');jp=tmp/(label+'.json');log=tmp/(label+'-result.json')
    reference.save(rp);order=reference.getGlyphOrder();jp.write_text(json.dumps({'glyphs':[records.get(n) for n in order],'dot_id':order.index('dot') if 'dot' in order else None}))
    subprocess.run([str(binary),'--generate',str(rp),str(jp),shape_name,str(out),str(log)],check=True)
    actual=TTFont(out,checkChecksums=2);actual.ensureDecompiled();actual.saveXML(tmp/(label+'.ttx'))
    assert actual.getGlyphOrder()==TTFont(rp).getGlyphOrder();assert actual.getBestCmap()==TTFont(rp).getBestCmap()
    for tag in ['hmtx','hhea','OS/2','name','post','cmap','GSUB']:
        if tag in reference:assert actual.getTableData(tag)==reference.getTableData(tag),(label,tag)
    expected=fonttools_contours(reference);got=fonttools_contours(actual)
    for i,(a,b) in enumerate(zip(got,expected)):assert canon(a)==canon(b),(label,order[i],'outline mismatch')
    process=subprocess.Popen([str(binary),'--inspect-stream',str(out)],stdout=subprocess.PIPE,text=True)
    for i,expected in enumerate(fonttools_contours(actual)):
        line=process.stdout.readline();assert line,(label,'missing streamed glyph',i)
        entry=json.loads(line);assert entry['gid']==i
        assert canon(entry['contours'])==canon(expected),(label,i,'native decode mismatch')
        if shape_name=='dot' and order[i] in records:
            assert entry['components']==sum(r.count('#') for r in records[order[i]]['rows'])
    assert not process.stdout.read(),(label,'extra streamed glyphs')
    assert process.wait()==0
    sanitized=tmp/(label+'-ots.ttf');status=ots.sanitize(str(out),str(sanitized),capture_output=True)
    assert status.returncode==0,status.stderr.decode()+status.stdout.decode()
    ft=freetype.Face(str(out));rf=freetype.Face(str(rp));sf=freetype.Face(str(sanitized))
    for i,name in enumerate(order):
        b=bitmap(ft,i);assert bitmap(sf,i)==b,(label,name,'OTS pixel change')
        assert b==bitmap(rf,i),(label,name,'FreeType reference mismatch')
        if name in records:
            assert actual_ink(b)==expected_ink(records[name]),(label,name,'source pixel mismatch')
            assert b[4]==records[name]['adv']*64,(label,name,'advance')
    text=''.join(chr(c) for c in list(reference.getBestCmap())[:200])
    for language in ['zh-Hans','zh-Hant','zh-HK','ja','ko','en']:
        assert shape(out.read_bytes(),text,language)==shape(rp.read_bytes(),text,language)
    repacked=tmp/(label+'-repack.ttf');subprocess.run([str(binary),'--repack',str(rp),str(repacked)],check=True)
    again=TTFont(repacked);again.ensureDecompiled()
    for a,b in zip(fonttools_contours(again),fonttools_contours(reference)):assert canon(a)==canon(b)
    return {'case':label,'glyphs':len(order),'sha256':hashlib.sha256(out.read_bytes()).hexdigest(),'bytes':out.stat().st_size,
        'fontTools':'pass','OTS':'pass','FreeType_reference_and_source':'pass','HarfBuzz_6_languages':'pass','read_external_reencode':'pass'}

def main():
    p=argparse.ArgumentParser();p.add_argument('--package',type=Path,default=ROOT.parent);p.add_argument('--binary',type=Path,default=ROOT/'target/release/examples/glyf_check');p.add_argument('--output',type=Path);p.add_argument('--cjk-count',type=int,default=1024);p.add_argument('--font',help='Optional exact sample filename');a=p.parse_args()
    ref=load_reference(a.package);results=[]
    with tempfile.TemporaryDirectory(prefix='fontpixel-glyf-') as temp:
        tmp=Path(temp)
        for path in sorted((a.package/'samples/tong-pixel-sans').glob('*.bdf')):
            if a.font and path.name!=a.font:continue
            source=parse_bdf(path);cps=sorted(source)
            if 'Latin' not in path.name:
                cps=sorted({cps[i*(len(cps)-1)//(a.cjk_count-1)] for i in range(a.cjk_count)})
            records={ref.gname(cp):source[cp] for cp in cps};order=['.notdef']+list(records);cmap={cp:ref.gname(cp) for cp in cps}
            size='Small' if 'Small' in path.name else 'Large';spacing='mono' if 'Mono' in path.name else 'prop';ref.set_size(size)
            for shape_name in ['square','dot']:
                font=ref.build_ttf(spacing,shape_name,'test',(order,records,cmap,{}),size)
                label=path.stem+'-'+shape_name;results.append(verify_case(a.binary,font,records,shape_name,tmp,label));print(label,'PASS',len(order),flush=True)
        # Independent-square geometry, holes/corner-touching pixels and offsets.
        records={'A':{'rows':['###','#.#','###'],'x':-1,'y':-2,'adv':4},'B':{'rows':['#..','.#.','..#'],'x':0,'y':0,'adv':3},'space':{'rows':[],'x':0,'y':0,'adv':3}}
        font=ref.build_ttf('prop','square','test',(['.notdef']+list(records),records,{65:'A',66:'B',32:'space'},{}),'Large')
        for name,g in records.items():
            pen=TTGlyphPen(None)
            for x,y in ref.dots(g):
                pen.moveTo((x+5,y+5));pen.lineTo((x+5,y+95));pen.lineTo((x+95,y+95));pen.lineTo((x+95,y+5));pen.closePath()
            font['glyf'][name]=pen.glyph()
        results.append(verify_case(a.binary,font,records,'separate',tmp,'separate-holes-offsets'))
        # FontTools-authored transforms, nested composites, and point attachments.
        font=ref.build_ttf('prop','square','test',(['.notdef']+list(records),records,{65:'A',66:'B',32:'space'},{}),'Large')
        g=Glyph();g.numberOfContours=-1;c=GlyphComponent();c.glyphName='A';c.x=125;c.y=-75;c.transform=[[0.5,0.25],[-0.25,1.0]];c.flags=0x800
        d=GlyphComponent();d.glyphName='A';d.firstPt=2;d.secondPt=0;d.flags=0;g.components=[c,d];font['glyf']['B']=g
        g=Glyph();g.numberOfContours=-1;c=GlyphComponent();c.glyphName='B';c.x=-50;c.y=50;c.flags=0x1000;g.components=[c];font['glyf']['space']=g
        rp=tmp/'transforms.ttf';op=tmp/'transforms-native.ttf';font.save(rp);subprocess.run([str(a.binary),'--repack',str(rp),str(op)],check=True)
        got=inspect(a.binary,op)['contours'];expected=fonttools_contours(TTFont(rp))
        for c,d in zip(got,expected):assert canon(c)==canon(d),('affine/point/nested',c,d)
        f=freetype.Face(str(rp));n=freetype.Face(str(op))
        for i in range(4):assert bitmap(f,i)==bitmap(n,i)
        results.append({'case':'affine-point-nested','glyphs':4,'fontTools':'pass','FreeType':'pass'})
    output={'tools':{name:importlib.metadata.version(name) for name in ['fonttools','freetype-py','uharfbuzz','opentype-sanitizer']},'selection':{'font_filter':a.font,'Latin':'all selected Latin samples, all glyphs','regional':f'{a.cjk_count} evenly selected glyphs per selected regional sample'},'results':results,
        'scope':'TTF outline glyph-level equivalence; native variants/locl creation and other sfnt formats remain their later work items'}
    if a.output:a.output.write_text(json.dumps(output,indent=2)+'\n')
    print(json.dumps({'cases':len(results),'generated_glyph_checks':sum(r['glyphs'] for r in results)},sort_keys=True))
if __name__=='__main__':main()
