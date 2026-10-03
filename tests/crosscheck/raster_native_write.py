#!/usr/bin/env python3
"""Verify the new native FontSet writer via the actual CLI and external readers."""
import argparse,json,hashlib
from pathlib import Path
import freetype,ots
from fontTools.ttLib import TTFont,TTCollection
from glyf import parse_bdf,expected_ink
from raster_collections import call,ft_ink

def main():
 p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--work',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.work.mkdir(parents=True,exist_ok=True)
 source=Path(__file__).resolve().parents[3]/'samples/tong-pixel-sans/TongPixelSans14Large-Latin.bdf';records=parse_bdf(source);results=[]
 for shape in ['square','dot']:
  for kind in ['ttf','otf','woff','woff2','ttc']:
   path=a.work/f'native-{shape}.{kind}';call(a.binary,'convert',source,'-o',path,'--shape',shape,'--embed-bitmaps','--woff2-quality',1)
   f=TTCollection(path).fonts[0]if kind=='ttc'else TTFont(path);f.ensureDecompiled();ots.sanitize(str(path),check=True,capture_output=True)
   cmap=f.getBestCmap();assert set(cmap)==set(records);face=freetype.Face(str(path));face.set_pixel_sizes(0,14)
   for cp,record in records.items():
    gid=f.getGlyphID(cmap[cp]);ink,advance=ft_ink(face,gid);assert ink==expected_ink(record),(shape,kind,cp);assert advance==record['adv']
    face.load_glyph(gid,freetype.FT_LOAD_RENDER|freetype.FT_LOAD_TARGET_MONO)
    b=face.glyph.bitmap;d=bytes(b.buffer);s=face.glyph
    embedded={(s.bitmap_left+x,s.bitmap_top-y-1)for y in range(b.rows)for x in range(b.width)if(d[y*abs(b.pitch)+x//8]>>(7-x%8))&1}
    assert embedded==expected_ink(record),(shape,kind,cp,'embedded')
   results.append({'shape':shape,'container':kind,'mapped_glyphs':len(records),'outline_and_embedded_pixels_exact':True,'ots':True,'sha256':hashlib.sha256(path.read_bytes()).hexdigest()});print(shape,kind,'PASS',len(records),flush=True)
 a.output.write_text(json.dumps({'cases':results,'glyph_comparisons':sum(r['mapped_glyphs']for r in results)*2},indent=2)+'\n')
if __name__=='__main__':main()
