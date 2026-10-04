#!/usr/bin/env python3
"""Native NFNT/containers versus MIT monobit; independent wire and public-domain ufond samples."""
import argparse,hashlib,importlib.util,json,pathlib,subprocess
import monobit
p=argparse.ArgumentParser();p.add_argument('--driver',type=pathlib.Path,required=True);p.add_argument('--work',type=pathlib.Path,required=True);args=p.parse_args();args.work.mkdir(parents=True,exist_ok=True)
root=pathlib.Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('seed_nfnt',root/'fuzz/seed_nfnt.py');seed=importlib.util.module_from_spec(spec);spec.loader.exec_module(seed)
checks=[]
def native(path):return json.loads(subprocess.check_output([str(args.driver),'inspect',str(path)]))
def oracle(path,fmt):
    fonts=monobit.load(path,format=fmt)
    return [dict(ascent=f.ascent,descent=f.descent,glyphs=[dict(codes=[int(c) for c in g.codepoints],width=g.width,height=g.height,x=g.left_bearing,y=g.shift_up,advance=g.advance_width,pixels=g.as_bytes().hex()) for g in f.glyphs]) for f in fonts]
def compare(path,fmt):
    assert subprocess.check_output([str(args.driver),'detect',str(path)],text=True).strip()!='NoMatch',(path,'NFNT detection')
    a=native(path)['fonts'];b=oracle(path,fmt);assert len(a)==len(b),(path,'font count')
    for i,(a,b) in enumerate(zip(a,b)):
        for k in ['ascent','descent','glyphs']:assert a[k]==b[k],(path,i,k,a[k],b[k])
    checks.append(dict(file=path.name,sha256=hashlib.sha256(path.read_bytes()).hexdigest(),fonts=len(native(path)['fonts']),glyphs=sum(len(f['glyphs']) for f in native(path)['fonts'])))
    return monobit.load(path,format=fmt)
for index in range(24):
    path=args.work/f'generated-{index}.nfnt';path.write_bytes(seed.raster(index));fs=compare(path,'nfnt')
    # The independent encoder exercises optional fractional-width tables.
    oracle_path=args.work/f'oracle-{index}.nfnt';monobit.save(fs,oracle_path,format='nfnt',overwrite=True);compare(oracle_path,'nfnt')
    for mode,fmt in [('bare','nfnt'),('dfont','mac'),('font','mac')]:
        out=args.work/f'native-{index}.{mode}';subprocess.run([str(args.driver),'rewrite',str(path),str(out),mode],check=True,stdout=subprocess.DEVNULL);compare(out,fmt)
classic=args.work/'classic-font.suit';classic.write_bytes(seed.classic_font());fs=compare(classic,'mac')
assert native(classic)['fonts'][0]['family']==fs[0].family=='Griffin'
assert native(classic)['fonts'][0]['point']==fs[0].point_size==10
for name,raw in seed.seeds():
    if name.startswith(('macbinary-','apple-')) or name in ('multi-strike.dfont','original.dfont'):
        path=args.work/name;path.write_bytes(raw);compare(path,'mac' if name.endswith('.dfont') else '')
for name in ['4x6.dfont','4x6.bin']:
    path=args.work/name
    if path.exists():
        compare(path,'mac' if name.endswith('.dfont') else '')
# CiderPress2's .as stores the dfont in data fork entry 1.
apple=args.work/'4x6.dfont.as'
if apple.exists():
    compare(apple,'')
    assert native(apple)==native(args.work/'4x6.dfont')
# monobit 0.54's NFNT reader confuses fontType bit 7 (fctb) with Lisa compression.
# Check the ColorTable independently through its low-level oracle and Pillow BT.601.
from monobit.storage import Stream
from monobit.storage.fontformats.apple.fctb import extract_fctb,convert_fctb
from PIL import Image
raw,palette=seed.indexed_palette();path=args.work/'indexed-color.dfont';path.write_bytes(raw)
assert subprocess.check_output([str(args.driver),'detect',str(path)],text=True).strip()!='NoMatch'
ct=convert_fctb(**extract_fctb(Stream.from_data(palette,mode='r')),levels=4)
image=Image.frombytes('P',(4,1),bytes([0,1,2,3]));image.putpalette([component for rgb in ct for component in rgb]+[0]*(768-12))
coverage=bytes(255-v for v in image.convert('L').tobytes())
assert native(path)['fonts'][0]['glyphs'][0]['pixels']==coverage.hex()
checks.append(dict(file=path.name,sha256=hashlib.sha256(raw).hexdigest(),fonts=1,glyphs=2,oracle='monobit ColorTable + Pillow luminance'))
print(json.dumps(dict(oracle=monobit.__version__,checks=len(checks),detection_checks=len(checks),palette_reader_limitation='monobit 0.54 NFNT treats fctb flag as Lisa compression; ColorTable and Pillow checked independently',cases=checks),indent=2))
