#!/usr/bin/env python3
"""Reproducible W4.5 native/monobit/TeX/Go crosschecks; downloads stay outside repo.

Run with ../local-work/oracles/bin/python tests/crosscheck/w45.py --fetch.
Build Rust bridge with --no-default-features --features pk,gf,plan9 --example w45_timing.
TeX tools are external oracles only. Go dependency is BSD, test-only, pinned v0.24.0.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import urllib.request
import zipfile

ROOT=Path(__file__).resolve().parents[2]
LOCAL=ROOT.parent/'local-work'
GO_SOURCE=r'''package main
import("encoding/json";"fmt";"os";"image/color";"golang.org/x/image/font/plan9font";"golang.org/x/image/math/fixed")
type Glyph struct { Codes []int `json:"codes"`; Bounds []int `json:"bounds"`; Advance []int `json:"advance"`; Pixels string `json:"pixels"` }
func main(){b,e:=os.ReadFile(os.Args[1]);if e!=nil{panic(e)};f,e:=plan9font.ParseSubfont(b,0);if e!=nil{panic(e)};glyphs:=[]Glyph{};for r:=rune(0);;r++ {dr,mask,p,adv,ok:=f.Glyph(fixed.Point26_6{},r);if !ok{break};raster:=make([]byte,((dr.Dx()+7)/8)*dr.Dy());for y:=0;y<dr.Dy();y++{for x:=0;x<dr.Dx();x++{a:=color.AlphaModel.Convert(mask.At(p.X+x,p.Y+y)).(color.Alpha).A;if a!=0{raster[y*((dr.Dx()+7)/8)+x/8]|=128>>uint(x%8)}}};glyphs=append(glyphs,Glyph{[]int{int(r)},[]int{dr.Dx(),dr.Dy(),dr.Min.X,-dr.Max.Y},[]int{int(adv)>>6,0},fmt.Sprintf("%x",raster)})};json.NewEncoder(os.Stdout).Encode(map[string]interface{}{"glyphs":glyphs})}
'''
SOURCES={
 '8x13.0000':('https://raw.githubusercontent.com/9fans/plan9port/master/font/fixed/8x13.0000','d7123170ba85c15aa37dab40413baceef9e59c13b267290b4ae67eec28f94d3d'),
 '4x6.0000':('https://raw.githubusercontent.com/9fans/plan9port/master/font/fixed/4x6.0000','11edb1337d70fe3bf4589be1616b0bcbf49b7d66f7d9cb396fb8c22660370faf'),
 'pk300.zip':('https://mirrors.ibiblio.org/CTAN/fonts/cm/pk/pk300.zip','a4c6dff0c08b815cb3b039afef57af8c0705a9cb3ce6d4172a35744cfdc73137'),
}
def sha(b):return hashlib.sha256(b).hexdigest()
def run(*args,cwd=None):
    p=subprocess.run([str(a)for a in args],cwd=cwd,check=True,capture_output=True,text=True)
    return p.stdout

def fetch(dest):
    for name,(url,digest)in SOURCES.items():
        path=dest/name
        if not path.exists():path.write_bytes(urllib.request.urlopen(url,timeout=45).read())
        assert sha(path.read_bytes())==digest,(path,'source hash differs')
    z=zipfile.ZipFile(dest/'pk300.zip')
    for name in ('cmr10.300pk','cmr10.329pk'):(dest/name).write_bytes(z.read('pk300/'+name))

def points(g):
    w,h,x,y=g['bounds'];raw=bytes.fromhex(g['pixels']);stride=(w+7)//8
    return {(x+col,y+h-1-row)for row in range(h)for col in range(w)if raw[row*stride+col//8]&(128>>(col%8))}
def compare_ink(a,b):
    assert len(a['glyphs'])==len(b['glyphs'])
    for x,y in zip(a['glyphs'],b['glyphs']):
        assert x['codes']==y['codes'],(x['codes'],y['codes'])
        assert x['advance']==y['advance'],(x['codes'],'advance')
        assert points(x)==points(y),(x['codes'],'pixels')
def compare_exact(a,b):
    assert len(a['glyphs'])==len(b['glyphs'])
    for x,y in zip(a['glyphs'],b['glyphs']):
        for key in ('codes','bounds','advance','pixels'):assert x[key]==y[key],(x['codes'],key)
def monobit_dump(path,fmt):
    import monobit
    font=monobit.load(path,format=fmt)[0]
    glyphs=[]
    for g in font.glyphs:
        pixels=bytearray(((g.width+7)//8)*g.height)
        for y,row in enumerate(g.as_matrix()):
            for x,v in enumerate(row):
                if v:pixels[y*((g.width+7)//8)+x//8]|=128>>(x%8)
        glyphs.append(dict(codes=[int.from_bytes(bytes(g.codepoint),'big')],bounds=[g.width,g.height,g.left_bearing,g.shift_up],advance=[g.advance_width,0],pixels=pixels.hex()))
    return {'glyphs':glyphs}

def main():
    p=argparse.ArgumentParser();p.add_argument('--bridge',type=Path,default=LOCAL/'target-w45/debug/examples/w45_timing');p.add_argument('--tex-bin',type=Path,default=LOCAL/'texlive-oracle/usr/bin');p.add_argument('--go-oracle',type=Path);p.add_argument('--work',type=Path,default=LOCAL/'w45-crosscheck');p.add_argument('--fetch',action='store_true');p.add_argument('--report',type=Path,default=ROOT/'reports/w4.5-crosscheck.json');a=p.parse_args();a.work.mkdir(parents=True,exist_ok=True)
    checks=[];dump=lambda fmt,path:json.loads(run(a.bridge,'dump',fmt,path))
    if a.fetch:fetch(a.work)
    go=a.go_oracle
    if go is None:
        d=a.work/'go-oracle';d.mkdir(exist_ok=True);(d/'go.mod').write_text('module w45plan9oracle\n\ngo 1.23.0\n\nrequire golang.org/x/image v0.24.0\n');(d/'main.go').write_text(GO_SOURCE);run('go','mod','tidy',cwd=d);go=d/'plan9-oracle';run('go','build','-o',go,'.',cwd=d)
    for fmt in ('pk','gf','plan9c'):
        src=a.work/f'native.{fmt}';run(a.bridge,'sample',fmt,src);native=dump(fmt,src)
        if fmt=='plan9c':compare_exact(native,json.loads(run(go,src)));checks.append(dict(check='native Plan 9 -> Go',glyphs=len(native['glyphs'])));continue
        oracle=monobit_dump(src,'pkfont'if fmt=='pk'else'gf');compare_ink(native,oracle);checks.append(dict(check=f'native {fmt} -> monobit',glyphs=len(native['glyphs'])))
        text=run(a.tex_bin/('pktype'if fmt=='pk'else'gftype'),src);(a.work/f'{fmt}type.txt').write_text(text);assert 'Bad ' not in text and 'should be'not in text and '\n!'not in text
        other='gf'if fmt=='pk'else'pk';dest=a.work/f'tex.{other}';run(a.tex_bin/('pktogf'if fmt=='pk'else'gftopk'),src,dest);compare_ink(native,dump(other,dest));checks.append(dict(check=f'native {fmt} -> TeX -> {other} -> native',glyphs=len(native['glyphs'])))
    for name in ('cmr10.300pk','cmr10.329pk'):
        src=a.work/name
        if not src.exists():continue
        native=dump('pk',src);compare_ink(native,monobit_dump(src,'pkfont'));gf=a.work/(name+'.gf');run(a.tex_bin/'pktogf',src,gf);compare_ink(native,dump('gf',gf));repacked=a.work/(name+'.native.pk');run(a.bridge,'convert','pk','pk',src,repacked);compare_exact(native,dump('pk',repacked));run(a.tex_bin/'pktype',repacked);checks.append(dict(check='real CM PK + TeX GF + native roundtrip',name=name,glyphs=len(native['glyphs']),sha256=sha(src.read_bytes())))
    for name in ('8x13.0000','4x6.0000'):
        src=a.work/name
        if not src.exists():continue
        native=dump('plan9',src);compare_exact(native,json.loads(run(go,src)));dest=a.work/(name+'.native');run(a.bridge,'convert','plan9','plan9c',src,dest);compare_exact(native,dump('plan9',dest));compare_exact(native,json.loads(run(go,dest)));checks.append(dict(check='real Plan 9 <-> native, Go both directions',name=name,glyphs=len(native['glyphs']),sha256=sha(src.read_bytes())))
    import monobit
    report=dict(checks=checks,monobit_version=monobit.__version__,go_module='golang.org/x/image v0.24.0 (BSD-3-Clause)',tex_version=run(a.tex_bin/'pktype','--version').splitlines()[0],sources={n:dict(url=u,sha256=h)for n,(u,h)in SOURCES.items()},oracle_limits=['monobit 0.54 has no PK/GF writers or Plan 9 codec','monobit GF adds one blank right column; compare absolute ink + advance','monobit PK ignores preamble physical metrics; TeX tools validate those','Go Plan 9 oracle only supports compressed k1/k2; native raw/legacy/grayscale variants have independent wire tests'],licenses={'CM':'Donald Knuth font license, unmodified upstream bitmaps; https://ctan.org/pkg/cm','Plan 9 fixed':'public domain, https://raw.githubusercontent.com/9fans/plan9port/master/font/fixed/README'},implementation_sha256={str(p.relative_to(ROOT)):sha(p.read_bytes())for name in ('pk','gf','plan9')for p in [ROOT/f'crates/fontpixel/src/formats/{name}.rs',*sorted((ROOT/f'crates/fontpixel/src/formats/{name}').glob('*.rs'))]})
    a.report.parent.mkdir(exist_ok=True);a.report.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(dict(checks=len(checks),report=str(a.report))))
if __name__=='__main__':main()
