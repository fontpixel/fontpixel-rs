#!/usr/bin/env python3
"""W5.5 repeatable native C consumers and fixed-release monobit crosschecks.

Downloaded upstream runtime files remain in FONTPIXEL_WORK_ROOT/w55. Consumers
link upstream decoding routines unchanged; tiny host drawing/memory hooks capture
pixels rather than accessing embedded display hardware. No supplied font source
is ever compiled by the Rust library; only this test compiles project-authored
writer output and fixed upstream test fonts against trusted upstream runtimes.
"""
import argparse, hashlib, json, os, pathlib, subprocess, sys, urllib.request, tarfile
ROOT=pathlib.Path(__file__).resolve().parents[2]
WORK=pathlib.Path(os.environ.get('FONTPIXEL_WORK_ROOT',ROOT.parent/'local-work'))/'w55'
EXPECTED_HASHES={'u8g2_font.c': '3d205ff43e0da767aaf87787777ef38ae930a876702c560e7d0e60183ad9730a', 'u8g2.h': 'c85d1926d21514bbe1503b29b6c6a46b264c78259ddbd82c82177467d901f971', 'u8x8.h': 'd8fba514283df6974861ac8b5551b36eb3f2e72488b6a4c0e0c83bd2d032c796', 'u8x8_8x8.c': 'da32dff05050e4b18ea9007fdefc49ef31bf24b85ba2200fa791755aa81b2884', 'u8g_font.c': '436e251dc0f45ef771d0dea543da5091e896c680abdb06c2056f48074e2837a1', 'u8g.h': '7a4a7fb1b302c20c30a1855de0f31262eb2662f0720ca45eed89e72f78bbce1b', 'lvgl-v8.3.11.tar.gz': '83b7325a4b78cbb19a39f3ac75346c8086926d7faeab5ff2d825c20da0bbdb46', 'u8g2_example.c': '48f17be689f0ce9dec06075e5830a6de2c470e3761d2226ae7fa80c1959d2bf6', 'spleen5x8.h': '8ac9133b8ba0e83a524651b38eac8a94d51a623673f3b55199e947d9ba69ebd5'}

EXPECTED_HASHES.update({'u8x8-legacy-runtime/u8x8_8x8.c': 'fe885f590784b34ced1cfec7f5b12e1a3427f12b37579167e2c43ec0f83b42e4', 'u8x8-legacy-runtime/u8x8.h': 'ae618fc17c72a6c42416119523a70ae50ad954aa5d9f3ef5dc9cece0c4c6337f'})

def run(argv,**kwargs):
    p=subprocess.run([str(x) for x in argv],text=True,capture_output=True,**kwargs)
    if p.returncode: raise RuntimeError(f'{argv}\nexit={p.returncode}\n{p.stdout}\n{p.stderr}')
    return p.stdout

def fetch(name,url):
    p=WORK/name
    p.parent.mkdir(parents=True,exist_ok=True)
    if not p.exists():p.write_bytes(urllib.request.urlopen(url).read())
    assert hashlib.sha256(p.read_bytes()).hexdigest()==EXPECTED_HASHES[name],f"upstream content changed: {name}"
    return p

def normalized(value):
    return sorted(value['glyphs'],key=lambda g:g['codes'])

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--bridge',type=pathlib.Path,default=ROOT.parent/'local-work/target-w55/debug/examples/w55_timing');parser.add_argument('--out',type=pathlib.Path,default=ROOT/'reports/w5.5-crosscheck.json');a=parser.parse_args();WORK.mkdir(parents=True,exist_ok=True)
    urls={
        'u8x8-legacy-runtime/u8x8_8x8.c':'https://raw.githubusercontent.com/olikraus/u8g2/b2d84613a8ea258049c8aa7be816acd4139bfcc7/csrc/u8x8_8x8.c',
        'u8x8-legacy-runtime/u8x8.h':'https://raw.githubusercontent.com/olikraus/u8g2/b2d84613a8ea258049c8aa7be816acd4139bfcc7/csrc/u8x8.h',
        'u8g2_font.c':'https://raw.githubusercontent.com/olikraus/u8g2/d6c8499c5f2707cac8eccd09fd8f677d12b17977/csrc/u8g2_font.c',
        'u8g2.h':'https://raw.githubusercontent.com/olikraus/u8g2/d6c8499c5f2707cac8eccd09fd8f677d12b17977/csrc/u8g2.h',
        'u8x8.h':'https://raw.githubusercontent.com/olikraus/u8g2/d6c8499c5f2707cac8eccd09fd8f677d12b17977/csrc/u8x8.h',
        'u8x8_8x8.c':'https://raw.githubusercontent.com/olikraus/u8g2/d6c8499c5f2707cac8eccd09fd8f677d12b17977/csrc/u8x8_8x8.c',
        'u8g_font.c':'https://raw.githubusercontent.com/olikraus/u8glib/c5aac51b301ff82b5bfd06c35edf7cb819e00177/csrc/u8g_font.c',
        'u8g.h':'https://raw.githubusercontent.com/olikraus/u8glib/c5aac51b301ff82b5bfd06c35edf7cb819e00177/csrc/u8g.h',
        'u8g2_example.c':'https://raw.githubusercontent.com/olikraus/u8g2/d6c8499c5f2707cac8eccd09fd8f677d12b17977/tools/font/build/single_font_files/u8g2_font_5x7_tf.c',
        'spleen5x8.h':'https://raw.githubusercontent.com/NetBSD/src/trunk/sys/dev/wsfont/spleen5x8.h',
        'lvgl-v8.3.11.tar.gz':'https://codeload.github.com/lvgl/lvgl/tar.gz/refs/tags/v8.3.11',
    }
    provenance={}
    for name,url in urls.items():
        p=fetch(name,url);provenance[name]={'url':url,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()}
    lv=WORK/'lvgl-8.3.11'
    if not lv.exists():
        with tarfile.open(WORK/'lvgl-v8.3.11.tar.gz') as archive:archive.extractall(WORK,filter='data')
    common=['cc','-std=c11','-O2','-ffunction-sections','-fdata-sections',f'-I{WORK}']
    for source in ['u8g2_font.c','u8g_font.c','u8x8_8x8.c']:
        run(common+['-c',WORK/source,'-o',WORK/(source+'.o')])
    run(common+['-DLV_CONF_SKIP','-DLV_USE_FONT_COMPRESSED=1',f'-I{lv}','-c',lv/'src/font/lv_font_fmt_txt.c','-o',WORK/'lv-runtime.o'])
    run(common+['-DLV_CONF_SKIP',f'-I{lv}','-c',lv/'src/font/lv_font_loader.c','-o',WORK/'lv-loader.o'])
    fixtures={};results={}
    for kind in ['u8g2','u8glib','u8glib-compact','u8glib-gray','u8x8','u8x8-legacy','netbsd','drcs','lvgl','lvgl-rle','lvgl-xor','lvgl2-rle','lvgl4-xor','lvgl8','lvgl4-binary-xor']:
        path=WORK/('fixture-'+kind+'.h');run([a.bridge,'fixture',kind,path]);fixtures[kind]=path
        expected=json.loads(run([a.bridge,'inspect',kind,path]));results[kind]={'glyphs':len(expected['glyphs']),'bytes':path.stat().st_size,'sha256':hashlib.sha256(path.read_bytes()).hexdigest()}
        if kind in ['u8g2','u8glib','u8glib-compact','u8glib-gray','u8x8']:
            code=embedded_consumer(kind,path)
            consumer=WORK/(kind+'-consumer.c');consumer.write_text(code)
            obj='u8g2_font.c.o' if kind=='u8g2' else 'u8x8_8x8.c.o' if kind=='u8x8' else 'u8g_font.c.o'
            exe=WORK/(kind+'-consumer');run(common+[consumer,WORK/obj,'-Wl,--gc-sections','-o',exe]);actual=json.loads(run([exe]));assert normalized(actual)==normalized(expected),(kind,actual,expected);results[kind]['native_consumer']='pass'
        if kind=='u8x8-legacy':
            legacy=WORK/'u8x8-legacy-runtime';code=embedded_consumer('u8x8',path).replace('extern void u8x8_get_glyph_data(u8x8_t*,uint8_t,uint8_t*,uint8_t);',f'#include "{legacy}/u8x8_8x8.c"').replace('u8x8_get_glyph_data(&u,cp,buf,0)','u8x8_get_glyph_data(&u,cp,buf)')
            consumer=WORK/'u8x8-legacy-consumer.c';consumer.write_text(code);exe=WORK/'u8x8-legacy-consumer';run(['cc',f'-I{legacy}']+common[1:]+[consumer,'-Wl,--gc-sections','-o',exe]);actual=json.loads(run([exe]));assert normalized(actual)==normalized(expected),(kind,actual,expected);results[kind]['native_consumer']='pass (2018 two-byte header runtime)'
        if kind.startswith('lvgl'):
            consumer=WORK/(kind+'-consumer.c');consumer.write_text(lvgl_consumer(path, 'binary' in kind));exe=WORK/(kind+'-consumer')
            run(common+['-DLV_CONF_SKIP','-DLV_USE_FONT_COMPRESSED=1',f'-I{lv}',consumer,WORK/'lv-runtime.o',*([WORK/'lv-loader.o'] if 'binary' in kind else []),'-Wl,--gc-sections','-o',exe]);actual=json.loads(run([exe]));assert normalized(actual)==normalized(expected),(kind,actual,expected);results[kind]['native_consumer']='pass'
    import monobit, importlib.metadata
    for kind,fmt in [('drcs','dec'),('netbsd','netbsd')]:
        f=monobit.load(fixtures[kind],format=fmt)[0];expected=json.loads(run([a.bridge,'inspect',kind,fixtures[kind]]))
        for record in expected['glyphs']:
            cp=record['codes'][0];g=f.get_glyph(codepoint=cp)
            pixels=[255*int(v) for row in g.as_matrix() for v in row]
            assert pixels==record['pixels'],(kind,cp,pixels,record['pixels'])
            assert (g.width,g.height,g.left_bearing,g.shift_up,g.advance_width)==(record['width'],record['height'],record['x'],record['y'],record['advance'])
        out=WORK/(kind+'-monobit.h')
        if kind=='drcs':
            try:monobit.save(f,out,format=fmt,overwrite=True)
            except ValueError as exc:
                results[kind]['upstream_metadata_reemit_failure']=str(exc)
                f=monobit.Font(f.glyphs,encoding='ascii')
                monobit.save(f,out,format=fmt,overwrite=True)
        else:monobit.save(f,out,format=fmt,overwrite=True)
        back=json.loads(run([a.bridge,'inspect',kind,out])); back['glyphs']=[g for g in back['glyphs'] if any(cp in (65,66,67) for cp in g['codes'])]; assert normalized(back)==normalized(expected),(kind,back,expected)
        results[kind]['monobit_bidirectional']='pass'
    for kind,path in [('u8g2',WORK/'u8g2_example.c'),('lvgl',lv/'src/font/lv_font_montserrat_14.c'),('netbsd',WORK/'spleen5x8.h')]:
        expected=json.loads(run([a.bridge,'inspect',kind,path]));count=len(expected['glyphs']);expected['glyphs']=[g for g in expected['glyphs'] if g['codes'][0] in (65,66,67)]
        if kind=='netbsd':
            f=monobit.load(path,format='netbsd')[0]
            for record in expected['glyphs']:
                g=f.get_glyph(codepoint=record['codes'][0]);assert [255*int(v) for row in g.as_matrix() for v in row]==record['pixels'];assert (g.width,g.height,g.left_bearing,g.shift_up,g.advance_width)==(record['width'],record['height'],record['x'],record['y'],record['advance'])
        else:
            if kind=='lvgl':code=lvgl_consumer(path).replace('uint8_t *_lv_font_decompr_buf;','#define font lv_font_montserrat_14\nuint8_t *_lv_font_decompr_buf;');obj='lv-runtime.o'
            else:code='#include "u8g2.h"\n'+embedded_consumer(kind,path).replace('static unsigned char pixels','static const uint8_t *font=u8g2_font_5x7_tf;\nstatic unsigned char pixels').replace('ci<4','ci<3');obj='u8g2_font.c.o'
            consumer=WORK/(kind+'-external-consumer.c');consumer.write_text(code);exe=WORK/(kind+'-external-consumer')
            run(common+['-DLV_CONF_SKIP','-DLV_USE_FONT_COMPRESSED=1',f'-I{lv}',consumer,WORK/obj,'-Wl,--gc-sections','-o',exe]);actual=json.loads(run([exe]));assert normalized(actual)==normalized(expected),(kind,actual,expected)
        results[kind+'-upstream-font']={'glyphs_read':count,'glyphs_compared':3,'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'independent_consumer':'monobit' if kind=='netbsd' else 'native','result':'pass'}
    import monobit.storage.fontformats.softfont.dec as dec_oracle
    oracle_hash=hashlib.sha256(pathlib.Path(dec_oracle.__file__).read_bytes()).hexdigest()
    report={'python':sys.version,'monobit':importlib.metadata.version('monobit'),'monobit_dec_sha256':oracle_hash,'runtime_provenance':provenance,'results':results}
    a.out.parent.mkdir(parents=True,exist_ok=True);a.out.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'passed':len(results),'report':str(a.out)}))

PRELUDE='#include <stdio.h>\n#include <string.h>\n#include <stdlib.h>\n'
def embedded_consumer(kind,path):
    capture='static unsigned char pixels[256][256];\n'
    prefix=PRELUDE+f'#include "{path}"\n'+capture
    if kind=='u8g2':
        return prefix+r'''
extern void u8g2_read_font_info(u8g2_font_info_t*,const uint8_t*);
extern const uint8_t *u8g2_font_get_glyph_data(u8g2_t*,uint16_t);
extern int8_t u8g2_font_decode_glyph(u8g2_t*,const uint8_t*);
uint8_t u8g2_IsIntersection(u8g2_t *u,u8g2_uint_t a,u8g2_uint_t b,u8g2_uint_t c,u8g2_uint_t d){return 1;}
void u8g2_DrawHVLine(u8g2_t *u,u8g2_uint_t x,u8g2_uint_t y,u8g2_uint_t n,uint8_t dir){for(unsigned i=0;i<n;i++)pixels[y][x+i]=u->draw_color?255:0;}
int main(void){u8g2_t u={0};u.font=font;u8g2_read_font_info(&u.font_info,font);printf("{\"glyphs\":[");for(unsigned ci=0;ci<4;ci++){unsigned cp=ci==3?0x4e00:65+ci;memset(pixels,0,sizeof pixels);u.draw_color=1;u.font_decode.target_x=64;u.font_decode.target_y=64;u.font_decode.is_transparent=1;int adv=u8g2_font_decode_glyph(&u,u8g2_font_get_glyph_data(&u,cp));int w=u.font_decode.glyph_width,h=u.font_decode.glyph_height,x=u.font_decode.target_x,y=u.font_decode.target_y;if(cp>65)printf(",");printf("{\"codes\":[%u],\"width\":%d,\"height\":%d,\"x\":%d,\"y\":%d,\"advance\":%d,\"pixels\":[",cp,w,h,x-64,64-y-h,adv);for(int i=0;i<w*h;i++)printf("%s%u",i?",":"",pixels[y+i/w][x+i%w]);printf("]}");}printf("]}\n");}
'''
    if kind=='u8x8':
        return prefix+r'''
extern void u8x8_get_glyph_data(u8x8_t*,uint8_t,uint8_t*,uint8_t);
int main(void){u8x8_t u={0};u.font=font;printf("{\"glyphs\":[");for(unsigned cp=65;cp<=67;cp++){uint8_t buf[8];u8x8_get_glyph_data(&u,cp,buf,0);if(cp>65)printf(",");printf("{\"codes\":[%u],\"width\":8,\"height\":8,\"x\":0,\"y\":0,\"advance\":8,\"pixels\":[",cp);for(int i=0;i<64;i++)printf("%s%d",i?",":"",buf[i%8]&(1<<(i/8))?255:0);printf("]}");}printf("]}\n");}
'''
    # U8glib's glyph getter provides native metrics; draw functions provide pixels.
    aa='u8g_draw_aa_glyph' if kind=='u8glib-gray' else 'u8g_draw_glyph'
    return prefix+r'''
extern int8_t u8g_draw_glyph(u8g_t*,u8g_uint_t,u8g_uint_t,uint8_t);
extern int8_t u8g_draw_aa_glyph(u8g_t*,u8g_uint_t,u8g_uint_t,uint8_t);
uint8_t u8g_IsBBXIntersection(u8g_t *u,u8g_uint_t x,u8g_uint_t y,u8g_uint_t w,u8g_uint_t h){return 1;}
void u8g_Draw8Pixel(u8g_t*u,u8g_uint_t x,u8g_uint_t y,uint8_t dir,uint8_t data){for(int i=0;i<8;i++)pixels[y][x+i]=(data&(128>>i))?255:0;}
void u8g_Draw4TPixel(u8g_t*u,u8g_uint_t x,u8g_uint_t y,uint8_t dir,uint8_t data){for(int i=0;i<4;i++)pixels[y][x+i]=((data>>(6-i*2))&3)*85;}
int main(void){u8g_t u={0};u.font=font;printf("{\"glyphs\":[");for(unsigned cp=65;cp<=67;cp++){memset(pixels,0,sizeof pixels);int adv=DRAW(&u,64,64,cp);int w=u.glyph_width,h=u.glyph_height,x=u.glyph_x,y=u.glyph_y;if(cp>65)printf(",");printf("{\"codes\":[%u],\"width\":%d,\"height\":%d,\"x\":%d,\"y\":%d,\"advance\":%d,\"pixels\":[",cp,w,h,x,y,adv);for(int i=0;i<w*h;i++)printf("%s%u",i?",":"",pixels[64-y-h+i/w][64+x+i%w]);printf("]}");}printf("]}\n");}
'''.replace('DRAW(',aa+'(')

def lvgl_consumer(path, binary=False):
    code=PRELUDE+f'#include "{path}"\n'+r'''
uint8_t *_lv_font_decompr_buf;
void *lv_mem_buf_get(uint32_t n){return calloc(n?n:1,1);}
void lv_mem_buf_release(void*p){free(p);}
void *lv_mem_realloc(void*p,size_t n){return realloc(p,n);}
void lv_mem_free(void*p){free(p);}
void *_lv_utils_bsearch(const void*key,const void*base,uint32_t n,uint32_t size,int(*cmp)(const void*,const void*)){return bsearch(key,base,n,size,cmp);}
int main(void){printf("{\"glyphs\":[");for(unsigned cp=65;cp<=67;cp++){lv_font_glyph_dsc_t d; if(!lv_font_get_glyph_dsc_fmt_txt(&font,&d,cp,0))return 3;const uint8_t *p=lv_font_get_bitmap_fmt_txt(&font,cp);if(cp>65)printf(",");printf("{\"codes\":[%u],\"width\":%u,\"height\":%u,\"x\":%d,\"y\":%d,\"advance\":%u,\"pixels\":[",cp,d.box_w,d.box_h,d.ofs_x,d.ofs_y,d.adv_w);for(unsigned i=0;i<d.box_w*d.box_h;i++){unsigned bit=i*d.bpp;unsigned v=(p[bit/8]>>(8-d.bpp-bit%8))&((1u<<d.bpp)-1);printf("%s%u",i?",":"",(v*255+((1u<<d.bpp)-1)/2)/((1u<<d.bpp)-1));}printf("]}");}printf("]}\n");}
'''
    if binary:
        code=code.replace(f'#include "{path}"', '#include <lvgl.h>\n'+r'''void *lv_mem_alloc(size_t n){return calloc(n?n:1,1);}
lv_fs_res_t lv_fs_open(lv_fs_file_t *f,const char *path,lv_fs_mode_t mode){f->file_d=fopen(path,"rb");return f->file_d?LV_FS_RES_OK:LV_FS_RES_UNKNOWN;}
lv_fs_res_t lv_fs_close(lv_fs_file_t *f){fclose(f->file_d);return LV_FS_RES_OK;}
lv_fs_res_t lv_fs_read(lv_fs_file_t *f,void *b,uint32_t n,uint32_t *read){size_t got=fread(b,1,n,f->file_d);if(read)*read=got;return got==n?LV_FS_RES_OK:LV_FS_RES_UNKNOWN;}
lv_fs_res_t lv_fs_seek(lv_fs_file_t *f,uint32_t pos,lv_fs_whence_t how){return fseek(f->file_d,pos,how==LV_FS_SEEK_SET?SEEK_SET:how==LV_FS_SEEK_CUR?SEEK_CUR:SEEK_END)?LV_FS_RES_UNKNOWN:LV_FS_RES_OK;}
''')
        code=code.replace('int main(void){',f'int main(void){{lv_font_t *loaded=lv_font_load("{path}");if(!loaded)return 4;lv_font_t font=*loaded;')
    return code
if __name__=='__main__':main()
