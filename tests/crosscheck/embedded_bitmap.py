#!/usr/bin/env python3
"""W2.4 native embedded bitmap interoperability. Never edits package inputs."""
import argparse, hashlib, importlib.metadata, io, json, os, struct, subprocess, sys, tempfile, time
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont, newTable
from fontTools.ttLib.tables.DefaultTable import DefaultTable
from fontTools.ttLib.tables.sbixStrike import Strike
from fontTools.ttLib.tables.sbixGlyph import Glyph
from PIL import Image
import freetype, uharfbuzz as hb, ots
ROOT=Path(__file__).resolve().parents[2]
def pack(fmt,*v):return struct.pack('>'+fmt,*v)
def command(*args):return subprocess.run(list(map(str,args)),check=True,capture_output=True,text=True)
def base():
    fb=FontBuilder(1400,isTTF=True);order=['.notdef','A','B','C'];fb.setupGlyphOrder(order);fb.setupCharacterMap({65:'A',66:'B',67:'C'})
    fb.setupGlyf({n:TTGlyphPen(None).glyph() for n in order});fb.setupHorizontalMetrics({n:(400,0) for n in order});fb.setupHorizontalHeader(ascent=1100,descent=-300)
    fb.setupNameTable({'familyName':'Fontpixel bitmap fixture','styleName':'Regular','uniqueFontIdentifier':'MIT bitmap fixture','fullName':'Fontpixel bitmap fixture','psName':'FontpixelBitmapFixture'});fb.setupOS2();fb.setupPost();fb.setupMaxp();fb.font.recalcTimestamp=False;fb.font['head'].created=fb.font['head'].modified=3660681600;return fb.font

def tables(index,image,depth,payload,version=2):
    sub=pack('HHI',index,image,4)
    if index==1:sub+=pack('II',0,len(payload))
    elif index==3:sub+=pack('HH',0,len(payload))
    elif index==4:sub+=pack('IHHHH',1,1,0,2,len(payload))
    elif index in (2,5):
        sub+=pack('I',len(payload))+bytes([2,3,255,1,4,254,2,5])
        if index==5:sub+=pack('IH',1,1)
    line=bytes([11,253,3,1,0,0,0,0,1,255,0,0])
    loc=pack('II',version<<16,1)+pack('IIII',56,8+len(sub),1,0)+line+bytes(12)+pack('HHBBBB',1,1,14,14,depth,1)+pack('HHI',1,1,8)+sub
    return loc,pack('I',version<<16)+payload

def png():
    im=Image.new('RGBA',(3,2));im.putdata([(0,0,0,255),(255,0,0,128),(255,255,255,0),(0,255,0,64),(0,0,255,192),(255,255,255,255)])
    o=io.BytesIO();im.save(o,format='PNG');return o.getvalue()

def fixtures(path):
    cases=[]
    for family in ['otb','bdat']:
      for index in [1,2,3,4,5]:
       for image in ([5] if index in [2,5] else [1,2,6,7]):
        for depth in [1,2,4,8]:
         # Independent expected gray levels and packing, not native encode output.
         vals=[0,1,0,1,1,1] if depth==1 else [0,1,2,3,2,1]
         maxv=(1<<depth)-1
         if depth==4:vals=[v*5 for v in vals]
         if depth==8:vals=[v*85 for v in vals]
         bits=''.join(f'{v:0{depth}b}' for v in vals)
         if image in [1,6]:bits=''.join(bits[i:i+3*depth].ljust(((3*depth+7)//8)*8,'0') for i in range(0,len(bits),3*depth))
         bits=bits.ljust((len(bits)+7)//8*8,'0');pixels=int(bits,2).to_bytes(len(bits)//8,'big')
         header=b'' if image==5 else bytes([2,3,255,1,4]+([254,2,5] if image in [6,7] else []))
         l,d=tables(index,image,depth,header+pixels);f=base();dt,lt=('EBDT','EBLC') if family=='otb' else ('bdat','bloc')
         for tag,b in [(dt,d),(lt,l)]:t=DefaultTable(tag);t.data=b;f[tag]=t
         p=path/f'{family}-i{index}-f{image}-d{depth}.ttf';f.save(p)
         cases.append(dict(path=p,family=family,index=index,image=image,depth=depth,expected=[v*255//maxv for v in vals],origins={65:(-1,-1)}))
    for image in [8,9]:
        # Independently authored direct A plus composite B at a positive x offset.
        first=bytes([2,3,255,1,4,0xa0,0x40]);second=bytes([2,4,255,1,5]+([254,2,6] if image==9 else [0]))+pack('H',1)+pack('Hbb',1,1,0)
        data=pack('I',0x20000)+first+second
        line=bytes([11,253,4,1,0,0,0,0,1,255,0,0])
        loc=pack('II',0x20000,1)+pack('IIII',56,48,2,0)+line+bytes(12)+pack('HHBBBB',1,2,14,14,1,1)
        loc+=pack('HHIHHI',1,1,16,2,2,32)+pack('HHIII',1,1,4,0,len(first))+pack('HHIII',1,image,4+len(first),0,len(second))
        f=base()
        for tag,b in [('EBDT',data),('EBLC',loc)]:t=DefaultTable(tag);t.data=b;f[tag]=t
        p=path/f'composite-f{image}.ttf';f.save(p);cases.append(dict(path=p,family='otb',expected=[255,0,255,0,255,0],origins={65:(-1,-1)},composite_expected=[0,255,0,255,0,0,255,0]))
    first=bytes([2,3,255,1,4])+bytes([85]*6);second=bytes([2,3,255,1,4])+bytes([170]*6);third=bytes([2,3,255,1,4,0,0,2,0,1,0,0,0,2,0,0])
    data=pack('I',0x20000)+first+second+third;line=bytes([11,253,3,1,0,0,0,0,1,255,0,0])
    loc=pack('II',0x20000,1)+pack('IIII',56,52,2,0)+line+bytes(12)+pack('HHBBBB',1,3,14,14,8,1)+pack('HHIHHI',1,2,16,3,3,36)+pack('HHIIII',1,1,4,0,11,22)+pack('HHIII',1,8,26,0,16)
    f=base()
    for tag,b in [('EBDT',data),('EBLC',loc)]:t=DefaultTable(tag);t.data=b;f[tag]=t
    p=path/'composite-gray-overlap.ttf';f.save(p);cases.append(dict(path=p,family='otb',expected=[85]*6,origins={65:(-1,-1)},gray_composite_expected=[255]*6))
    for index,image in [(1,17),(3,18),(2,19),(5,19)]:
        p=png();header=bytes([2,3,255,1,4]) if image==17 else bytes([2,3,255,1,4,254,2,5]) if image==18 else b''
        l,d=tables(index,image,32,header+pack('I',len(p))+p,3);f=base()
        for tag,b in [('CBDT',d),('CBLC',l)]:t=DefaultTable(tag);t.data=b;f[tag]=t
        p=path/f'color-i{index}-f{image}.ttf';f.save(p);cases.append(dict(path=p,family='color',expected=[255,128,0,64,192,255],origins={65:(-1,-1)}))
    for outline in [False,True]:
        f=base()
        if outline:
            pen=TTGlyphPen(None);pen.moveTo((200,-100));pen.lineTo((300,-100));pen.lineTo((300,100));pen.lineTo((200,100));pen.closePath();f['glyf']['A']=pen.glyph();f['hmtx'].metrics['A']=(400,200)
        t=newTable('sbix');t.version=1;t.flags=1;t.strikes={14:Strike(ppem=14,resolution=72)}
        t.strikes[14].glyphs={'A':Glyph(glyphName='A',originOffsetX=1,originOffsetY=-1,graphicType='png ',imageData=png()),'B':Glyph(glyphName='B',originOffsetX=2,originOffsetY=3,graphicType='dupe',referenceGlyphName='A')};f['sbix']=t
        p=path/f'sbix-{outline}.ttf';f.save(p);cases.append(dict(path=p,family='color',expected=[255,128,0,64,192,255],origins={65:((3,-2) if outline else (1,-1)),66:(1,-1)}))
    return cases

def ft_bitmap(face,cp):
    face.select_size(0);face.load_char(cp,freetype.FT_LOAD_RENDER|freetype.FT_LOAD_COLOR)
    s=face.glyph;b=s.bitmap;d=bytes(b.buffer);out=[]
    for y in range(b.rows):
      for x in range(b.width):
       at=y*abs(b.pitch)
       if b.pixel_mode==freetype.FT_PIXEL_MODE_MONO:v=((d[at+x//8]>>(7-x%8))&1)*255
       elif b.pixel_mode==freetype.FT_PIXEL_MODE_BGRA:v=d[at+x*4+3]
       elif b.pixel_mode==freetype.FT_PIXEL_MODE_GRAY:v=d[at+x]
       elif b.pixel_mode==freetype.FT_PIXEL_MODE_GRAY2:v=((d[at+x//4]>>(6-(x%4)*2))&3)*85
       elif b.pixel_mode==freetype.FT_PIXEL_MODE_GRAY4:v=((d[at+x//2]>>(4-(x%2)*4))&15)*17
       else:raise AssertionError(('unsupported FT pixel mode',b.pixel_mode))
       out.append(v)
    return dict(w=b.width,h=b.rows,x=s.bitmap_left,y=s.bitmap_top-b.rows,advance=s.advance.x//64,pixels=out)

def pixels(g):
    if not g['mono']:return g['pixels']
    return [((g['pixels'][y*((g['w']+7)//8)+x//8]>>(7-x%8))&1)*255 for y in range(g['h']) for x in range(g['w'])]

def native_read(binary,p,family,out):
    command(binary,'read',family,p,out);return json.loads(out.read_text())
def shape(data,text):
    f=hb.Font(hb.Face(data));f.scale=(1400,1400);b=hb.Buffer();b.add_str(text);b.guess_segment_properties();hb.shape(f,b);return [(g.codepoint,p.x_advance,p.x_offset,p.y_offset)for g,p in zip(b.glyph_infos,b.glyph_positions)]

def source_bdf(path):
    glyphs={};g=None;bitmap=False
    for line in path.read_text().splitlines():
        key,_,v=line.partition(' ')
        if key=='STARTCHAR':g={};bitmap=False
        elif key=='ENCODING':g['cp']=int(v)
        elif key=='DWIDTH':g['advance']=int(v.split()[0])
        elif key=='BBX':g['w'],g['h'],g['x'],g['y']=map(int,v.split())
        elif key=='BITMAP':g['pixels']=[];bitmap=True
        elif key=='ENDCHAR':glyphs[g['cp']]=g;bitmap=False
        elif bitmap:g['pixels'].extend(int(c)*255 for c in bin(int(line,16))[2:].zfill(len(line)*4)[:g['w']])
    return glyphs

def main():
    ap=argparse.ArgumentParser();ap.add_argument('--package',type=Path,default=ROOT.parent);ap.add_argument('--binary',type=Path,default=ROOT/'target/release/examples/bitmap_check');ap.add_argument('--synthetic-only',action='store_true');ap.add_argument('--output',type=Path,default=ROOT/'tests/crosscheck/embedded-bitmap-results.json');ap.add_argument('--fixtures',type=Path);a=ap.parse_args()
    results=[];checks=0;start=time.monotonic()
    with tempfile.TemporaryDirectory(prefix='fontpixel-bitmap-') as tmp:
      tmp=Path(tmp);cases=fixtures(tmp)
      if a.fixtures:
        import shutil
        a.fixtures.mkdir(parents=True,exist_ok=True)
        # Keep a compact corpus spanning every branch, not every depth cross product.
        chosen=[c for c in cases if '-d1.' in c['path'].name or c['family']=='color' or 'composite_expected' in c or 'gray_composite_expected' in c]
        for c in chosen:shutil.copyfile(c['path'],a.fixtures/c['path'].name)
      for c in cases:
        f=TTFont(c['path']);f.ensureDecompiled();native=native_read(a.binary,c['path'],c['family'],tmp/'read.json');face=freetype.Face(str(c['path']))
        gs={cp:g for g in native['fonts'][0]['glyphs'] for cp in g['codes']}
        ft=[]
        for cp,origin in c['origins'].items():
            g=gs[cp];assert pixels(g)==c['expected'],c['path'];assert (g['x'],g['y'])==origin,(c['path'],cp,g,origin)
            actual=ft_bitmap(face,cp)
            if actual['pixels'] != c['expected']:
                # FreeType 2.13.3 pads bit-aligned gray rows on extraction; fontTools
                # and the OpenType packing contract independently agree with native.
                assert c.get('image') in [2,5,7] and c.get('depth') in [2,4] and actual['pixels']==[0,85,170,170,85,0],(c['path'],actual)
                c['freetype_limitation']='bit-aligned gray row boundary'
            if c['family']=='otb' and 'depth' in c:
                bitmap=f['EBDT'].strikeData[0]['A'];m=getattr(bitmap,'metrics',None)
                if m is None:m=f['EBLC'].strikes[0].indexSubTables[0].metrics
                actual_rows=[bitmap.getRow(y,bitDepth=c['depth'],metrics=m) for y in range(2)]
                values=[((row[x*c['depth']//8]>>(8-c['depth']-(x*c['depth'])%8))&((1<<c['depth'])-1))*255//((1<<c['depth'])-1) for row in actual_rows for x in range(3)]
                assert values==c['expected'],('fontTools row decoding',c['path'],values)
            assert (actual['x'],actual['y'])==origin,(c['path'],cp,'FT origin',actual,origin)
            assert actual['advance']==g['advance'][0];checks+=1
        if 'composite_expected' in c:
            g=gs[66];assert pixels(g)==c['composite_expected'];actual=ft_bitmap(face,66);assert actual['pixels']==c['composite_expected'];assert (actual['x'],actual['y'],actual['advance'])==(-1,-1,5);checks+=1
        if 'gray_composite_expected' in c:
            g=gs[67];assert pixels(g)==c['gray_composite_expected'];actual=ft_bitmap(face,67);assert actual['pixels']==c['gray_composite_expected'];checks+=1
        # OTS checks the outline-bearing container; it may discard unsupported bitmap tags.
        run=ots.sanitize(str(c['path']),str(tmp/'sanitized.ttf'),capture_output=True)
        assert run.returncode==0,(c['path'],run.stderr)
        results.append(dict(case=c['path'].name,freetype_limitation=c.get('freetype_limitation'),fonttools=True,freetype=True,ots=True,native=True,sha256=hashlib.sha256(c['path'].read_bytes()).hexdigest()))
      if not a.synthetic_only:
       for bdf in sorted((a.package/'samples/tong-pixel-sans').glob('*.bdf')):
        source=source_bdf(bdf)
        for family in ['otb','bdat']:
            out=tmp/f'{family}.otb';command(a.binary,'write',family,out,bdf);f=TTFont(out);f.ensureDecompiled();native=native_read(a.binary,out,family,tmp/'read.json');face=freetype.Face(str(out));gs={cp:g for g in native['fonts'][0]['glyphs'] for cp in g['codes']};assert set(gs)==set(source)
            for cp,expected in source.items():
                g=gs[cp];actual=ft_bitmap(face,cp)
                for key in ('w','h','x','y'):assert g[key]==expected[key],(bdf,family,cp,key)
                assert pixels(g)==expected['pixels'];assert g['advance'][0]==expected['advance'];assert actual=={k:expected[k]for k in ('w','h','x','y','advance','pixels')},(bdf,cp,actual,expected);checks+=1
            assert dict(f.getBestCmap()).keys()==source.keys();sh=shape(out.read_bytes(),'ABC一二三');assert sh
            results.append(dict(case=bdf.name,family=family,glyphs=len(source),fonttools=True,freetype=True,native=True,harfbuzz_smoke=sh,sha256=hashlib.sha256(out.read_bytes()).hexdigest()))
       for ref in sorted((a.package/'samples/tong-pixel-sans').glob('*.otb')):
        native=native_read(a.binary,ref,'otb',tmp/'reference.json');out=tmp/'repack.otb';command(a.binary,'repack','otb',ref,out);original=TTFont(ref);rebuilt=TTFont(out);assert set(original.getBestCmap())==set(rebuilt.getBestCmap());a_face=freetype.Face(str(ref));b_face=freetype.Face(str(out))
        for cp in original.getBestCmap():assert ft_bitmap(a_face,cp)==ft_bitmap(b_face,cp),(ref,cp);checks+=1
        # Compare mapped glyph IDs and design advances for the unchanged 14-ppem cmap.
        assert shape(ref.read_bytes(),'ABC一二三')==shape(out.read_bytes(),'ABC一二三')
        results.append(dict(case=ref.name,reference_roundtrip=True,glyphs=len(original.getBestCmap()),freetype=True,harfbuzz=True))
       # Independent monobit writer, after primary checks to isolate its fontTools extension hooks.
       import monobit
       from monobit.storage.fontformats.sfnt import save_sfnt
       source=source_bdf(a.package/'samples/tong-pixel-sans/TongPixelSans14Large-Latin.bdf')
       native_path=tmp/'native-latin.otb';command(a.binary,'write','otb',native_path,a.package/'samples/tong-pixel-sans/TongPixelSans14Large-Latin.bdf')
       mono=monobit.load(native_path)
       for family,table in [('otb','EBDT'),('bdat','bdat')]:
           p=tmp/f'monobit-{family}.ttf';monobit.save(mono,p,format='sfnt',bitmap_table=table,ebsc_mapped_sizes=((7,28) if family=='otb' else ()),overwrite=True)
           loaded=native_read(a.binary,p,family,tmp/'monobit.json');gs={cp:g for g in loaded['fonts'][0]['glyphs']for cp in g['codes']};assert gs.keys()==source.keys()
           for cp,g in gs.items():
               expected=source[cp];assert pixels(g)==expected['pixels'];assert all(g[k]==expected[k]for k in ['w','h','x','y']);assert g['advance'][0]==expected['advance'];checks+=1
           rebuilt=tmp/f'monobit-rebuilt-{family}.ttf';command(a.binary,'repack',family,p,rebuilt);rb=TTFont(rebuilt)
           if family=='otb':assert rb['EBSC'].numSizes==2
           results.append(dict(case=f'monobit-{family}',glyphs=len(gs),native_reader=True,monobit_reader_writer=True,ebsc_targets=([7,28] if family=='otb' else []),monobit_bdat_ebsc_limitation=('upstream KeyError EBLC when asked to generate EBSC for bdat' if family=='bdat' else None)))
       # OTS supports outline-bearing fonts but strips bitmap tables by default.
       f=TTFont(native_path);fb=FontBuilder(font=f);fb.isTTF=True;fb.setupGlyf({n:TTGlyphPen(None).glyph()for n in f.getGlyphOrder()});fb.setupMaxp();p=tmp/'native-embedded.ttf';f.save(p)
       sanitized=tmp/'native-sanitized.ttf';run=ots.sanitize(str(p),str(sanitized),capture_output=True);assert run.returncode==0,run.stderr
       kept=set(TTFont(sanitized).keys());assert 'EBDT' not in kept and 'EBLC' not in kept
       bitmap_only=ots.sanitize(str(native_path),str(tmp/'native-only-sanitized.ttf'),capture_output=True);assert bitmap_only.returncode!=0 and b'no supported glyph data' in bitmap_only.stderr
       results.append(dict(case='native-sfnt-ots',outline_container_accepted=True,bitmap_tables_stripped=True,bitmap_only_rejected='no supported glyph data table(s) present'))
      payload=dict(versions={n:importlib.metadata.version(n)for n in ['fonttools','freetype-py','uharfbuzz','opentype-sanitizer']},freetype_library=freetype.version(),cases=results,glyph_checks=checks,elapsed_seconds=round(time.monotonic()-start,3))
      a.output.write_text(json.dumps(payload,indent=2)+'\n');print(json.dumps(dict(cases=len(results),glyph_checks=checks,elapsed_seconds=payload['elapsed_seconds'])))
if __name__=='__main__':main()
