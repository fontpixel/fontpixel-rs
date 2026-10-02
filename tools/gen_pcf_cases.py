#!/usr/bin/env python3
"""Generate synthetic PCF byte oracles from the read-only MIT Python reference."""
import argparse, ast, gzip, hashlib, json, random, sys, warnings
from pathlib import Path
sys.dont_write_bytecode=True
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT.parent/'reference/bdfparser/src'))
from bdfparser import Font
from bdfparser.pcf import write_pcf, read_pcf
REFERENCE=ROOT.parent/'reference/bdfparser/tests/test_pcf.py'

def reference_helpers():
    tree=ast.parse(REFERENCE.read_text())
    selected=[]
    for node in tree.body:
        if isinstance(node,ast.Assign) and any(isinstance(t,ast.Name) and t.id.startswith('SYN_') for t in node.targets): selected.append(node)
        elif isinstance(node,ast.FunctionDef) and node.name in ('bare_font','meta','random_font'):selected.append(node)
    namespace={'Font':Font,'random':random}
    exec(compile(ast.Module(body=selected,type_ignores=[]),str(REFERENCE),'exec'),namespace)
    return namespace

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--check',action='store_true');args=parser.parse_args()
    helpers=reference_helpers();cases=[]
    warnings.simplefilter('ignore')
    sources={name:helpers[name] for name in ['SYN_TERMINAL','SYN_PROPORTIONAL','SYN_UNENCODED_ONLY','SYN_TWO_ROWS']}
    layouts=[dict(byteorder=by,bitorder=bi,glyphpad=p,scanunit=u) for by in ('msb','lsb') for bi in ('msb','lsb') for p in (1,2,4,8) for u in (1,2,4) if by==bi or u<=p]
    for name,source in sources.items():
        for layout in layouts:
            font=Font().load_string(source);wire=write_pcf(font,**layout)
            cases.append(dict(name=name,source=source,options=layout,pcf=wire.hex(),bdf=read_pcf(wire).tobdf()))
    for seed in range(128):
        source=helpers['random_font'](seed).tobdf();font=Font().load_string(source);wire=write_pcf(font)
        cases.append(dict(name=f'random-{seed}',source=source,options={},pcf=wire.hex(),bdf=read_pcf(wire).tobdf()))
    # Exercise BDF source quirks (including malformed hex) through actual semantics.
    for c in json.loads((ROOT/'crates/fontpixel/tests/data/bdf_reference.json').read_text())[:204]:
        if 'parse_error' in c:continue
        case={'name':'bdf-edge-'+c['name'],'source':c['source'],'options':{}}
        font=Font().load_string(c['source'])
        try:
            wire=write_pcf(font);case.update(pcf=wire.hex(),bdf=read_pcf(wire).tobdf())
        except Exception as e:case['error']=str(e)
        cases.append(case)
    # Python str.strip includes C0 information separators, unlike Unicode White_Space.
    source='STARTFONT 2.1\nFONT trim\nSIZE 8 75 75\nFONTBOUNDINGBOX 1 1 0 0\nCHARS 1\nSTARTCHAR A\nENCODING 65\nDWIDTH 0 0\nBBX 1 1 0 0\nBITMAP\n\x1c80\x1f\nENDCHAR\nENDFONT\n'
    wire=write_pcf(Font().load_string(source))
    cases.append(dict(name='bdf-edge-PCF Python strip control separators',source=source,options={},pcf=wire.hex(),bdf=read_pcf(wire).tobdf()))
    # Copy-free read oracle for committed synthetic compiler goldens.
    for path in sorted((ROOT/'tests/data/pcf').glob('*.pcf.gz')):
        wire=gzip.decompress(path.read_bytes())
        import struct
        for i in range(struct.unpack_from('<I',wire,4)[0]):
            kind,fmt,_,_=struct.unpack_from('<4I',wire,8+16*i)
            if kind==8:break
        opts=dict(byteorder='msb' if fmt&4 else 'lsb',bitorder='msb' if fmt&8 else 'lsb',glyphpad=1<<(fmt&3),scanunit=1<<((fmt>>4)&3))
        model=read_pcf(wire)
        cases.append(dict(name='compiler-golden-'+path.name,options=opts,pcf=wire.hex(),bdf=model.tobdf(),rewrite=write_pcf(model,**opts).hex()))
    raw=(json.dumps(cases,ensure_ascii=False,separators=(',',':'))+'\n').encode();compressed=gzip.compress(raw,mtime=0)
    path=ROOT/'crates/fontpixel/tests/data/pcf_reference.json.gz'
    if args.check:assert path.read_bytes()==compressed,'PCF oracle drift'
    else:path.write_bytes(compressed)
    print(f'{len(cases)} PCF cases, {len(raw)} JSON bytes, {len(compressed)} compressed, SHA-256 {hashlib.sha256(compressed).hexdigest()}')
if __name__=='__main__':main()
