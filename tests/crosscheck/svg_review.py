#!/usr/bin/env python3
"""Independent Cairo regression oracle for separately painted SVG glyph paths."""
import argparse, io, json, subprocess
from pathlib import Path
import cairosvg
from PIL import Image
from fontTools.svgLib.path import parse_path
from fontTools.pens.recordingPen import RecordingPen

def rows(g):
    w,h,_,_=g['bbox'];stride=(w+7)//8
    return [''.join('1' if g['pixels'][y*stride+x//8] & (128>>(x%8)) else '0' for x in range(w)) for y in range(h)]

def main():
    p=argparse.ArgumentParser();p.add_argument('--driver',required=True);p.add_argument('--work',required=True,type=Path);p.add_argument('--output',required=True,type=Path);a=p.parse_args();a.work.mkdir(parents=True,exist_ok=True)
    # Avoid exact-center diagonal boundaries: Cairo antialias tie-breaking is
    # distinct from the documented native half-open crossing convention.
    cases=[
        ['M0 0h300v300H0Z','M0 0v300h300V0Z'],
        ['M0 0h300v300H0Z','M100 100v300h300V100Z'],
        ['M0 0h400v400H0Z M100 100V300H300V100Z','M100 100h200v200H100Z'],
        ['M100 100h100v100H100z L400 100 L400 400'],
        ['M0 0Q100 400 400 0Z','M107 -7L307 313L417 -7Z'],
        ['m-100 -100h200v200h-200z','M200 100h100v100H200Z'],
    ];results=[]
    for index,paths in enumerate(cases):
        for path in paths:parse_path(path,RecordingPen())
        content=''.join(f'<path d="{path}"/>' for path in paths)
        font=a.work/f'paint-{index}.svg';dump=font.with_suffix('.json')
        font.write_text(f'<svg><font><font-face units-per-em="800"/><glyph>{content}</glyph></font></svg>')
        subprocess.run([a.driver,str(font),str(dump),'8'],check=True)
        g=json.loads(dump.read_text())['fonts'][0]['glyphs'][0]
        factor=64;left,bottom=-3,-3;w,h=12,12
        doc=f'<svg xmlns="http://www.w3.org/2000/svg" width="{w*factor}" height="{h*factor}" viewBox="{-0.5/factor} {-0.5/factor} {w} {h}"><g transform="matrix(.01 0 0 -.01 {-left} {h+bottom})">{content}</g></svg>'
        im=Image.open(io.BytesIO(cairosvg.svg2png(bytestring=doc.encode()))).convert('RGBA')
        points={(x,y) for y in range(h) for x in range(w) if im.getpixel((x*factor+factor//2,y*factor+factor//2))[3]>=128}
        if points:
            xmin=min(x for x,y in points);xmax=max(x for x,y in points);ymin=min(y for x,y in points);ymax=max(y for x,y in points)
            expected=[''.join('1' if (x,y) in points else '0' for x in range(xmin,xmax+1)) for y in range(ymin,ymax+1)]
            bbox=[xmax-xmin+1,ymax-ymin+1,xmin+left,bottom+h-1-ymax]
        else:expected=[];bbox=[0,0,0,0]
        assert g['bbox']==bbox and rows(g)==expected,(index,g['bbox'],bbox,rows(g),expected)
        results.append({'case':index,'paths':len(paths),'bbox':bbox,'exact':True})
    out={'cairosvg':cairosvg.__version__,'cases':results,'failures':0};a.output.write_text(json.dumps(out,indent=2)+'\n');print(json.dumps(out,indent=2))
if __name__=='__main__':main()
