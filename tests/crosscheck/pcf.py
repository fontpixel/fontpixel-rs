#!/usr/bin/env python3
"""All supplied PCF goldens and BDF corpus, with bdftopcf/monobit oracles.
Never writes to reference/ or samples/. Run with the release CLI.
"""
import argparse,ast,gzip,hashlib,json,os,struct,subprocess,sys,tempfile,warnings
from pathlib import Path
sys.dont_write_bytecode=True
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT.parent/'reference/bdfparser/src'))
from bdfparser import Font
from bdfparser.pcf import read_pcf,write_pcf

def digest(b):return hashlib.sha256(b).hexdigest()
def fmt_options(b):
    for i in range(struct.unpack_from('<I',b,4)[0]):
        kind,fmt,_,_=struct.unpack_from('<4I',b,8+16*i)
        if kind==8:return dict(byteorder='msb' if fmt&4 else 'lsb',bitorder='msb' if fmt&8 else 'lsb',glyphpad=1<<(fmt&3),scanunit=1<<((fmt>>4)&3))
    raise AssertionError('no bitmap table')
def canonicalizer():
    source=ROOT.parent/'reference/bdfparser/tests/test_pcf.py';tree=ast.parse(source.read_text())
    nodes=[n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name in ('bdf_for_bdftopcf','_glyph_entries','_token')]
    namespace={'Font':Font};exec(compile(ast.Module(body=nodes,type_ignores=[]),str(source),'exec'),namespace)
    return namespace['bdf_for_bdftopcf']
def convert(binary,source,target,**options):
    flags=[x for k,v in options.items() for x in ['--'+k,str(v)]]
    p=subprocess.run([binary,'convert',str(source),'-o',str(target),'--report','json',*flags],capture_output=True)
    if p.returncode:raise RuntimeError(f'{source}: {p.stderr.decode()}')
    return target.read_bytes(),json.loads(p.stdout)['report']
def signature(font):
    def glyph(g):return (g.as_matrix(),g.advance_width,g.left_bearing,g.shift_up)
    return {'encoded':{int(g.codepoint):glyph(g) for g in font.glyphs if g.codepoint},'unencoded':[glyph(g) for g in font.glyphs if not g.codepoint]}
def monobit_check(binary,tmp,source):
    import monobit
    wire=tmp/'rust.pcf';back=tmp/'rust.bdf';external=tmp/'monobit.pcf';external_bdf=tmp/'monobit-back.bdf'
    convert(binary,source,wire);convert(binary,wire,back)
    m=monobit.load(wire,format='pcf')[0];as_bdf=monobit.load(back,format='bdf')[0]
    def ordered(f):return [(g.as_matrix(),g.advance_width,g.left_bearing,g.shift_up) for g in f.glyphs]
    assert ordered(m)==ordered(as_bdf),source
    mismatch=sum(a.codepoint!=b.codepoint for a,b in zip(m.glyphs,as_bdf.glyphs))
    monobit.save(m,external,format='pcf',overwrite=True)
    convert(binary,external,external_bdf)
    ext=monobit.load(external,format='pcf')[0];rust=monobit.load(external_bdf,format='bdf')[0]
    assert ordered(ext)==ordered(rust),source
    reverse_mismatch=sum(a.codepoint!=b.codepoint for a,b in zip(ext.glyphs,rust.glyphs))
    # Independent exact encoding/pixel/metric comparison where monobit's known
    # swapped two-byte encoding-table axes are not involved.
    f=Font(source);f.glyphs={cp:g for cp,g in f.glyphs.items() if 0<=cp<=255};f.unencoded=[]
    ascii_bdf=tmp/'singlebyte.bdf';ascii_bdf.write_text(f.tobdf())
    convert(binary,ascii_bdf,wire);convert(binary,wire,back)
    single=monobit.load(wire,format='pcf')[0]
    assert signature(single)==signature(monobit.load(back,format='bdf')[0])
    monobit.save(single,external,format='pcf',overwrite=True);convert(binary,external,external_bdf)
    assert signature(monobit.load(external,format='pcf')[0])==signature(monobit.load(external_bdf,format='bdf')[0])
    return dict(source=str(source.relative_to(ROOT)),glyphs=len(m.glyphs),bidirectional_all_glyph_pixels_metrics='pass',singlebyte_glyphs=len(single.glyphs),singlebyte_bidirectional_encodings='pass',monobit_054_two_byte_read_code_mismatches=mismatch,monobit_054_two_byte_write_code_mismatches=reverse_mismatch,limitation='monobit 0.54 swaps the two-byte encoding-table axes; no Rust workaround is applied')
def main():
    parser=argparse.ArgumentParser();parser.add_argument('--binary',default=str(ROOT/'target/release/fontpixel'));parser.add_argument('--output',type=Path);parser.add_argument('--monobit',action='store_true');args=parser.parse_args()
    warnings.simplefilter('ignore');canon=canonicalizer();corpus=[];goldens=[]
    with tempfile.TemporaryDirectory(prefix='fontpixel-pcf-check-') as directory:
        tmp=Path(directory);target=tmp/'out.pcf';bdf=tmp/'back.bdf'
        for source in sorted((ROOT.parent/'reference').rglob('*.pcf*')):
            if source.suffix not in ('.pcf','.gz'):continue
            data=source.read_bytes();wire=gzip.decompress(data) if data[:2]==b'\x1f\x8b' else data
            expected=read_pcf(wire);actual,_=convert(args.binary,source,bdf)
            assert actual==expected.tobdf().encode(),f'PCF read: {source}'
            opts=fmt_options(wire);rewritten,_=convert(args.binary,source,target,**opts)
            python_rewrite=write_pcf(expected,**opts)
            assert rewritten==python_rewrite,f'PCF rewrite: {source}'
            # A second cycle must stabilize unless the original compiler's buggy flags
            # corrupted ink metrics; Python still supplies the exact rewrite oracle.
            cycled,_=convert(args.binary,target,tmp/'cycle.pcf',**opts)
            assert cycled==write_pcf(read_pcf(rewritten),**opts),source
            goldens.append(dict(source=str(source.relative_to(ROOT.parent)),input_sha256=digest(wire),glyphs=len(expected.glyphs)+len(expected.unencoded),read_bdf_sha256=digest(actual),rewrite_sha256=digest(rewritten),byte_identical_input=rewritten==wire,stable_second_cycle=cycled==rewritten,result='pass'))
        sources=sorted((ROOT.parent/'reference/bdfparser/tests/fonts').glob('*.bdf'))+sorted((ROOT.parent/'samples').rglob('*.bdf'))
        for source in sources:
            f=Font(source);expected=write_pcf(f);actual,report=convert(args.binary,source,target)
            assert actual==expected,f'BDF->PCF: {source}'
            external=subprocess.run(['bdftopcf'],input=canon(f),capture_output=True)
            assert external.returncode==0,(source,external.stderr)
            assert actual==external.stdout,f'bdftopcf: {source}'
            corpus.append(dict(source=str(source.relative_to(ROOT.parent)),glyphs=len(f.glyphs)+len(f.unencoded),bytes=len(actual),sha256=digest(actual),python='byte-identical',bdftopcf='byte-identical',loss_entries=sum(e['impact']=='loss' for e in report['entries'])))
            print('PASS',source.name,flush=True)
        independent=[]
        if args.monobit:
            for source in sorted((ROOT/'tests/data/bdf').glob('*.bdf')):
                if not source.name.endswith('.golden.bdf'):independent.append(monobit_check(args.binary,tmp,source))
    result={'python':sys.version,'bdftopcf':subprocess.run(['bdftopcf','-version'],capture_output=True,text=True).stdout.strip(),'pcf_inputs':goldens,'bdf_corpus':corpus,'monobit':independent}
    if args.output:args.output.write_text(json.dumps(result,indent=2)+'\n')
    print(f'PASS {len(goldens)} PCF inputs, {len(corpus)} BDF corpus, {len(independent)} monobit bidirectional checks')
if __name__=='__main__':main()
