#!/usr/bin/env python3
"""W5.B independent external-tool checks; synthetic fixtures are original MIT artwork."""
import argparse, hashlib, importlib, json, os, struct, subprocess, sys, urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
WORK=ROOT.parent/'local-work'
P=WORK/'w5b-oracle'
FIX=P/'fixtures'
LOG={'commands':[],'sources':[],'checks':[],'known_oracle_differences':[]}
def sha(b):return hashlib.sha256(b).hexdigest()
def run(args,cwd=ROOT):
 p=subprocess.run(list(map(str,args)),cwd=cwd,capture_output=True)
 LOG['commands'].append({'argv':list(map(str,args)),'cwd':str(cwd),'exit':p.returncode,'stdout_sha256':sha(p.stdout),'stderr_sha256':sha(p.stderr),'stdout':p.stdout.decode(errors='replace')[:8000],'stderr':p.stderr.decode(errors='replace')[:8000]})
 if p.returncode:raise RuntimeError(p.stderr.decode(errors='replace'))
 return p.stdout

def source(url,name,license):
 b=urllib.request.urlopen(url,timeout=40).read();dest=P/name;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(b)
 LOG['sources'].append({'url':url,'path':str(dest),'sha256':sha(b),'license':license})
 return b

def pixels(g):
 if g['gray']:return g['bytes']
 return [255*((g['bytes'][y*((g['width']+7)//8)+x//8]>>(7-x%8))&1) for y in range(g['height']) for x in range(g['width'])]
def mapped(f):return {c:g for g in f['glyphs'] for c in g['codes']}
def compare(a,b,geometry=True):
 assert len(a)==len(b),(len(a),len(b))
 for fa,fb in zip(a,b):
  ma,mb=mapped(fa),mapped(fb);assert ma.keys()==mb.keys(),(ma.keys(),mb.keys())
  for cp,g in ma.items():
   h=mb[cp];assert pixels(g)==pixels(h),(cp,pixels(g),pixels(h));assert g['advance']==h['advance'],(cp,g,h)
   if geometry:assert tuple(g[k] for k in ['width','height','x','y'])==tuple(h[k] for k in ['width','height','x','y']),(cp,g,h)
def rust(profile,path):return json.loads(run([EXE,'dump',profile,path]))
def mbdump(path,fmt,native8=False):
 import monobit
 result=[]
 for f in monobit.load(path,format=fmt):
  gs=[]
  for g in f.glyphs:
   cps=[int(c) for c in g.codepoints] or [ord(c) for c in g.chars if len(c)==1]
   levels=7 if native8 else g.levels-1
   gs.append({'codes':cps,'width':g.width,'height':g.height,'x':g.left_bearing,'y':g.shift_up,'advance':g.advance_width,'gray':True,'bytes':[(int(v)*255+levels//2)//levels for row in g.as_matrix() for v in row]})
  result.append({'glyphs':gs})
 return result

def kernel_oracle():
 # BSD native byte/bit conversion bodies remain unmodified and external.
 url='https://raw.githubusercontent.com/NetBSD/src/trunk/sys/dev/wsfont/wsfont.c'
 raw=source(url,'wsfont.c','BSD-2-Clause').decode()
 header=source('https://raw.githubusercontent.com/NetBSD/src/trunk/sys/dev/wsfont/wsfont.h','wsfont.h','BSD-2-Clause').decode()
 table=raw[raw.index('static const u_char reverse[256]'):raw.index('static struct\tfont *wsfont_find0')]
 start=raw.index('static void\nwsfont_revbit(');end=raw.index('void\nwsfont_enum(',start)
 funcs=raw[start:end]
 macro=next(line for line in header.splitlines() if line.startswith('#define FONT_IS_ALPHA'))
 copyright=raw[:raw.index('#include')]
 glue=r'''
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
struct wsdisplay_font { unsigned fontwidth,fontheight,stride,numchars; unsigned char *data; };
'''+table+funcs+'\n'+macro+r'''
static uint32_t le(const unsigned char *p) { return p[0]|p[1]<<8|p[2]<<16|p[3]<<24; }
int main(int argc,char **argv) {
 if(argc!=2)return 2;FILE *f=fopen(argv[1],"rb");if(!f)return 3;unsigned char h[100];if(fread(h,1,100,f)!=100||memcmp(h,"WSFT",4))return 4;
 struct wsdisplay_font font={le(h+80),le(h+84),le(h+88),le(h+72),0};size_t n=(size_t)font.stride*font.fontheight*font.numchars; font.data=malloc(n);if(!font.data||fread(font.data,1,n,f)!=n)return 5;fclose(f);
 if(le(h+92)==2)wsfont_revbit(&font);if(le(h+96)==2)wsfont_revbyte(&font);
 printf("[{\"glyphs\":[");for(unsigned i=0;i<font.numchars;i++){if(i)putchar(',');printf("{\"codes\":[%u],\"width\":%u,\"height\":%u,\"x\":0,\"y\":0,\"advance\":%u,\"gray\":true,\"bytes\":[",le(h+68)+i,font.fontwidth,font.fontheight,font.fontwidth);
 for(unsigned y=0;y<font.fontheight;y++)for(unsigned x=0;x<font.fontwidth;x++){if(y||x)putchar(',');unsigned char *p=font.data+((size_t)i*font.fontheight+y)*font.stride;printf("%u",FONT_IS_ALPHA(&font)?p[x]:255*((p[x/8]>>(7-x%8))&1));}printf("]}");}printf("]}]\n");free(font.data);return 0;
}
'''
 p=P/'wsfont-kernel-oracle.c';p.write_text(copyright+glue)
 LOG['sources'].append({'path':str(p),'sha256':sha(p.read_bytes()),'derived_from':url,'license':'BSD-2-Clause','adaptation':'Original reverse table, wsfont_revbit and wsfont_revbyte verbatim; original FONT_IS_ALPHA macro; standalone bounded-fixture file reader and JSON output wrapper. No native routine changes.'})
 run(['cc','-O2',p,'-o',P/'wsfont-oracle'])

def first_four():
 run([EXE,'emit',FIX]);kernel_oracle()
 source('http://www.eonet.ne.jp/~hirotsu/bin/bmf_format.txt','beos-format.txt','Author format documentation; research only')
 source('https://people.inf.ethz.ch/wirth/ProjectOberon/Sources/Fonts.Mod.txt','Fonts.Mod.txt','Project Oberon source; research only')
 source('https://raw.githubusercontent.com/Prodatron/symdoc-developer/13f125a73925a76b9016de5ebe874b591b8aec2b/SymbOS-DesktopDataRecords.txt','SymbOS-DesktopDataRecords.txt','GPL-3.0 documentation; facts only, no production code copied')
 for profile in ['beos','beos-16','oberon','symbos','wsfont']:
  fmt=profile.split('-')[0];src=json.loads((FIX/(profile+'.source.json')).read_text());got=mbdump(FIX/(profile+'.bin'),fmt,native8=profile=='beos');compare(src,got);compare(src,rust(profile,FIX/(profile+'.bin')));LOG['checks'].append(profile+' writer and all source pixels/metrics/codes independently decoded by monobit')
 LOG['known_oracle_differences'].append('BeOS Native8: monobit nibble decoding used unchanged, sample interpretation explicitly normalized by 255/7 according to Hirotsu original specification; unmodified monobit scales by 255/15. FullNibble profile is compared without this adaptation.')
 for profile in ['wsfont','wsfont-rbit','wsfont-rbyte','wsfont-rbit-rbyte','wsfont-alpha']:
  src=json.loads((FIX/(profile+'.source.json')).read_text());native=json.loads(run([P/'wsfont-oracle',FIX/(profile+'.bin')]));compare(src,native);compare(src,rust(profile,FIX/(profile+'.bin')));LOG['checks'].append(profile+' all pixels/metrics/codes checked through unmodified NetBSD kernel order conversion functions')
 # Test separately authored wire records, not only our writer's output.
 import monobit
 for profile in ['beos-16','oberon','symbos','wsfont']:
  fmt=profile.split('-')[0];f=monobit.load(FIX/(profile+'.bin'),format=fmt)
  out=FIX/(profile+'.monobit.bin');monobit.save(f,out,format=fmt,overwrite=True)
  compare(mbdump(out,fmt),rust(profile,out));LOG['checks'].append(profile+' upstream monobit writer independently read by Rust')
 # All SymbOS variants are decoded by an independent primary-spec parser below.
 for profile in ['symbos','symbos-small','symbos-big','symbos-big-extended','symbos-small-extended']:
  b=(FIX/(profile+'.bin')).read_bytes();got=symbos_spec(b);src=json.loads((FIX/(profile+'.source.json')).read_text());compare(src,got);compare(src,rust(profile,FIX/(profile+'.bin')));LOG['checks'].append(profile+' primary-spec parser all pixels/metrics/codes; this is an independent authored parser, not a native SymbOS runtime')
 # Fingerprint exact executable Python modules and package version.
 LOG['versions']={'python':sys.version,'monobit':monobit.__version__}
 for name in ['beos','oberon','symbos','wsfont']:
  m=importlib.import_module('monobit.storage.fontformats.'+name);p=Path(m.__file__);LOG['sources'].append({'path':str(p),'sha256':sha(p.read_bytes()),'license':'MIT','package':'monobit 0.54.0 PyPI sdist'})
 for p in sorted(FIX.iterdir()):
  if p.is_file():LOG.setdefault('artifacts',[]).append({'path':str(p),'sha256':sha(p.read_bytes()),'bytes':p.stat().st_size})

def symbos_spec(b):
 big=bool(b[0]&64);small=bool(b[0]&32) and not big;h=b[0]&(63 if big else 15);first=1 if b[0]&128 else b[1];count=255 if b[0]&128 else 128-first;gs=[]
 for i in range(count):
  if big:
   p=3+2*i+int.from_bytes(b[2+2*i:4+2*i],'little');width=b[p];p+=1;cols=[]
   while b[p]:
    cw=b[p];p+=1;cols.append((cw,b[p:p+h]));p+=h
   rows=[[255*((data[y]>>(7-x))&1) for cw,data in cols for x in range(cw)] for y in range(h)]
  else:
   size=9 if small else 16;p=2+i*size;width=b[p];rows=[[255*((b[p+1+y]>>(7-x))&1) for x in range(width)] for y in range(h)]
  gs.append({'codes':[first+i],'width':width,'height':h,'x':0,'y':0,'advance':width,'gray':True,'bytes':[v for row in rows for v in row]})
 return [{'glyphs':gs}]

# Standalone wrappers are original fixture adapters; no third-party implementation
# is embedded. Native implementation bodies are fetched into local-work only.
GPI_WRAPPER='/* Original JSON wrapper around Alexander Taylor\'s unchanged public-domain parser. */\n#include <stdio.h>\n#include <stdlib.h>\n#include "otypes.h"\n#include "gpifont.h"\nint main(int argc,char **argv){\n if(argc!=2)return 2;ULONG count=1;printf("[");for(ULONG face=0;face<count;face++){\n OS2FONTRESOURCE f={0};ULONG rc=ReadOS2FontResource(argv[1],face,&count,&f);if(rc){fprintf(stderr,"native GPI error %u\\n",rc);return 3;}\n if(face)putchar(\',\');printf("{\\"glyphs\\":[");int comma=0;\n for(ULONG cp=(USHORT)f.pMetrics->usFirstChar;cp<=(ULONG)(USHORT)f.pMetrics->usFirstChar+(USHORT)f.pMetrics->usLastChar;cp++){\n GLYPHBITMAP g={0};if(!ExtractOS2FontGlyph(cp,&f,&g))continue;if(comma++)putchar(\',\');\n printf("{\\"codes\\":[%u],\\"width\\":%u,\\"height\\":%u,\\"x\\":%d,\\"y\\":%d,\\"advance\\":%d,\\"gray\\":true,\\"bytes\\":[",cp,g.width,g.rows,g.horiBearingX,f.pFontDef->pCellBaseOffset-(int)g.rows,g.horiAdvance);\n for(unsigned y=0;y<g.rows;y++)for(unsigned x=0;x<g.width;x++){if(x||y)putchar(\',\');printf("%u",255*((g.buffer[y*g.pitch+x/8]>>(7-x%8))&1));}printf("]}");free(g.buffer);\n }printf("]}");free(f.pSignature);\n }printf("]\\n");return 0;\n}\n'
GEOS_WRAPPER='/* Original harness calling the external publisher\'s unchanged wire classes. */\nimport com.kreative.bitsnpicas.geos.*;\nimport java.io.*;\nimport java.nio.file.*;\nimport java.util.*;\npublic class GeosOracle {\n static boolean comma;\n static void glyph(GEOSFontStrike s,int index,int cp){\n  if(index>=s.getGlyphCount())return;byte[][] rows=s.getGlyph(index);if(rows==null)return;\n  int width=s.xCoord[index+1]-s.xCoord[index],x=0,advance=width;\n  if(s.offsetWidths!=null){x=s.offsetWidths[index].offset;advance=s.offsetWidths[index].width;}\n  if(cp>=128&&width==0&&advance==0)return;\n  if(comma)System.out.print(",");comma=true;\n  System.out.printf("{\\"codes\\":[%d],\\"width\\":%d,\\"height\\":%d,\\"x\\":%d,\\"y\\":%d,\\"advance\\":%d,\\"gray\\":true,\\"bytes\\":[",cp,width,s.height,x,s.ascent+1-s.height,advance);\n  boolean c=false;for(byte[] row:rows)for(byte v:row){if(c)System.out.print(",");c=true;System.out.print(v&255);}System.out.print("]}");\n }\n public static void main(String[] args)throws Exception{\n  byte[] data=Files.readAllBytes(Path.of(args[1]));boolean bare=args[0].equals("bare");\n  if(bare){GEOSFontStrike s=new GEOSFontStrike(data);System.out.print("[{\\"glyphs\\":[");for(int cp=32;cp<127;cp++)glyph(s,cp-32,cp);System.out.println("]}]");if(args.length>2)Files.write(Path.of(args[2]),s.write());return;}\n  GEOSFontFile file=new GEOSFontFile(new DataInputStream(new ByteArrayInputStream(data)));\n  List<Integer> sizes=file.getFontPointSizes();boolean repaired=false;\n  // Documented harness-only workaround for publisher isMega\'s Unicode-map test.\n  byte[] r54=file.vlirData.get(54);\n  if(!file.isMega()&&r54.length>=14){GEOSFontStrike index=new GEOSFontStrike(r54);if(index.bitmap.length==0&&index.utf8Map!=null){sizes.removeIf(i->i>=49&&i<=54);repaired=true;}}\n  GEOSFontFile written=new GEOSFontFile();written.setFontName(file.getFontName());written.setFontID(file.getFontID());\n  System.out.print("[");boolean fc=false;\n  for(int size:sizes){GEOSFontPointSize p=file.getFontPointSize(size);\n   if(repaired&&size==48){p=file.getFontPointSize(54);p.megaStrikeIndex=new GEOSFontStrike(r54);p.megaStrikeIndex.utf8Map=p.utf8Map;p.megaStrikes=new GEOSFontStrike[6];for(int i=0;i<6;i++)p.megaStrikes[i]=new GEOSFontStrike(file.vlirData.get(48+i));p.strike=null;}\n   if(fc)System.out.print(",");fc=true;System.out.print("{\\"glyphs\\":[");comma=false;\n   for(int cp=32;cp<127;cp++)glyph(p.isMega()?p.megaStrikes[(cp-32)/16]:p.strike,cp-32,cp);\n   if(p.isUTF8())for(int cp=128;cp<=0x10ffff;cp++){UTF8StrikeEntry e=p.utf8Map.get(cp);if(e!=null)glyph(p.utf8Strikes.get(e),cp&63,cp);}\n   System.out.print("]}");if(args.length>2)written.setFontPointSize(size,p);\n  }\n  System.out.println("]");if(args.length>2)written.recalculate();if(args.length>2)try(DataOutputStream out=new DataOutputStream(new FileOutputStream(args[2]))){written.write(out);}\n }\n}\n'
PCGEOS_WRAPPER='/* External oracle: packed C transcription of PC/GEOS fontDr.def declarations.\n * Original declarations Copyright GeoWorks; original source GPL-3.0.\n * Standalone fixture/JSON wrapper authored for fontpixel; never linked into production.\n */\n#include <stdint.h>\n#include <stdio.h>\n#include <stdlib.h>\n#include <math.h>\n#pragma pack(push,1)\ntypedef struct{uint8_t frac;int16_t whole;}WB;\ntypedef struct{char signature[4];uint8_t minor,major;uint16_t header;}FileInfo;\ntypedef struct{uint16_t id,maker;uint8_t family;char name[20];uint16_t start,end,outline_start,outline_end;}Info8;\ntypedef struct{uint16_t id,maker;uint8_t family;uint16_t name[20];uint16_t start,end,outline_start,outline_end,first,last;}Info16;\ntypedef struct{uint8_t style;WB point;uint16_t size;uint32_t pos;}Point8;\ntypedef struct{uint8_t style;int8_t point;uint8_t charset;uint16_t size;uint32_t pos;}Point16;\ntypedef struct{uint16_t size,maker;WB avg,max,adjust,height,accent,mean,base_adjust,baseline,descent,leading;uint16_t kern_count,kern_pairs,kern_values;}Common;\ntypedef struct{Common common;uint8_t first,last,def;WB under,thickness,strike,above,below;int16_t min_x,min_y,max_y,max_x;uint16_t pixel_height;uint8_t flags;uint16_t heap;}Buffer8;\ntypedef struct{Common common;uint16_t first,last,def;WB under,thickness,strike,above,below;int16_t min_x,min_y;uint16_t pixel_height;uint8_t flags;uint16_t heap;}Buffer16;\ntypedef struct{uint16_t offset;WB width;uint8_t flags;uint16_t usage;}Char8;\ntypedef struct{uint16_t offset;WB width;uint8_t flags;}Char16;\ntypedef struct{uint8_t width,rows;int8_t y,x;}Data;\n#pragma pack(pop)\n_Static_assert(sizeof(Info8)==33&&sizeof(Info16)==57,"native info sizes");\n_Static_assert(sizeof(Point8)==10&&sizeof(Point16)==9,"native point sizes");\n_Static_assert(sizeof(Buffer8)==71&&sizeof(Buffer16)==70,"native buffer sizes");\nstatic double q(WB v){return v.whole+v.frac/256.0;}\nint main(int argc,char**argv){if(argc!=2)return 2;FILE*f=fopen(argv[1],"rb");if(!f)return 3;fseek(f,0,SEEK_END);long len=ftell(f);rewind(f);unsigned char*b=malloc(len);if(fread(b,1,len,f)!=(size_t)len)return 4;fclose(f);FileInfo*head=(void*)b;int dbcs=head->major==2;unsigned tab=dbcs?((Info16*)(b+8))->start:((Info8*)(b+8))->start;unsigned end=dbcs?((Info16*)(b+8))->end:((Info8*)(b+8))->end;unsigned stride=dbcs?sizeof(Point16):sizeof(Point8);printf("[");\nfor(unsigned p=tab;p<end;p+=stride){if(p!=tab)putchar(\',\');void*entry=b+6+p;unsigned pos=dbcs?((Point16*)entry)->pos:((Point8*)entry)->pos;unsigned char*font=b+pos;Common*c=(void*)font;unsigned first=dbcs?((Buffer16*)font)->first:((Buffer8*)font)->first;unsigned last=dbcs?((Buffer16*)font)->last:((Buffer8*)font)->last;unsigned hs=dbcs?sizeof(Buffer16):sizeof(Buffer8);unsigned cs=dbcs?sizeof(Char16):sizeof(Char8);double point=dbcs?((Point16*)entry)->point:q(((Point8*)entry)->point);unsigned style=dbcs?((Point16*)entry)->style:((Point8*)entry)->style;\nprintf("{\\"native_style\\":%u,\\"point_value\\":%.8g,\\"ascent\\":%.8g,\\"descent\\":%.8g,\\"line_height\\":%.8g,\\"glyphs\\":[",style,point,nearbyint(q(c->height)-q(c->accent)-q(c->descent)),nearbyint(q(c->descent)),nearbyint(q(c->height)+q(c->leading)+q(c->adjust)));int comma=0;\nfor(unsigned cp=first;cp<=last;cp++){Char16*ce=(void*)(font+hs+(cp-first)*cs);if(!ce->offset)continue;Data*d=(void*)(font+ce->offset);unsigned char*bits=(void*)(d+1);if(comma++)putchar(\',\');printf("{\\"codes\\":[%u],\\"width\\":%u,\\"height\\":%u,\\"x\\":%d,\\"y\\":%.0f,\\"advance\\":%.0f,\\"gray\\":true,\\"bytes\\":[",cp,d->width,d->rows,d->x,nearbyint(q(c->baseline)-d->rows-d->y),nearbyint(q(ce->width)));for(unsigned y=0;y<d->rows;y++)for(unsigned x=0;x<d->width;x++){if(x||y)putchar(\',\');printf("%u",ce->flags&8?0:255*((bits[y*((d->width+7)/8)+x/8]>>(7-x%8))&1));}printf("]}");}printf("]}");}printf("]\\n");free(b);return 0;}\n'
VT_PREFIX='/* Original fixture wrapper; native lookup bodies inserted unchanged below. */\n#include <stdint.h>\n#include <stdio.h>\n#include <stdlib.h>\n#include <string.h>\n#define __packed __attribute__((packed))\n#define __nonstring\n#include <sys/font.h>\n#define howmany(a,b) (((a)+(b)-1)/(b))\ntypedef uint64_t term_char_t;\n#define TCHAR_CHARACTER(c) ((uint32_t)((c)&0x1fffff))\n#define TCHAR_FORMAT(c) ((c)>>32)\n#define TF_BOLD 1\n#define TF_CJK_RIGHT 2\n'
VT_SUFFIX='static uint32_t be32(const unsigned char*p){return (uint32_t)p[0]<<24|(uint32_t)p[1]<<16|p[2]<<8|p[3];}\nstatic uint16_t be16(const unsigned char*p){return p[0]<<8|p[1];}\nint main(int argc,char**argv){if(argc!=2)return 2;FILE*f=fopen(argv[1],"rb");if(!f)return 3;unsigned char h[32];if(fread(h,1,32,f)!=32)return 4;struct vt_font v={0};v.vf_width=h[8];v.vf_height=h[9];unsigned bytes=((v.vf_width+7)/8)*v.vf_height;unsigned count=be32(h+12);v.vf_bytes=malloc((size_t)count*bytes);if(fread(v.vf_bytes,bytes,count,f)!=count)return 5;uint8_t*present=calloc(0x110000,1);\nfor(unsigned i=0;i<4;i++){unsigned n=be32(h+16+4*i);v.vf_map_count[i]=n;v.vf_map[i]=calloc(n,sizeof(vfnt_map_t));for(unsigned j=0;j<n;j++){unsigned char b[8];if(fread(b,1,8,f)!=8)return 6;vfnt_map_t*m=&v.vf_map[i][j];m->vfm_src=be32(b);m->vfm_dst=be16(b+4);m->vfm_len=be16(b+6);for(unsigned k=0;k<=m->vfm_len;k++)present[m->vfm_src+k]|=1<<i;}}\nfclose(f);unsigned styles=v.vf_map_count[2]||v.vf_map_count[3]?2:1;printf("[");for(unsigned style=0;style<styles;style++){if(style)putchar(\',\');printf("{\\"glyphs\\":[");int comma=0;\nfor(unsigned cp=0;cp<0x110000;cp++){if(!(present[cp]&(style?15:3)))continue;unsigned wide=!!(present[cp]&(style?10:2));unsigned width=v.vf_width*(1+wide);const uint8_t*left=vtfont_lookup(&v,cp|((uint64_t)style<<32));const uint8_t*right=vtfont_lookup(&v,cp|((uint64_t)(style|2)<<32));if(comma++)putchar(\',\');printf("{\\"codes\\":[%u],\\"width\\":%u,\\"height\\":%u,\\"x\\":0,\\"y\\":0,\\"advance\\":%u,\\"gray\\":true,\\"bytes\\":[",cp,width,v.vf_height,width);\nfor(unsigned y=0;y<v.vf_height;y++)for(unsigned x=0;x<width;x++){if(x||y)putchar(\',\');unsigned xx=x%v.vf_width;const uint8_t*p=x<v.vf_width?left:right;printf("%u",255*((p[y*((v.vf_width+7)/8)+xx/8]>>(7-xx%8))&1));}printf("]}");}printf("]}");}printf("]\\n");return 0;}\n'

def authored(name,data,description):
 dest=P/name;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_text(data)
 LOG['sources'].append({'path':str(dest),'sha256':sha(dest.read_bytes()),'license':'Original test adapter; no production dependency','adaptation':description})
 return dest

def remaining_oracles():
 import difflib
 # Public-domain author parser and original writer, compiled unchanged.
 base='https://raw.githubusercontent.com/altsan/os2-gpi-font-tools/8c4cb34a013a9daa0039a4ff0a86bb5fea5dd08d/'
 for name in ['gpifont.h','otypes.h','os2res.h','pmugl.h']:
  source(base+'include/'+name,'gpi/include/'+name,'Public domain author parser/declarations (pmugl: original source header)')
 for name in ['gpifont.c','os2font.c']:
  source(base+'parser/'+name,'gpi/'+name,'Public domain, Alexander Taylor 2012/2023')
 wrapper=authored('gpi-oracle.c',GPI_WRAPPER,'Calls unchanged ReadOS2FontResource and ExtractOS2FontGlyph; reads documented baseline word; emits JSON. No native source changes.')
 run(['cc','-O2','-I'+str(P/'gpi/include'),wrapper,P/'gpi/gpifont.c','-o',P/'gpi-oracle'])
 run(['cc','-O2','-I'+str(P/'gpi/include'),P/'gpi/os2font.c',P/'gpi/gpifont.c','-lm','-o',P/'os2font'])
 run(['cc','-O2','-shared','-fPIC','-I'+str(P/'gpi/include'),P/'gpi/gpifont.c','-o',P/'gpi-native.so'])
 # External MPL publisher classes remain inside the pinned release JAR.
 jar=WORK/'w44-oracles/BitsNPicas.jar'
 if not jar.exists():source('https://github.com/kreativekorp/bitsnpicas/releases/download/v2.2.2/BitsNPicas.jar','BitsNPicas.jar','MPL-1.1/LGPL alternative');jar=P/'BitsNPicas.jar'
 assert sha(jar.read_bytes())=='22685a52bc1a7f26680f80197548e74dc61a9f58eb083d6f700220a48a1165a6'
 LOG['sources'].append({'url':'https://github.com/kreativekorp/bitsnpicas/releases/download/v2.2.2/BitsNPicas.jar','path':str(jar),'sha256':sha(jar.read_bytes()),'license':'MPL-1.1 with LGPL alternative','scope':'External release executable; separate pinned source declarations used for research'})
 base='https://raw.githubusercontent.com/kreativekorp/bitsnpicas/43b11dca0809a3342b8281b570897e8654ffe705/main/java/BitsNPicas/'
 for name in ['GEOSFontFile.java','GEOSFontStrike.java','UTF8StrikeMap.java','VLIRRecordBlock.java','ConvertFile.java']:
  source(base+'src/com/kreative/bitsnpicas/geos/'+name,'geos-source/'+name,'MPL-1.1/LGPL alternative; format facts only')
 source(base+'LICENSE','geos-source/LICENSE','MPL-1.1/LGPL alternative')
 java=authored('GeosOracle.java',GEOS_WRAPPER,'Calls unchanged publisher wire readers/writers and glyph extractor; harness fixes only collection selection for MEGA+UTF8, documented separately.')
 run(['javac','-cp',jar,java]);LOG['java_classpath']=str(jar)+os.pathsep+str(P)
 # PC/GEOS original native declarations, not implementation code.
 revision='8251622828433bfe48873992245af36f19e6ae57'
 for path in ['Include/Internal/fontDr.def','Library/Kernel/Graphics/graphicsConstant.def']:
  source('https://raw.githubusercontent.com/bluewaysw/pcgeos/'+revision+'/'+path,'pcgeos-source/'+Path(path).name,'GPL-3.0 original declarations; facts only, no implementation copied')
 pc=authored('pcgeos-declarations-oracle.c',PCGEOS_WRAPPER,'Independent C parser with packed structure declarations and compile-time size assertions derived from native field facts. This is not execution of the GEOS kernel.')
 run(['cc','-O2',pc,'-lm','-o',P/'pcgeos-oracle'])
 # FreeBSD native writer and kernel lookup, external BSD-2-Clause bodies.
 base='https://raw.githubusercontent.com/FreeBSD/freebsd-src/main/'
 src=source(base+'usr.bin/vtfontcvt/vtfontcvt.c','vtfont/vtfontcvt.c','BSD-2-Clause').decode()
 for name in ['font.h','fnv_hash.h']:source(base+'sys/sys/'+name,'vtfont/sys/'+name,'BSD-2-Clause')
 source(base+'sys/dev/vt/vt_font.c','vtfont/vt_font.c','BSD-2-Clause')
 authored('vtfont/sys/endian.h','#include <endian.h>\n'.replace('\\n','\n'),'Linux byte-order include shim')
 compat='#include <stdint.h>\n#include <stdio.h>\n#include <stdlib.h>\ntypedef unsigned long u_register_t;\n#define __packed __attribute__((packed))\n#define __nonstring\n#define __unused __attribute__((unused))\nstatic char *fgetln(FILE *f,size_t *len){static char *line=NULL;static size_t cap=0;ssize_t n=getline(&line,&cap,f);if(n<0)return NULL;*len=(size_t)n;return line;}\n'
 port=src.replace('#include <sys/types.h>','#include <sys/types.h>\n'.replace('\\n','\n')+compat,1)
 port=port.replace('#include <sys/queue.h>','#include <sys/queue.h>\n#ifndef TAILQ_FOREACH_SAFE\n#define TAILQ_FOREACH_SAFE(v,h,f,t) for((v)=TAILQ_FIRST(h);(v)&&(((t)=TAILQ_NEXT(v,f)),1);(v)=(t))\n#endif'.replace('\\n','\n'))
 port=port.replace('#include <lz4.h>','/* External native .fnt output only; compressed C-source mode unavailable. */\nstatic size_t lz4_compress(void*a,void*b,size_t c,size_t d,int e){(void)a;(void)b;(void)c;(void)d;(void)e;abort();}'.replace('\\n','\n'))
 native=authored('vtfont/vtfontcvt-linux.c',port,'Linux libc/endian/queue annotation compatibility only. Native font parsing and VFNT writer unchanged. Unused LZ4 C-source output stub aborts; only native font mode is tested.')
 authored('vtfont/vtfontcvt-linux.diff',''.join(difflib.unified_diff(src.splitlines(True),port.splitlines(True),fromfile='upstream/vtfontcvt.c',tofile='linux/vtfontcvt.c')),'Complete portability diff')
 run(['cc','-O2','-I'+str(P/'vtfont'),native,'-o',P/'vtfontcvt'])
 raw=(P/'vtfont/vt_font.c').read_text();body=raw[raw.index('static uint16_t\nvtfont_bisearch'):raw.index('struct vt_font *\nvtfont_ref')]
 native=authored('vtfont-kernel-oracle.c',raw[:raw.index('#include')]+VT_PREFIX+body+VT_SUFFIX,'Native vtfont_bisearch and vtfont_lookup copied unchanged externally; standalone fixture reader and terminal flag adapters only')
 run(['cc','-O2','-I'+str(P/'vtfont'),native,'-o',P/'vtfont-oracle'])

def last_four():
 import monobit
 LOG.setdefault('versions', {'python':sys.version,'monobit':monobit.__version__})
 remaining_oracles()
 for profile in ['geos-bare','geos','geos-extended','geos-utf8','geos-mega','geos-mega-utf8']:
  path=FIX/(profile+'.bin');published=FIX/(profile+'.publisher.bin')
  native=json.loads(run(['java','-cp',LOG['java_classpath'],'GeosOracle','bare' if profile=='geos-bare' else 'cvt',path,published]));compare(rust(profile,path),native);compare(native,rust(profile,published))
  LOG['checks'].append(profile+' full pixels/metrics/codes/strike count through external publisher GEOS classes, and publisher-rebuilt resource independently read by Rust')
 LOG['known_oracle_differences'].append('BitsNPicas 2.2.2 isMega tests bitmap offset >= entire record length, so fails when an index-only MEGA record appends UTF8 metadata. Harness selects the six native subset records and native UTF8 map explicitly for this combination. All strip, glyph and map decoding/writing stays in unchanged publisher classes.')
 for profile,fmt in [('geos-bare','vlir'),('geos','geos'),('pcgeos','pcgeos'),('vtfont','vtfont')]:
  path=FIX/(profile+'.bin');compare(rust(profile,path),mbdump(path,fmt));fonts=monobit.load(path,format=fmt);dest=FIX/(profile+'.monobit.bin');monobit.save(fonts,dest,format=fmt,overwrite=True);compare(mbdump(dest,fmt),rust(profile,dest));LOG['checks'].append(profile+' monobit read/write all mapped pixels, integer metrics and codes (see style/point limitations)')
 for profile in ['gpi-fixed','gpi-prop','gpi','gpi-lx']:
  path=FIX/(profile+'.bin');native=json.loads(run([P/'gpi-oracle',path]));compare(rust(profile,path),native)
  if profile!='gpi-lx':
   dest=FIX/(profile+'.native-writer.bin');run([P/'os2font',path,'/O:'+str(dest)]);compare(json.loads(run([P/'gpi-oracle',dest])),rust(profile,dest))
  LOG['checks'].append(profile+' complete advertised range and all pixels/metrics/codes via unchanged author parser; bare variants additionally rebuilt by author os2font writer')
 # Native full-resource decompression, plus every EXEPACK2 token/overlap case.
 import ctypes
 library=ctypes.CDLL(str(P/'gpi-native.so'))
 for name,data,expected in [('LXUnpack1',bytes([3,0,2,0,65,66,0,0]),b'ABABAB'),('LXUnpack2',bytes([16,65,66,67,68,5,2,69,0,0]),b'ABCDEBCD'),('LXUnpack2',bytes([8,65,66,0x22,0,0,0]),b'ABABA'),('LXUnpack2',bytes([0x0b,0x21,0,65,66,0,0]),b'ABABAB'),('LXUnpack2',bytes([0,4,90,0,0]),b'ZZZZ')]:
  buf=ctypes.create_string_buffer(4096);buf[:len(data)]=data;func=getattr(library,name);func.argtypes=[ctypes.c_void_p,ctypes.c_uint16];func.restype=ctypes.c_uint16;n=func(buf,len(data));assert buf.raw[:len(expected)]==expected;assert buf.raw[len(expected):n]==bytes(max(0,n-len(expected)))
  LOG['checks'].append(name+' native token '+data.hex()+' -> '+expected.hex()+' matches Rust unit vector')
 plain=(FIX/'gpi-lx.bin').read_bytes();page_map=struct.unpack_from('<I',plain,72)[0]+8;at=struct.unpack_from('<I',plain,128)[0]+4096
 raw=(FIX/'gpi.bin').read_bytes()
 for kind in [1,5]:
  compressed=struct.pack('<HH',1,len(raw))+raw+b'\0\0' if kind==1 else b''.join(bytes([len(raw[i:i+63])*4])+raw[i:i+63] for i in range(0,len(raw),63))+b'\0\0'
  data=bytearray(plain);struct.pack_into('<HH',data,page_map+4,len(compressed),kind);data[at:at+len(compressed)]=compressed;path=FIX/f'gpi-lx-exepack{1 if kind==1 else 2}.bin';path.write_bytes(data)
  compare(rust('gpi-lx',path),json.loads(run([P/'gpi-oracle',path])));LOG['checks'].append(f'GPI LX compression flag {kind}: complete resource pixels/metrics/mappings through unchanged native author decompressor')
 # The author's NE parser branch is disabled; use independent monobit NE extraction
 # and then the unchanged author bare-resource decoder for every extracted face.
 from monobit.storage.fontformats.fon.os2.ne import read_os2_ne
 import io
 path=FIX/'gpi-ne.bin';raw=path.read_bytes();stream=io.BytesIO(raw);stream.seek(struct.unpack_from('<I',raw,60)[0]);resources=read_os2_ne(stream,False);native=[]
 for i,data in enumerate(resources):
  dest=FIX/f'gpi-ne.face{i}.bin';dest.write_bytes(data);native.extend(json.loads(run([P/'gpi-oracle',dest])))
 compare(rust('gpi-ne',path),native);LOG['checks'].append('GPI NE independently extracted by monobit OS/2 segment parser, then complete advertised ranges decoded by unchanged native author resource parser')
 LOG['known_oracle_differences'].append('monobit 0.54 GPI uses usLastChar as a count although native FOCA defines an inclusive final offset, omitting the last mapped glyph. Full-range checks use the author C parser. Author os2font writer assumes an additional .null descriptor: when the input lacks one its uncounted extra descriptor can be invalid; Rust retains those opaque bytes but exposes only structurally valid uncounted glyphs.')
 for profile in ['pcgeos','pcgeos-styled','pcgeos-dbcs']:
  path=FIX/(profile+'.bin');native=json.loads(run([P/'pcgeos-oracle',path]));got=rust(profile,path);compare(got,native)
  for f,n in zip(got,native):
   for k in ['ascent','descent','line_height']:assert f[k]==n[k],(profile,k,f[k],n[k])
   assert f['point'][0]/f['point'][1]==n['point_value']
   assert n['native_style']==(0 if profile=='pcgeos' else 0x30)
   assert f['style']==('Regular' if profile=='pcgeos' else 'Bold Italic')
  LOG['checks'].append(profile+' all pixels/codes/metrics/points/strike counts through independently compiled packed original-layout declarations oracle; not a GEOS runtime')
 LOG['known_oracle_differences'].append('PC/GEOS monobit 0.54 shifts point records by one byte and reverses several glyph flag meanings. Simple regular integer-point SBCS pixels remain comparable; native style, fractional point sizes and DBCS are checked against independently compiled original field declarations.')
 # Original hand-built pool: independent per-half bold fallback and unused glyph.
 raw=b'VFNT0002'+bytes([5,2,0,0])+struct.pack('>I4I',6,1,1,2,2)+bytes.fromhex('8040f88840e0a8d8709818c0')
 raw+=b''.join(struct.pack('>IHH',*v) for v in [(65,1,1),(65,2,0),(65,3,0),(66,0,0),(65,0,0),(66,4,0)])
 path=FIX/'vtfont-fallback.bin';path.write_bytes(raw);compare(rust('vtfont',path),json.loads(run([P/'vtfont-oracle',path])));LOG['checks'].append('VFNT non-byte-aligned half cells, zero-index bold fallback independently per half, aliases and effective styles verified through unchanged FreeBSD kernel lookup routines')
 # Feed separately authored HEX glyphs to the native FreeBSD compiler.
 regular='# Width: 5\n# Height: 2\n0041:F888\n0042:40E0\n4E00:F840\n'.replace('\\n','\n')
 bold='# Width: 5\n# Height: 2\n0041:A8D8\n'.replace('\\n','\n')
 normal=FIX/'vtfont-native-normal.hex';normal.write_text(regular);boldfile=FIX/'vtfont-native-bold.hex';boldfile.write_text(bold);dest=FIX/'vtfont-native.bin'
 run([P/'vtfontcvt','-n','-f','font','-o',dest,normal,boldfile]);compare(rust('vtfont',dest),json.loads(run([P/'vtfont-oracle',dest])));LOG['checks'].append('Original synthetic HEX glyphs independently compiled by FreeBSD vtfontcvt, then all mapped glyphs/styles checked against native kernel lookup')
 for name in ['geos','vtfont','pcgeos','fon.os2.gpifont','fon.os2.ne','fon.os2.lx']:
  m=importlib.import_module('monobit.storage.fontformats.'+name);p=Path(m.__file__);LOG['sources'].append({'path':str(p),'sha256':sha(p.read_bytes()),'license':'MIT','package':'monobit 0.54.0 PyPI sdist'})
 for name in ['gpi-oracle','os2font','gpi-native.so','pcgeos-oracle','vtfontcvt','vtfont-oracle','GeosOracle.class']:
  p=P/name;LOG['sources'].append({'path':str(p),'sha256':sha(p.read_bytes()),'scope':'external compiled executable'})
 LOG['artifacts']=[{'path':str(p),'sha256':sha(p.read_bytes()),'bytes':p.stat().st_size} for p in sorted(FIX.iterdir()) if p.is_file()]

if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--exe',type=Path,default=WORK/'target-w5b/debug/examples/w5b_timing');p.add_argument('--first-four',action='store_true');p.add_argument('--last-four',action='store_true');a=p.parse_args();EXE=a.exe.resolve();P.mkdir(parents=True,exist_ok=True);FIX.mkdir(exist_ok=True)
 try:
  if a.last_four:run([EXE,'emit',FIX])
  else:first_four()
  if not a.first_four:last_four()
  LOG['status']='pass'
 except Exception as e:LOG['status']='fail';LOG['error']=repr(e);raise
 finally:(ROOT/'reports/w5.b-crosscheck.json').write_text(json.dumps(LOG,indent=2)+'\n')
