#!/usr/bin/env python3
"""W5.4 external-tool crosschecks. Original synthetic fixtures are MIT.
Run with scripts/local-env.sh sourced; all third-party code stays in local-work.
"""
import argparse, difflib, hashlib, importlib, json, os, re, shutil, struct, subprocess, sys, urllib.request, zipfile
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2]
WORK = ROOT.parent / 'local-work'
P = WORK / 'w54-oracle'
E = WORK / 'w54-eka'
F = WORK / 'w54-fontconv'
FIX = P / 'fixtures'
LOG = {'commands': [], 'sources': [], 'checks': [], 'known_oracle_differences': []}
def sha(data): return hashlib.sha256(data).hexdigest()
def run(args, cwd=ROOT, output=None):
    p = subprocess.run([str(x) for x in args], cwd=cwd, capture_output=True)
    LOG['commands'].append({'argv': list(map(str,args)), 'cwd': str(cwd), 'exit': p.returncode, 'stdout_sha256':sha(p.stdout), 'stderr_sha256':sha(p.stderr), 'stdout':p.stdout.decode(errors='replace')[:10000], 'stderr':p.stderr.decode(errors='replace')[:10000]})
    if output: Path(output).write_bytes(p.stdout)
    if p.returncode: raise RuntimeError(f'{args}: {p.stderr.decode(errors="replace")}')
    return p.stdout

def source(url, dest, edits=()):
    raw = urllib.request.urlopen(url,timeout=60).read()
    after=raw
    for a,b in edits: after=after.replace(a.encode(),b.encode())
    dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(after)
    LOG['sources'].append({'url':url,'path':str(dest),'original_sha256':sha(raw),'compiled_sha256':sha(after),'patch': ''.join(difflib.unified_diff(raw.decode(errors='replace').splitlines(True),after.decode(errors='replace').splitlines(True)))})
    return raw

def setup():
    P.mkdir(parents=True,exist_ok=True);FIX.mkdir(exist_ok=True);F.mkdir(exist_ok=True)
    # Pin source commits, not mutable branch heads. Tree IDs came from GitHub's tree API.
    os1='a48c2548bd6f6086dc5dd4a8b19705da2176039c'
    base=f'https://raw.githubusercontent.com/cdaffara/symbiandump-os1/{os1}/sf/os/graphics/graphicstools/bitmapfonttools/'
    for name in ['FNTBODY.CPP','FNTBODY.H','FNTSTD.H']:
        source('https://raw.githubusercontent.com/cdaffara/symbiandump-os2/d1900b19b4525c2b5544aa5f3a9ce53e51a5acc0/sf/os/textandloc/fontservices/fontstore/src/'+name,P/name)
    names=['FNTTRAN','FNTREADR','FNTRECRD','LEXICAL','READER','RECORD','STRNG']
    for n in names:
        source(base+'src/'+n+'.CPP', P/(n+'.CPP'), [('streamoff idOffset =','uint32 idOffset =')])
        if n!='FNTTRAN': source(base+'inc/'+n+'.H',P/(n+'.H'))
    for n in ['GDR.H','UID.H','LST.H','STRNG.INL']:
        source(base+'inc/'+n,P/n,[('typedef signed long int32;','typedef signed int int32;'),('typedef unsigned long uint32;','typedef unsigned int uint32;')])
    run(['g++','-std=c++98','-fpermissive','-include','cstring','-I'+str(P),*[P/(n+'.CPP') for n in names],'-o',P/'fnttran'])
    url='https://downloads.sourceforge.net/project/palmfontconv/Font%20Converters%20for%20PalmOS%20Dev/1.63/FontConvForPalmOS163full.zip'
    archive=P/'FontConv163.zip'
    if not archive.exists(): source(url,archive)
    LOG['sources'].append({'url':url,'path':str(archive),'sha256':sha(archive.read_bytes()),'license':'GPL-2.0-or-later converter; BSD-3-Clause grayfont runtime'})
    with zipfile.ZipFile(archive) as z:
        for name in ['fontconv.c','fontconv.h','palmtypes.h','togray.c','topalmtext.c','encodings.c','missingicons.c','README.TXT','COPYING','GRANT','GrayFormat.txt','fsconvert.c']:
            if name not in z.namelist(): continue
            raw=z.read(name);data=raw.replace(b'typedef unsigned long DWord;',b'typedef unsigned int DWord;').replace(b'#include <freetype/ttunpat.h>',b'')
            (F/name).write_bytes(data)
            LOG['sources'].append({'url':url+'!/'+name,'path':str(F/name),'original_sha256':sha(raw),'compiled_sha256':sha(data),'patch':''.join(difflib.unified_diff(raw.decode(errors='replace').splitlines(True),data.decode(errors='replace').splitlines(True)))})
    run(['gcc','-w','-O2','-o','fsconvert','fsconvert.c'],F)
    run(['gcc','-w','-O2','-Dstrnicmp=strncasecmp','-o','fontconv','fontconv.c','-lm'],F)
    run(['gcc','-w','-O2','-Dstrnicmp=strncasecmp','-DFONTCONV_LIB','-o','togray','togray.c','fontconv.c','-lm'],F)
    ft=run(['pkg-config','--cflags','--libs','freetype2']).decode().split()
    run(['gcc','-w','-O2','-DFT_PARAM_TAG_UNPATENTED_HINTING=0x756e7061','-Dstrnicmp=strncasecmp','-DFONTCONV_LIB','-o','topalmtext','topalmtext.c','fontconv.c',*ft,'-lm'],F)
    ec='1fcfadbff7f8b81d8c17bc82d04b0c94d16803bf'
    for name in ['src/emu/loader/include/loader/gdr.h','src/emu/loader/src/gdr.cpp']:
        source(f'https://raw.githubusercontent.com/EKA2L1/EKA2L1/{ec}/'+name,E/name)
    for name,text in SHIMS.items():
        p=E/'shim'/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(text)
    run(['g++','-std=c++17','-O2','-I'+str(E/'shim'),'-I'+str(E/'src/emu/loader/include'),E/'src/emu/loader/src/gdr.cpp',E/'shim/main.cpp','-licuuc','-licudata','-o',E/'gdr-oracle'])
    raw=(E/'src/emu/loader/src/gdr.cpp').read_text()
    patched=raw.replace('static constexpr std::uint32_t FNTTRAN_VERSION_INLINE_METRICS = 40;', 'static constexpr std::uint32_t FNTTRAN_VERSION_INLINE_METRICS = 40;\n    static bool w54_version42 = false;').replace('read += stream->read(&the_header.fnt_tran_version_, sizeof(the_header.fnt_tran_version_));', 'read += stream->read(&the_header.fnt_tran_version_, sizeof(the_header.fnt_tran_version_));\n        w54_version42 = the_header.fnt_tran_version_ == 42;').replace('read += stream->read(&header.bitmap_encoding_, sizeof(header.bitmap_encoding_));', 'if (w54_version42) { char extra[5]; if (stream->read(extra, 5) != 5) return false; }\n        read += stream->read(&header.bitmap_encoding_, sizeof(header.bitmap_encoding_));')
    (E/'gdr42.cpp').write_text(patched)
    LOG['sources'].append({'path':str(E/'gdr42.cpp'),'original_sha256':sha(raw.encode()),'compiled_sha256':sha(patched.encode()),'patch':''.join(difflib.unified_diff(raw.splitlines(True),patched.splitlines(True))), 'purpose':'External v42 header extension per Nokia runtime; original metric and bitmap decoder unchanged. This is not upstream EKA2L1 v42 support.'})
    run(['g++','-std=c++17','-O2','-I'+str(E/'shim'),'-I'+str(E/'src/emu/loader/include'),E/'gdr42.cpp',E/'shim/main.cpp','-licuuc','-licudata','-o',E/'gdr42-oracle'])


def adapter(*args): return run([EXE,*args])
def dump(fmt,path): return json.loads(adapter('dump',fmt,path))
def pixels(g):
    if g['gray']: return g['bytes']
    return [255*((g['bytes'][y*((g['width']+7)//8)+x//8]>>(7-x%8))&1) for y in range(g['height']) for x in range(g['width'])]
def coords(g): return {(g['x']+x,g['y']+g['height']-1-y):p for y in range(g['height']) for x in range(g['width']) if (p:=pixels(g)[y*g['width']+x])}
def mapped(font): return {c:g for g in font['glyphs'] for c in g['codes']}
def compare(a,b,geometry=True):
    assert len(a)==len(b),(len(a),len(b))
    for fa,fb in zip(a,b):
        ma,mb=mapped(fa),mapped(fb)
        assert ma.keys()==mb.keys(),(ma.keys(),mb.keys())
        for c,ga in ma.items():
            gb=mb[c]
            assert coords(ga)==coords(gb),(c,coords(ga),coords(gb))
            assert ga['advance']==gb['advance'],(c,ga,gb)
            if geometry: assert tuple(ga[k] for k in ['width','height','x','y'])==tuple(gb[k] for k in ['width','height','x','y'])

def monobit_dump(path,format):
    import monobit
    fonts=monobit.load(path,format=format)
    result=[]
    for f in fonts:
        glyphs=[]
        for g in f.glyphs:
            glyphs.append({'codes':[int(c) for c in g.codepoints], 'width':g.width,'height':g.height,'x':g.left_bearing,'y':g.shift_up,'advance':g.advance_width,'gray':True,'bytes':[int(x)*255 for row in g.as_matrix() for x in row]})
        result.append({'glyphs':glyphs,'ascent':f.ascent,'descent':f.descent})
    return result

def prc(records):
    b=bytearray(78+10*len(records)+2);b[:8]=b'Original';b[32:34]=(1).to_bytes(2,'big');b[60:68]=b'FontFont';b[76:78]=len(records).to_bytes(2,'big')
    for i,(tag,id,data) in enumerate(records):
        at=78+10*i;b[at:at+4]=tag;b[at+4:at+6]=id.to_bytes(2,'big');b[at+6:at+10]=len(b).to_bytes(4,'big');b.extend(data)
    return b

def gray_writer_check(path):
    # Independent index/header validation plus Pillow's 4-bpp unpacker.
    from PIL import Image
    raw=path.read_bytes();n=int.from_bytes(raw[76:78],'big');r={}
    entries=[(raw[78+i*10:82+i*10],int.from_bytes(raw[82+i*10:84+i*10],'big'),int.from_bytes(raw[84+i*10:88+i*10],'big')) for i in range(n)]
    for i,(tag,id,at) in enumerate(entries):r[(tag,id)]=raw[at:entries[i+1][2] if i+1<n else len(raw)]
    h=next(v for (t,_),v in r.items() if t==b'GrFn');first,last=struct.unpack_from('>HH',h,2);ra,ga=struct.unpack_from('>HH',h,18);height=int.from_bytes(h[8:10],'big');asc=int.from_bytes(h[10:12],'big',signed=True);glyphs=[]
    for code in range(first,last+1):
        left,adv,width,rn,ix,_=struct.unpack_from('>hhHHHH',h,ga+12*(code-first))
        if not rn:continue
        rid=int.from_bytes(h[ra+8*(rn-1)+4:ra+8*(rn-1)+6],'big');data=r[(b'GU34',rid)];off,length=struct.unpack_from('>HH',data,ix*4);image=data[off:off+length];w,hh,stride,flags=struct.unpack_from('>HHHH',image);assert(w,hh)==(width,height);assert flags&0x8000==0 and image[8:10]==bytes([4,3]);assert image[10]==24
        values=[]
        for y in range(hh):
            row=image[24+y*stride:24+(y+1)*stride]
            im=Image.frombytes('L',(stride*2,1),row,'raw','L;4')
            values.extend(list(im.tobytes())[:w])
        glyphs.append(dict(codes=[code],width=w,height=hh,x=left,y=asc-hh,advance=adv,gray=True,bytes=values))
    return [{'glyphs':glyphs}]

def fixtures():
    adapter('emit',FIX)
    (FIX/'original.bdf').write_text(BDF)
    (FIX/'original.gd').write_text(GD)
    run([P/'fnttran',FIX/'original.bdf',FIX/'original.gd',FIX/'external.gdr'])
    # Original deterministic outline; third-party proprietary fonts are unnecessary.
    from fontTools.fontBuilder import FontBuilder
    from fontTools.pens.ttGlyphPen import TTGlyphPen
    fb=FontBuilder(1000,isTTF=True);fb.setupGlyphOrder(['.notdef','A','C']);fb.setupCharacterMap({65:'A',67:'C'});gs={}
    for name in ['.notdef','A','C']:
        pen=TTGlyphPen(None)
        if name!='.notdef': pen.moveTo((0,0));pen.lineTo((500,700));pen.lineTo((600,0));pen.closePath()
        gs[name]=pen.glyph()
    fb.setupGlyf(gs);fb.setupHorizontalMetrics({n:(650,0) for n in gs});fb.setupHorizontalHeader(ascent=800,descent=-200);fb.setupNameTable({'familyName':'Original','styleName':'Regular','uniqueFontIdentifier':'w54-original','fullName':'Original','psName':'Original'});fb.setupOS2(sTypoAscender=800,sTypoDescender=-200,usWinAscent=800,usWinDescent=200);fb.setupPost();fb.setupMaxp();fb.font['head'].created=fb.font['head'].modified=2082844800;fb.save(FIX/'original.ttf')
    run([F/'topalmtext','-g','-f-',FIX/'original.ttf','8','gray'],output=FIX/'gray-original.txt')
    out=P/'gray-out';out.mkdir(exist_ok=True)
    run([F/'togray','-W'+str(out),'0080',FIX/'gray-original.txt','GU14','GU34','GL14','GL34','GR14','GR34'])
    rs=[]
    for p in sorted(out.glob('*.bin')):rs.append((p.stem[:4].encode(),int(p.stem[4:],16),p.read_bytes()))
    (out/'external-gray.prc').write_bytes(prc(rs))


def checks():
    for profile,format in [('v1','nfnt'),('v2','nfnt2'),('pdb','palm'),('prc','palm-prc')]:
        src=json.loads((FIX/f'{profile}.source.json').read_text()); got=monobit_dump(FIX/f'{profile}.bin',format);compare(src,got);compare(src,dump('palm',FIX/f'{profile}.bin'));LOG['checks'].append(profile+' full pixels/metrics/codes monobit and rust')
    # FontConv's own afnx input/output confirms offset/advance byte order.
    for profile in ['afnx']:
        run([F/'fontconv','-l',FIX/f'{profile}.bin','t',FIX/f'{profile}.txt'])
        txt=(FIX/f'{profile}.txt').read_text();assert "// WIDTH 2 (OFFSET 0, DELTA 2)\nGLYPH 'A'\n##\n-#\n##\n--\n#-\n--" in txt;assert "GLYPH 'C'\n-#\n##\n#-\n#-\n##\n--" in txt
        compare(json.loads((FIX/f'{profile}.source.json').read_text()),dump('palm',FIX/f'{profile}.bin'))
    for flag,profile in [('1a','external-afnx'),('1','external-nfnt2')]:
        run([F/'fontconv',FIX/'v1.bin',flag,FIX/f'{profile}.bin']);compare(json.loads((FIX/'v1.source.json').read_text()),dump('palm',FIX/f'{profile}.bin'))
    temp=P/'subst';temp.mkdir(exist_ok=True)
    for suffix in ['0000','0001','0002','0007']: shutil.copy(FIX/'fontsubst.bin',temp/f'afnx{suffix}.bin')
    run([F/'fsconvert',str(temp)+'/', '-'])
    assert (temp/'afnx7d00.bin').read_bytes()==(FIX/'afnx.bin').read_bytes()
    for suffix in ['7d00','7d01','7d02','7d03']: shutil.copy(FIX/'afnx.bin',temp/f'afnx{suffix}.bin')
    run([F/'fsconvert',str(temp)+'/', 'Original'])
    assert (temp/'afnx0000.bin').read_bytes()==(FIX/'fontsubst.bin').read_bytes()
    LOG['checks'].append('FontConv afnx/FontSubst reader and native afnx/nfnt2 writer, all mapped glyphs')
    ma=monobit_dump(FIX/'afnx.bin','afnx');assert mapped(ma[0])[65]['advance']==0
    LOG['known_oracle_differences'].append('monobit 0.54 afnx swaps one-byte offset/advance fields; its advance 0 disagrees with original FontConv advance 2. FontConv validates afnx.')
    src=json.loads((FIX/'double.source.json').read_text()); mb=monobit_dump(FIX/'double.bin','nfnt2');assert 66 in mapped(mb[1]);mb[1]['glyphs']=[g for g in mb[1]['glyphs'] if 66 not in g['codes']];compare(src,mb);compare(src,dump('palm',FIX/'double.bin'))
    LOG['known_oracle_differences'].append('monobit 0.54 high-density missing ff/ff entry becomes phantom code 66: sentinels are tested after scaling. Compare all real codes and assert this extra explicitly.')
    for profile in ['gray1','gray2']:
        src=json.loads((FIX/f'{profile}.source.json').read_text());compare(src,gray_writer_check(FIX/f'{profile}.bin'));compare(src,dump('palm',FIX/f'{profile}.bin'))
    gray=dump('palm',P/'gray-out/external-gray.prc');assert len(gray)==6
    alphabet='-123456789ABCDE#';rows=['---C2','--7#4','-3##7','-D##9','A###B','-----'];expected=[alphabet.index(c)*17 for row in rows for c in row]
    for f in gray:
        assert mapped(f).keys()=={65,67}
        for g in mapped(f).values():assert (g['width'],g['height'],g['x'],g['y'],g['advance'])==(5,6,0,0,5);assert pixels(g)==expected
    LOG['checks'].append('FontConv original six GrayFont profiles, rotations, compression and aliased bitmap indexes; Gray1/2 writer independently unpacked by Pillow')
    for profile in ['gdr39','gdr40','gdr41','gdr42','gdr-large','gdr-unicode']:
        src=json.loads((FIX/f'{profile}.source.json').read_text());native=json.loads(run([E/('gdr42-oracle' if profile=='gdr42' else 'gdr-oracle'),FIX/f'{profile}.bin']))
        for f in native:
            for g in f['glyphs']:g.update(codes=[g.pop('code')],gray=True,bytes=[v*255 for v in g.pop('pixels')])
        compare(src,native);compare(src,dump('gdr',FIX/f'{profile}.bin'))
        if profile=='gdr-unicode': assert native[0]['family_units']==list(struct.unpack('<'+'H'*(len(src[0]['family'].encode('utf-16-le'))//2),src[0]['family'].encode('utf-16-le')))
    src=json.loads((FIX/'gdr41.source.json').read_text());compare(src,dump('gdr',FIX/'external.gdr'),geometry=False)
    LOG['checks'].append('GDR39/40/41 writer all pixels/mappings/metrics checked by unmodified EKA2L1 loader; original Symbian fnttran41 output decoded with equivalent positioned ink and advances')
    compare(json.loads((FIX/'gdr42.source.json').read_text()),dump('gdr',FIX/'gdr42.bin'))
    LOG['checks'].append('GDR42 full pixels/metrics/codes checked by external EKA2L1 decoder with documented five-byte header extension; original native bitmap decoder unchanged; this extension is not upstream support')
    import monobit
    LOG['oracle_versions']={'python':sys.version,'monobit':monobit.__version__}
    for name in ['monobit.storage.fontformats.apple.palm','monobit.storage.fontformats.apple.nfnt','monobit.storage.fontformats.apple.nfnt2','monobit.storage.fontformats.apple.grayfont']:
        module=importlib.import_module(name);p=Path(module.__file__);LOG['sources'].append({'module':name,'path':str(p),'sha256':sha(p.read_bytes())})
    LOG['known_oracle_differences'].append('monobit GrayFont module has no registered complete reader and no writer; monobit has no GDR codec. Neither is claimed as oracle support.')

SHIMS = {'main.cpp': '#include <common/buffer.h>\n#include <loader/gdr.h>\n#include <fstream>\n#include <iostream>\nint main(int argc,char**argv){if(argc!=2)return 2;eka2l1::common::ro_stream in;std::ifstream f(argv[1],std::ios::binary);in.b.assign(std::istreambuf_iterator<char>(f),{});eka2l1::loader::gdr::file_store s;if(!eka2l1::loader::gdr::parse_store(&in,s))return 3;\nstd::cout<<"[";bool a=false;for(auto&st:s.font_bitmaps_){if(a)std::cout<<",";a=true;std::cout<<"{\\"family_units\\":[";bool first_unit=true;for(auto& face:s.typefaces_){for(auto& ref:face.header_.bitmap_headers_)if(ref.uid_==st.header_.uid_){for(auto unit:face.header_.name_){if(!first_unit)std::cout<<",";first_unit=false;std::cout<<unsigned(unit);}break;}if(!first_unit)break;}std::cout<<"],\\"ascent\\":"<<int(st.header_.ascent_in_pixels_)<<",\\"size\\":"<<int(st.header_.cell_height_in_pixels_)<<",\\"glyphs\\":[";bool b=false;for(auto&sec:st.code_sections_){for(size_t i=0;i<sec.chars_.size();i++){auto&g=sec.chars_[i];if(g.metric_==&st.filler_metric_)continue;auto&m=*g.metric_;int w=m.move_in_pixels_-m.left_adj_in_pixels_-m.right_adjust_in_pixels_;if(b)std::cout<<",";b=true;std::cout<<"{\\"code\\":"<<sec.header_.start_+i<<",\\"width\\":"<<w<<",\\"height\\":"<<int(m.height_in_pixels_)<<",\\"x\\":"<<int(m.left_adj_in_pixels_)<<",\\"y\\":"<<m.ascent_in_pixels_-m.height_in_pixels_<<",\\"advance\\":"<<int(m.move_in_pixels_)<<",\\"pixels\\":[";for(int j=0;j<w*m.height_in_pixels_;j++){if(j)std::cout<<",";std::cout<<((g.data_[j/32]>>(j%32))&1);}std::cout<<"]}";}}std::cout<<"]}";}std::cout<<"]\\n";}\n', 'utils/des.h': '#pragma once\n#include <common/buffer.h>\n#include <unicode/ucnv.h>\nnamespace eka2l1::epoc {\ninline bool read_des_string(std::u16string& out,common::ro_stream*s,bool) {\n uint8_t v=0;if(s->read(&v,1)!=1)return false; uint32_t tag=v;\n if(!(v&1))tag>>=1;else if(!(v&2)){uint8_t z;s->read(&z,1);tag=(tag|(z<<8))>>2;}else if(!(v&4)){uint8_t z[3];s->read(z,3);tag=(tag|(z[0]<<8)|(z[1]<<16)|(uint32_t(z[2])<<24))>>3;}else return false;\n size_t n=tag>>1;out.resize(n);if(tag&1){for(auto&c:out){uint8_t z;if(s->read(&z,1)!=1)return false;c=z;}return true;}\n UErrorCode err=U_ZERO_ERROR;auto cv=ucnv_open("SCSU",&err);if(U_FAILURE(err))return false;\n const char* src=s->b.data()+s->at;const char* begin=src;UChar* target=(UChar*)out.data();\n ucnv_toUnicode(cv,&target,target+n,&src,s->b.data()+s->b.size(),nullptr,false,&err);\n s->at+=src-begin;ucnv_close(cv);return (err==U_BUFFER_OVERFLOW_ERROR||U_SUCCESS(err))&&target==(UChar*)out.data()+n;\n}\n}\n', 'common/buffer.h': '#pragma once\n#include <algorithm>\n#include <cstring>\n#include <cstdint>\n#include <string>\n#include <vector>\nnamespace eka2l1::common {\nstruct ro_stream { std::vector<char> b; size_t at=0; size_t read(void* p,size_t n) { n=std::min(n,b.size()-at); std::memcpy(p,b.data()+at,n);at+=n;return n; } };\ninline std::u16string lowercase_ucs2_string(std::u16string s) { for(auto& c:s)if(c>=65&&c<=90)c+=32;return s; }\ninline int compare_ignore_case(const std::u16string&a,const std::u16string&b) { return lowercase_ucs2_string(a).compare(lowercase_ucs2_string(b)); }\n}\n'}
BDF = 'STARTFONT 2.1\nFONT Original\nSIZE 6 72 72\nFONTBOUNDINGBOX 2 6 0 -2\nSTARTPROPERTIES 3\nUid 268435457\nFONT_ASCENT 4\nFONT_DESCENT 2\nENDPROPERTIES\nCHARS 2\nSTARTCHAR A\nENCODING 65\nDWIDTH 2 0\nBBX 2 6 0 -2\nBITMAP\nC0\n40\nC0\n00\n80\n00\nENDCHAR\nSTARTCHAR C\nENCODING 67\nDWIDTH 2 0\nBBX 2 6 0 -2\nBITMAP\n40\nC0\n80\n80\nC0\n00\nENDCHAR\nENDFONT\n'
GD = 'Typeface OriginalType\nName "Original"\nFontBitmaps\nOriginal\nEndFontBitmaps\nEndTypeface\nFontStoreFile\nCollectionUid 268435458\nKPixelAspectRatio 1000\nCopyrightInfo\n"Original MIT synthetic fixture"\nEndCopyrightInfo\nTypefaces\nOriginalType\nEndTypefaces\nEndFontStoreFile\n'

if __name__=='__main__':
    ap=argparse.ArgumentParser();ap.add_argument('--setup',action='store_true');ap.add_argument('--exe',default=str(WORK/'target-w54/debug/examples/w54_timing'));args=ap.parse_args();EXE=Path(args.exe)
    try:
        if args.setup:
            setup()
            (ROOT/'reports/w5.4-oracles.json').write_text(json.dumps({'sources':LOG['sources'],'commands':LOG['commands']},indent=2)+'\n')
        elif (ROOT/'reports/w5.4-oracles.json').exists():
            saved=json.loads((ROOT/'reports/w5.4-oracles.json').read_text());LOG['sources'].extend(saved['sources'])
        fixtures();checks();LOG['status']='pass'
    except Exception as e:
        LOG['status']='fail';LOG['error']=repr(e);raise
    finally:
        LOG['executables']=[{'path':str(p),'sha256':sha(p.read_bytes())} for p in [EXE,P/'fnttran',F/'fontconv',F/'fsconvert',F/'togray',F/'topalmtext',E/'gdr-oracle',E/'gdr42-oracle'] if p.exists()]
        LOG['artifacts']=[{'path':str(p.relative_to(WORK)),'sha256':sha(p.read_bytes()),'bytes':p.stat().st_size} for d in [FIX,P/'gray-out',E/'shim'] for p in sorted(d.rglob('*')) if p.is_file()]
        (ROOT/'reports/w5.4-crosscheck.json').write_text(json.dumps(LOG,indent=2)+'\n')
