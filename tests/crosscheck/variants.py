#!/usr/bin/env python3
"""Native variants/subset CLI vs unchanged Tong plan/feature policy and independent engines."""
import argparse,io,json,subprocess,tempfile,hashlib,importlib.metadata,sys
from pathlib import Path
import freetype,uharfbuzz as hb,ots
from fontTools.ttLib import TTFont
from fontTools import subset as ft_subset
from glyf import load_reference,parse_bdf,bitmap,actual_ink,expected_ink
ROOT=Path(__file__).resolve().parents[2]
LANGUAGES=['zh-Hans','zh-Hant','zh-HK','ja','ko','en']
SCRIPTS=['DFLT','hani','kana','hang','bopo','latn','grek','cyrl']
def invoke(binary,*args):
 p=subprocess.run([str(binary),*map(str,args)],capture_output=True,text=True)
 assert p.returncode==0,(args,p.stdout,p.stderr)
 return p.stdout

def shape(font,text,lang,script):
 b=hb.Buffer();b.add_str(text);b.direction='ltr';b.script=script;b.language=lang;hb.shape(font,b)
 return [(g.codepoint,p.x_advance,p.y_advance,p.x_offset,p.y_offset)for g,p in zip(b.glyph_infos,b.glyph_positions)]
def hbfont(data):
 f=hb.Font(hb.Face(data));f.scale=(1400,1400);return f

def sfnt_bytes(path):
 f=TTFont(path,recalcTimestamp=False);f.flavor=None;b=io.BytesIO();f.save(b);return b.getvalue()

def write_bdf(path,records,family='Test',ascent=14,descent=4):
 lines=['STARTFONT 2.1','FONT test','SIZE 14 75 75','FONTBOUNDINGBOX 32 18 -11 -4','STARTPROPERTIES 6',f'FAMILY_NAME "{family}"','PIXEL_SIZE 14',f'FONT_ASCENT {ascent}',f'FONT_DESCENT {descent}','CHARSET_REGISTRY "ISO10646"','CHARSET_ENCODING "1"','ENDPROPERTIES',f'CHARS {len(records)}']
 for cp,g in sorted(records.items()):
  h=len(g['rows']);w=len(g['rows'][0])if h else 0
  lines += [f'STARTCHAR U+{cp:04X}',f'ENCODING {cp}',f'SWIDTH {g["adv"]*1000//14} 0',f'DWIDTH {g["adv"]} 0',f'BBX {w} {h} {g["x"]} {g["y"]}','BITMAP']
  lines += [f'{int(r.replace("#","1").replace(".","0"),2) << ((-w)%8):0{(w+7)//8*2}X}' for r in g['rows']]
  lines += ['ENDCHAR']
 lines+=['ENDFONT'];path.write_text('\n'.join(lines)+'\n')

def compare_full(native,reference,plan,shape_name,label):
 a,b=TTFont(native),TTFont(reference);a.ensureDecompiled();b.ensureDecompiled()
 order,records,cmap,_=plan;n=len(order)+(shape_name=='dot' and 'glyf' in a)
 assert a['maxp'].numGlyphs==b['maxp'].numGlyphs==n,(label,'glyph count',a['maxp'].numGlyphs,b['maxp'].numGlyphs,n)
 assert {cp:a.getGlyphID(name)for cp,name in a.getBestCmap().items()}=={cp:b.getGlyphID(name)for cp,name in b.getBestCmap().items()},(label,'cmap')
 assert list(a['hmtx'].metrics.values())==list(b['hmtx'].metrics.values()),(label,'hmtx')
 for field in ['ascent','descent','lineGap','advanceWidthMax','minLeftSideBearing','minRightSideBearing','xMaxExtent','numberOfHMetrics']:
  assert getattr(a['hhea'],field)==getattr(b['hhea'],field),(label,'hhea',field,getattr(a['hhea'],field),getattr(b['hhea'],field))
 for field in ['xAvgCharWidth','usWeightClass','usWidthClass','fsType','achVendID','fsSelection','sTypoAscender','sTypoDescender','sTypoLineGap','usWinAscent','usWinDescent','ulUnicodeRange1','ulUnicodeRange2','ulUnicodeRange3','ulUnicodeRange4','usMaxContext']:
  assert getattr(a['OS/2'],field)==getattr(b['OS/2'],field),(label,'OS/2',field,getattr(a['OS/2'],field),getattr(b['OS/2'],field))
 assert sorted((n.platformID,n.platEncID,n.langID,n.nameID,n.toUnicode())for n in a['name'].names)==sorted((n.platformID,n.platEncID,n.langID,n.nameID,n.toUnicode())for n in b['name'].names),(label,'names')
 status=ots.sanitize(str(native),str(native.with_suffix('.ots')),capture_output=True);assert status.returncode==0,(label,'OTS',status.stderr,status.stdout)
 fa,fb,fc=freetype.Face(str(native)),freetype.Face(str(reference)),freetype.Face(str(native.with_suffix('.ots')))
 for gid,name in enumerate(order+(['dot']if n>len(order)else[])):
  value=bitmap(fa,gid);assert value==bitmap(fb,gid),(label,gid,name,'FreeType reference');assert value==bitmap(fc,gid),(label,gid,name,'sanitized pixels')
  if name in records:assert actual_ink(value)==expected_ink(records[name]),(label,gid,name,'source pixels')
 text=''.join(chr(cp)for cp in cmap);ha=hbfont(native.read_bytes());hb_=hbfont(reference.read_bytes());checks=0
 for script in SCRIPTS:
  for lang in LANGUAGES:
   for start in range(0,len(text),2048):
    t=text[start:start+2048];assert shape(ha,t,lang,script)==shape(hb_,t,lang,script),(label,script,lang,start,'shaping');checks+=1
 native.with_suffix('.ots').unlink()
 return {'case':label,'glyphs':n,'shaping_batches':checks,'bytes':native.stat().st_size,'sha256':hashlib.sha256(native.read_bytes()).hexdigest()}

def compare_subset(binary,source,tmp,label,cps):
 text=''.join(map(chr,cps));original=TTFont(source);original.ensureDecompiled();raw=sfnt_bytes(source);h=hbfont(raw)
 reference=TTFont(io.BytesIO(raw));opts=ft_subset.Options();opts.name_IDs=['*'];opts.name_legacy=True;opts.name_languages=['*'];opts.layout_features=['*'];ft=ft_subset.Subsetter(options=opts);ft.populate(unicodes=cps);ft.subset(reference)
 refpath=tmp/(label+'-fonttools.ttf');reference.save(refpath);rf=freetype.Face(str(refpath));orig=freetype.Face(io.BytesIO(raw));count=0
 for kind in ['ttf' if 'glyf'in original else 'otf','woff','woff2']:
  dest=tmp/(label+'-subset.'+kind);invoke(binary,'subset',source,'--text',text,'-o',dest,'--woff2-quality','5','--report','json')
  f=TTFont(dest);f.ensureDecompiled();assert set(f.getBestCmap())==set(cps)&set(original.getBestCmap()),(label,kind,'subset cmap')
  assert f.reader['name']==original.reader['name'],(label,kind,'name bytes')
  status=ots.sanitize(str(dest),str(dest)+'.ots',capture_output=True);assert status.returncode==0,(label,kind,'subset OTS',status.stderr)
  data=sfnt_bytes(dest);hs=hbfont(data);hf=hbfont(sfnt_bytes(refpath));sf=freetype.Face(str(dest))
  for script in SCRIPTS:
   for lang in LANGUAGES:
    aa=shape(h,text,lang,script);bb=shape(hs,text,lang,script);cc=shape(hf,text,lang,script);assert len(aa)==len(bb)==len(cc)
    for a,b,c in zip(aa,bb,cc):
     assert a[1:]==b[1:]==c[1:],(label,script,lang,'subset positions')
     assert bitmap(orig,a[0])==bitmap(sf,b[0])==bitmap(rf,c[0]),(label,script,lang,'subset pixels');count+=1
  Path(str(dest)+'.ots').unlink()
  # Read each web wrapper via native CLI and perform a second subset.
  again=tmp/(label+'-'+kind+'-again.'+('ttf'if 'glyf'in original else'otf'));invoke(binary,'subset',dest,'--text',text,'-o',again);assert TTFont(again).getBestCmap()==f.getBestCmap()
 return {'case':label,'retained_codepoints':len(cps),'subset_shaping_glyph_checks':count,'fonttools_subset_equivalent':True}

def main():
 p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--work',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--count',type=int,default=512);p.add_argument('--full',action='store_true');a=p.parse_args();a.work.mkdir(parents=True,exist_ok=True)
 if a.full:
  from fontTools.ttLib.woff2 import WOFF2GlyfTable
  original_components=WOFF2GlyfTable._decodeComponents
  def components_without_tail_copy(self,glyph):
   self.compositeStream=memoryview(self.compositeStream)
   return original_components(self,glyph)
  WOFF2GlyfTable._decodeComponents=components_without_tail_copy
 ref=load_reference(ROOT.parent);samples=ROOT.parent/'samples/tong-pixel-sans';faces={r:parse_bdf(samples/f'TongPixelSans14Large-{r}.bdf')for r in ['SC','TC','JP','KR','Latin']}
 if not a.full:
  codes=sorted(set().union(*(f.keys()for f in faces.values())));selected={codes[i*(len(codes)-1)//(a.count-1)]for i in range(a.count)}|set(map(ord,'A“”‘’…漢骨辻国语あ한'))
  faces={r:{cp:g for cp,g in f.items()if cp in selected}for r,f in faces.items()}
 ref.build.faces=lambda root,size:[(r,faces[r],faces[r])for r in ['SC','TC','JP','KR']]
 ref.build.latin_faces=lambda root,size:(faces['Latin'],faces['Latin'])
 plan=ref.plan('prop','Large');ref.set_size('Large');paths={}
 for region,face in faces.items():paths[region]=a.work/(region+'.bdf');write_bdf(paths[region],face)
 results=[];version='test';family=ref.family_of('prop','Large');names=ref.names(family,'Square',version)
 common=['variants','--base',paths['SC'],'--lang',f'zh-Hant,zh-HK={paths["TC"]}','--lang',f'ja={paths["JP"]}','--lang',f'ko={paths["KR"]}','--script',f'latn,grek,cyrl={paths["Latin"]}','--family',family,'--vendor','TONG','--font-version',version,'--copyright',names['copyright'],'--license',names['licenseDescription'],'--license-url',names['licenseInfoURL']]
 for shape_name in ['square','dot']:
  for kind,builder in [('ttf',ref.build_ttf),('otf',ref.build_otf)]:
   label=f'{"full"if a.full else"sample"}-{shape_name}-{kind}';native=a.work/(label+'.'+kind);reference=a.work/(label+'-reference.'+kind)
   invoke(a.binary,*common,'--shape',shape_name,'-o',native)
   expected=builder('prop',shape_name,version,plan,'Large');expected.save(reference)
   results.append(compare_full(native,reference,plan,shape_name,label));print(label,'full PASS',flush=True)
   cmap=sorted(plan[2]);cps=sorted(set(cmap[::max(1,len(cmap)//24)])|set(cp for cp in map(ord,'A“”‘’…漢骨辻国语あ한')if cp in plan[2]))
   results.append(compare_subset(a.binary,native,a.work,label,cps));print(label,'subset PASS',flush=True)
   a.output.write_text(json.dumps({'partial':True,'results':results},indent=2)+'\n')
   # Keep native files as reusable fuzz seeds; free generated oracle objects.
   del expected
  web=a.work/(f'{shape_name}-auto.woff2');invoke(a.binary,*common,'--shape',shape_name,'-o',web,'--woff2-quality','5');f=TTFont(web);f.ensureDecompiled();assert ('glyf'in f)==(shape_name=='dot');status=ots.sanitize(str(web),str(web)+'.ots',capture_output=True);assert status.returncode==0,status.stderr;Path(str(web)+'.ots').unlink()
 output={'partial':False,'tools':{n:importlib.metadata.version(n)for n in ['fonttools','freetype-py','uharfbuzz','opentype-sanitizer']},'woff2_oracle_zero_copy':a.full,'reference_plan':'unchanged python_reference.plan with sample-backed build.faces/latin_faces','results':results};a.output.write_text(json.dumps(output,indent=2)+'\n');print('ALL PASS',flush=True)
if __name__=='__main__':main()
