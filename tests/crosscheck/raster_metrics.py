#!/usr/bin/env python3
"""Independent phantom metrics fixtures, including scaled USE_MY_METRICS.

The expected result is obtained from deployed FreeType 2.13.3 without hinting.
All generated sources are project-owned fixtures; no reference font is edited.
"""
import argparse,json
from pathlib import Path
import freetype
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib.tables._g_l_y_f import Glyph,GlyphComponent
from raster_collections import call,pixels,ft_ink

def main():
 p=argparse.ArgumentParser();p.add_argument('--driver',type=Path,required=True);p.add_argument('--work',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.work.mkdir(parents=True,exist_ok=True);results=[]
 for use in [False,True]:
  for offset in [0,100,200]:
   for scale in [.5,1.,1.5]:
    pen=TTGlyphPen(None);pen.moveTo((100,0));pen.lineTo((100,100));pen.lineTo((200,100));pen.lineTo((200,0));pen.closePath();base=pen.glyph()
    composite=Glyph();composite.numberOfContours=-1;c=GlyphComponent();c.glyphName='A';c.x=offset;c.y=0;c.flags=0x200 if use else 0;c.transform=[[scale,0],[0,scale]];composite.components=[c]
    fb=FontBuilder(800,isTTF=True);fb.setupGlyphOrder(['.notdef','A','B']);fb.setupCharacterMap({65:'A',66:'B'});fb.setupGlyf({'.notdef':Glyph(),'A':base,'B':composite});fb.setupHorizontalMetrics({'.notdef':(0,0),'A':(300,0),'B':(500,0)});fb.setupHorizontalHeader(ascent=700,descent=-100);fb.setupNameTable({'familyName':'Raster metrics test','styleName':'Regular'});fb.setupOS2();fb.setupPost();fb.setupMaxp();source=a.work/'metric.ttf';fb.save(source)
    output=a.work/'metric.json';call(a.driver,'read',source,output,16,'outline');gs=json.loads(output.read_text())['fonts'][0]['glyphs'];f=freetype.Face(str(source));f.set_pixel_sizes(0,16)
    for g in gs:
     ink,advance=ft_ink(f,g['gid']);assert pixels(g)==ink,(use,offset,scale,g['gid'],pixels(g),ink);assert g['advance'][0]==advance
    results.append({'use_my_metrics':use,'translation':offset,'scale':scale,'glyphs':len(gs),'pixels_and_advances_equal':True})
 a.output.write_text(json.dumps({'cases':results,'glyphs':sum(r['glyphs']for r in results)},indent=2)+'\n');print(len(results),'cases PASS')
if __name__=='__main__':main()
