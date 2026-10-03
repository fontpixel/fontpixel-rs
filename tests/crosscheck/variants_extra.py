#!/usr/bin/env python3
"""External CFF/GSUB, bitmap, metric, strictness and CLI failure checks for W2.6."""
import argparse,io,json,subprocess,logging
from pathlib import Path
import freetype,ots
from fontTools.ttLib import TTFont
from fontTools.ttLib.sfnt import SFNTWriter
from fontTools.ttLib.tables import otTables
from variants import invoke,write_bdf,compare_subset,sfnt_bytes
from glyf import bitmap
ROOT=Path(__file__).resolve().parents[2]
def assemble(template,replacements):
 f=TTFont(template);out=io.BytesIO();tags=sorted(set(f.reader.keys())|set(replacements));w=SFNTWriter(out,len(tags),sfntVersion=f.sfntVersion)
 for tag in tags:w[tag]=replacements.get(tag,f.reader[tag]if tag in f.reader else b'')
 w.close();return out.getvalue()
def main():
 p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--work',type=Path,required=True);p.add_argument('--full-work',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.work.mkdir(parents=True,exist_ok=True);results=[]
 logging.getLogger('fontTools.ttLib.tables._h_e_a_d').setLevel(logging.ERROR)
 tiny=a.work/'tiny.bdf';write_bdf(tiny,{65:{'rows':['#'],'adv':4,'x':0,'y':0},66:{'rows':['##'],'adv':4,'x':0,'y':0}})
 template=a.work/'tiny.otf';invoke(a.binary,'variants','--base',tiny,'-o',template)
 for raw in sorted((ROOT/'tests/data/cff').glob('external-*.cff')):
  source=a.work/(raw.stem+'.otf');source.write_bytes(assemble(template,{'CFF ':raw.read_bytes()}));sub=a.work/(raw.stem+'-subset.otf');invoke(a.binary,'subset',source,'--text','B','-o',sub)
  one,two=TTFont(source),TTFont(sub);one.ensureDecompiled();two.ensureDecompiled();assert two['maxp'].numGlyphs==2
  assert one.reader['name']==two.reader['name'];f,g=freetype.Face(str(source)),freetype.Face(str(sub))
  for old,new in [(0,0),(2,1)]:
   assert bitmap(f,old)==bitmap(g,new),(raw.name,'unhinted pixels')
   for ppem in [9,14,19]:
    for face,gid in [(f,old),(g,new)]:face.set_pixel_sizes(0,ppem);face.load_glyph(gid,freetype.FT_LOAD_RENDER|freetype.FT_LOAD_TARGET_MONO)
    assert (f.glyph.bitmap.width,f.glyph.bitmap.rows,bytes(f.glyph.bitmap.buffer),f.glyph.advance.x)==(g.glyph.bitmap.width,g.glyph.bitmap.rows,bytes(g.glyph.bitmap.buffer),g.glyph.advance.x),(raw.name,'hinted pixels',ppem)
  status=ots.sanitize(str(sub),str(sub)+'.ots',capture_output=True);assert status.returncode==0,(raw.name,status.stderr)
  results.append({'case':raw.stem,'raw_CFF_preservation_hinted_and_unhinted_FreeType':'pass','OTS':'pass'})
 # Native reader handles independently compiled FEA GSUB rather than only its own extension encoding.
 for kind in ['ttf','otf']:
  source=a.full_work/('full-square-'+kind+'-reference.'+kind)
  cmap=sorted(TTFont(source).getBestCmap());cps=sorted(set(cmap[::2000])|{65,0x201c,0x201d,0x2026})
  results.append(compare_subset(a.binary,source,a.work,'external-GSUB-'+kind,cps))
 # Embed the same source strike and remap it together with outline IDs.
 native=a.work/'embedded.ttf';invoke(a.binary,'variants','--base',tiny,'--embed-bitmaps','--shape','dot','-o',native)
 sub=a.work/'embedded-subset.ttf';invoke(a.binary,'subset',native,'--unicodes','U+0042','-o',sub)
 f,g=freetype.Face(str(native)),freetype.Face(str(sub));f.select_size(0);g.select_size(0)
 for old,new in [(0,0),(2,1)]:
  f.load_glyph(old,freetype.FT_LOAD_SBITS_ONLY|freetype.FT_LOAD_RENDER);g.load_glyph(new,freetype.FT_LOAD_SBITS_ONLY|freetype.FT_LOAD_RENDER)
  assert (f.glyph.bitmap.width,f.glyph.bitmap.rows,bytes(f.glyph.bitmap.buffer))==(g.glyph.bitmap.width,g.glyph.bitmap.rows,bytes(g.glyph.bitmap.buffer))
 results.append({'case':'embedded-dot-dependency','embedded_FreeType':'pass'})
 # Safe refusal and explicit strict diagnostics leave an existing destination intact.
 unsupported=a.work/'unknown-table.otf';unsupported.write_bytes(assemble(template,{'TEST':b'opaque glyph IDs'}));dest=a.work/'sentinel.otf';dest.write_bytes(b'sentinel')
 for flags,code in [([],1),(['--drop-table','TEST','--strict'],3)]:
  p=subprocess.run([str(a.binary),'subset',str(unsupported),'--text','A','-o',str(dest),'--report','json',*flags],capture_output=True,text=True);assert p.returncode==code,(p.stdout,p.stderr);assert dest.read_bytes()==b'sentinel';json.loads(p.stderr)
 out=json.loads(invoke(a.binary,'subset',unsupported,'--text','A','-o',dest,'--drop-table','TEST','--report','json'));assert any(e['code']=='subset.table_dropped'for e in out['report']['entries'])
 results.append({'case':'explicit-loss-strict-atomicity','result':'pass'})
 a.output.write_text(json.dumps({'partial':False,'results':results},indent=2)+'\n');print('EXTRA PASS',len(results))
if __name__=='__main__':main()
