#!/usr/bin/env python3
"""CLI preview / PNG / independently rasterized SVG versus Python glyph pixels.
Needs Pillow and CairoSVG (test-only tools, not redistributed/runtime deps).
"""
import argparse,hashlib,itertools,json,subprocess,sys,tempfile,warnings
from pathlib import Path
sys.dont_write_bytecode=True
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT.parent/'reference/bdfparser/src'))
from bdfparser import Font
from PIL import Image
import cairosvg

def main():
    p=argparse.ArgumentParser();p.add_argument('--binary',default=str(ROOT/'target/release/fontpixel'));p.add_argument('--output',type=Path);a=p.parse_args()
    warnings.simplefilter('ignore')
    source=ROOT.parent/'samples/tong-pixel-sans/TongPixelSans14Large-SC.bdf'
    text='Hello, 世界! 永';f=Font(source)
    colors=[('#174a7e','#faf7ef',(23,74,126,255),(250,247,239,255)),('#00ff0080','transparent',(0,255,0,128),(0,0,0,0)),('#ffffff80','#00000040',(255,255,255,128),(0,0,0,64))]
    results=[]
    with tempfile.TemporaryDirectory(prefix='fontpixel-render-images-') as directory:
        d=Path(directory)
        for direction,scale,color in itertools.product(['lrtb','rltb','lrbt','rlbt','tbrl','tblr','btrl','btlr'],[1,3],colors):
            fg,bg,ink,paper=color
            expected=f.draw(text,direction=direction,linelimit=40)
            base=[str(source),text,'--direction',direction,'--line-limit','40']
            preview=subprocess.run([a.binary,'preview',*base,'--chars','10'],capture_output=True,check=True)
            assert preview.stdout.decode()=='\n'.join(expected.bindata)+'\n'
            expected_image=Image.new('RGBA',(expected.width(),expected.height()))
            expected_image.putdata([ink if c=='1' else paper for row in expected.bindata for c in row])
            expected_image=expected_image.resize((expected.width()*scale,expected.height()*scale),Image.Resampling.NEAREST)
            for kind in ['png','svg']:
                subprocess.run([a.binary,'render',*base,'-o',str(d/f'image.{kind}'),'--scale',str(scale),'--color',fg,'--background',bg],capture_output=True,check=True)
            png=Image.open(d/'image.png').convert('RGBA');assert png.tobytes()==expected_image.tobytes()
            cairosvg.svg2png(url=str(d/'image.svg'),write_to=str(d/'svg.png'))
            svg=Image.open(d/'svg.png').convert('RGBA')
            assert png.size==svg.size and png.tobytes()==svg.tobytes(),(direction,scale,fg,bg)
            results.append(dict(direction=direction,scale=scale,foreground=fg,background=bg,width=png.width,height=png.height,pixels_sha256=hashlib.sha256(png.tobytes()).hexdigest()))
        # Same glyphs through native PCF and gzip transports, not just BDF parsing.
        for ext in ['pcf','pcf.gz','bdf.gz']:
            target=d/f'font.{ext}'
            subprocess.run([a.binary,'convert',str(source),'-o',str(target)],capture_output=True,check=True)
            actual=subprocess.run([a.binary,'preview',str(target),text,'--chars','10'],capture_output=True,check=True)
            assert actual.stdout.decode()=='\n'.join(f.draw(text).bindata)+'\n'
    result=dict(cairosvg=cairosvg.__version__,cases=len(results),pixel_mismatches=0,transport_checks=['pcf','pcf.gz','bdf.gz'],results=results)
    if a.output:a.output.write_text(json.dumps(result,indent=2)+'\n')
    print(f'PASS {len(results)} preview/PNG/SVG image combinations, 3 PCF/gzip checks')
if __name__=='__main__':main()
