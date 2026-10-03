#!/usr/bin/env python3
"""Independent SVG path/image checks against supplied bdfparser and CairoSVG.
Does not copy reference inputs or use fontpixel to generate expected pixels.
"""
import argparse, hashlib, io, json, subprocess, sys, warnings, xml.etree.ElementTree as ET
from pathlib import Path
sys.dont_write_bytecode=True
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT.parent/'reference/bdfparser/src'))
from bdfparser import Font
from PIL import Image,ImageDraw
import cairosvg
from fontTools.svgLib.path import parse_path
from fontTools.pens.recordingPen import RecordingPen
FP='{https://fontpixel.org/ns/svg/1}'
SVG='{http://www.w3.org/2000/svg}'
def run(*args):
    p=subprocess.run(list(map(str,args)),capture_output=True)
    if p.returncode:raise RuntimeError((args,p.stderr.decode()))
    return p.stdout

def rows(g):
    w,h,_,_=g['bbox'];stride=(w+7)//8
    return [''.join('1' if g['pixels'][y*stride+x//8]&(128>>(x%8)) else '0' for x in range(w)) for y in range(h)]

def render_path(d,b,ppu=100):
    w,h,x,y=b
    if not w or not h:return []
    # Cairo independently parses and fills SVG; our reader is not used here.
    xml=f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}"><g transform="matrix({1/ppu} 0 0 {-1/ppu} {-x} {h+y})"><path d="{d}"/></g></svg>'
    png=cairosvg.svg2png(bytestring=xml.encode())
    im=Image.open(io.BytesIO(png)).convert('RGBA')
    return [''.join('1' if im.getpixel((x,y))[3]>=128 else '0' for x in range(w)) for y in range(h)]

def main():
    ap=argparse.ArgumentParser();ap.add_argument('--binary',required=True);ap.add_argument('--driver',required=True);ap.add_argument('--work',type=Path,required=True);ap.add_argument('--output',type=Path,required=True);a=ap.parse_args();a.work.mkdir(parents=True,exist_ok=True)
    warnings.simplefilter('ignore');results=[];path_count=0;raster_count=0
    sources=[ROOT.parent/'samples/tong-pixel-sans/TongPixelSans14Large-Latin.bdf',ROOT.parent/'samples/tong-pixel-sans/TongPixelSans14Large-SC.bdf']
    for source in sources:
        f=Font(source)
        for shape in ['square','square-separate','dot']:
            output=a.work/f'{source.stem}-{shape}.svg';dump=output.with_suffix('.json')
            run(a.binary,'convert',source,'-o',output,'--shape',shape,'--dotsize','1' if shape=='square' else '0.9')
            run(a.driver,output,dump,14,256*1024*1024)
            native=json.loads(dump.read_text())['fonts'][0];tree=ET.parse(output);glyphs=tree.findall(f'.//{SVG}glyph')
            byid={g.attrib[FP+'id']:g for g in glyphs}
            assert len(native['glyphs'])==len(f.glyphs)
            for ng,(cp,raw) in zip(native['glyphs'],f.glyphs.items()):
                ref=f.glyphbycp(cp);expected=ref.draw(1).bindata
                assert ng['bbox']==raw[2:6],(source,shape,cp,'bbox',ng['bbox'],raw[2:6])
                assert rows(ng)==expected,(source,shape,cp,'native pixels')
                assert ng['advance']==raw[8:10],(source,shape,cp,'advance',ng['advance'],raw[8:10])
            # All native visible paths are independently parsed, not a sample.
            for node in byid.values():
                pen=RecordingPen();parse_path(node.attrib.get('d',''),pen);path_count+=1
                b=list(map(int,node.attrib[FP+'bounds'].split()))
                expected=f.glyphbycp(ord(node.attrib["unicode"][0])).draw(1).bindata if 'unicode' in node.attrib and len(node.attrib['unicode'])==1 else None
                if expected is not None:
                    got=render_path(node.attrib.get('d',''),b)
                    assert got==expected,(source.name,shape,node.attrib.get('unicode'),'Cairo pixels')
                    raster_count+=1
            results.append(dict(source=source.name,shape=shape,glyphs=len(native['glyphs']),sha256=hashlib.sha256(output.read_bytes()).hexdigest()))
            print(f'PASS {source.name} {shape}: {len(native["glyphs"])} native glyphs and independent visible paths',flush=True)
            (a.work/'completed-cases.json').write_text(json.dumps(results,indent=2)+'\n')
    # Independent producer: supplied Python reference SVG font, including its
    # historical cell-based origin and synthetic .notdef. Compare to Cairo paths.
    f=Font(sources[0]);reference=a.work/'python-reference.svg';reference.write_text(f.tosvgfont())
    reference_dump=a.work/'python-reference.json';ppem=max(f.headers['fbbx'],f.headers['fbby'])
    run(a.driver,reference,reference_dump,ppem)
    decoded=json.loads(reference_dump.read_text())['fonts'][0]['glyphs'];nodes=ET.parse(reference).findall(f'.//{SVG}font/*')
    nodes=[n for n in nodes if n.tag in [SVG+'glyph',SVG+'missing-glyph']]
    assert len(decoded)==len(nodes)
    for g,n in zip(decoded,nodes):
        assert rows(g)==render_path(n.attrib.get('d',''),g['bbox'],1)
    # Generic cubic/quadratic/relative/rotated-arc paths: independent supersampled
    # Cairo pixel-center readings in a fixed padded canvas, then trim to actual ink.
    curve_paths=[
        'M0 0C100 600 700 -300 800 400L900-200Z',
        'M0 0Q123 587 811 13T1207 413L900-203Z',
        'M13 107A207 113 31 1 0 613 417a203 97 17 0 1-401-211Z',
        'm-103-97 311 0 0 307-311 0z m607 3 199 0 0 193-199 0z',
        'M17 19C111 517 309 719 417 111S811-197 1003 311L707-113Z',
    ]
    for i,path in enumerate(curve_paths):
        p=a.work/f'curve-{i}.svg';j=p.with_suffix('.json')
        p.write_text(f'<svg><font horiz-adv-x="1400"><font-face units-per-em="1600"/><glyph d="{path}"/></font></svg>')
        run(a.driver,p,j,16)
        actual=json.loads(j.read_text())['fonts'][0]['glyphs'][0]
        factor=64;left,bottom=-4,-5;w,h=22,20
        doc=f'<svg xmlns="http://www.w3.org/2000/svg" width="{w*factor}" height="{h*factor}" viewBox="{-0.5/factor} {-0.5/factor} {w} {h}"><g transform="matrix(.01 0 0 -.01 {-left} {h+bottom})"><path d="{path}"/></g></svg>'
        im=Image.open(io.BytesIO(cairosvg.svg2png(bytestring=doc.encode()))).convert('RGBA')
        points=[(x,y) for y in range(h) for x in range(w) if im.getpixel((x*factor+factor//2,y*factor+factor//2))[3]>=128]
        if points:
            xmin=min(x for x,y in points);xmax=max(x for x,y in points);ymin=min(y for x,y in points);ymax=max(y for x,y in points)
            expected=[''.join('1' if (x,y) in points else '0' for x in range(xmin,xmax+1)) for y in range(ymin,ymax+1)]
            bbox=[xmax-xmin+1,ymax-ymin+1,xmin+left,bottom+h-1-ymax]
        else:expected=[];bbox=[0,0,0,0]
        assert actual['bbox']==bbox and rows(actual)==expected,('generic curve',i,actual['bbox'],bbox)
    # Actual image visual proofs: reference and native square plus true circle.
    sc=Font(sources[1]);text='Hello, 世界! 永 0123';bitmap=sc.draw(text);panels=[]
    for shape in ['square','dot']:
        output=a.work/f'visual-{shape}.svg'
        run(a.binary,'render',sources[1],text,'-o',output,'--scale','5','--shape',shape,'--color','#174a7e','--background','#faf7ef')
        png=a.work/f'visual-{shape}.png';cairosvg.svg2png(url=str(output),write_to=str(png));panels.append(Image.open(png).convert('RGB'))
    refpng=a.work/'visual-reference.png'
    reference_image=Image.new('RGB',(bitmap.width(),bitmap.height()));reference_image.putdata([(23,74,126) if c=='1' else (250,247,239) for row in bitmap.bindata for c in row]);reference_image=reference_image.resize(panels[0].size,Image.Resampling.NEAREST);reference_image.save(refpng)
    assert reference_image.tobytes()==panels[0].tobytes()
    allpanels=[reference_image,*panels];sheet=Image.new('RGB',(max(p.width for p in allpanels)+24,sum(p.height+36 for p in allpanels)+12),'white');draw=ImageDraw.Draw(sheet);y=8
    for label,p in zip(['Supplied Python reference','Native SVG square (exact pixels)','Native SVG true circular dots'],allpanels):draw.text((12,y),label,fill='black');sheet.paste(p,(12,y+22));y+=p.height+36
    sheet.save(a.work/'visual-proof.png')
    result=dict(cairosvg=cairosvg.__version__,cases=results,native_glyph_comparisons=sum(c['glyphs'] for c in results),fonttools_path_parses=path_count,cairo_glyph_rasters=raster_count,independent_python_font_glyphs=len(decoded),visual=str(a.work/'visual-proof.png'),generic_curve_cases=len(curve_paths),failures=0)
    a.output.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
if __name__=='__main__':main()
