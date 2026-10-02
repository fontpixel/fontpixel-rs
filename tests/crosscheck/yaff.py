#!/usr/bin/env python3
"""Bidirectional monobit YAFF checks. Reference/sample inputs stay read-only."""
import argparse, hashlib, json, subprocess, tempfile, warnings
from pathlib import Path
import monobit
ROOT=Path(__file__).resolve().parents[2]
def signature(font):
    def character(g):
        if g.char:return g.char.value
        if g.codepoint and font.encoding in ('unicode','iso10646-1'):return chr(int(g.codepoint))
        return ''
    return [(g.as_matrix(),g.advance_width,g.left_bearing,g.shift_up,character(g)) for g in font.glyphs]
def run(binary,source,target):
    p=subprocess.run([binary,'convert',str(source),'-o',str(target),'--report','json'],capture_output=True)
    if p.returncode: raise RuntimeError(f'{source}: {p.stderr.decode()}')
    return json.loads(p.stdout)
def main():
    p=argparse.ArgumentParser();p.add_argument('--binary',default=str(ROOT/'target/release/fontpixel'));p.add_argument('--output',type=Path);a=p.parse_args();warnings.simplefilter('ignore');results=[]
    with tempfile.TemporaryDirectory(prefix='fontpixel-yaff-') as t:
        t=Path(t)
        for source in sorted((ROOT/'tests/data/bdf').glob('*.bdf')):
            if '.golden.' in source.name: continue
            expected=monobit.load(source,format='bdf')[0]
            rust=t/'rust.yaff'; back=t/'back.bdf'; external=t/'monobit.yaff'; normalized=t/'normalized.yaff'
            run(a.binary,source,rust); actual=monobit.load(rust,format='yaff')[0]
            assert signature(actual)==signature(expected),f'Rust -> monobit: {source}'
            run(a.binary,rust,back);run(a.binary,source,t/'canonical.bdf')
            assert back.read_bytes()==(t/'canonical.bdf').read_bytes(),'exact BDF source roundtrip'
            monobit.save(expected,external,format='yaff',overwrite=True);run(a.binary,external,normalized)
            assert signature(monobit.load(normalized,format='yaff')[0])==signature(expected),f'monobit -> Rust: {source}'
            results.append(dict(source=str(source.relative_to(ROOT)),glyphs=len(expected.glyphs),bidirectional_pixels_horizontal_metrics_characters='pass',bdf_exact_bytes='pass',yaff_sha256=hashlib.sha256(rust.read_bytes()).hexdigest()))
        synthetic=ROOT/'tests/data/yaff/portable.yaff'
        expected=monobit.load(synthetic,format='yaff');run(a.binary,synthetic,t/'synthetic.yaff');actual=monobit.load(t/'synthetic.yaff',format='yaff')
        assert len(actual)==len(expected)
        for e,r in zip(expected,actual):
            assert signature(e)==signature(r)
            assert [tuple(str(x) for x in g.get_labels()) for g in e.glyphs]==[tuple(str(x) for x in g.get_labels()) for g in r.glyphs]
        monobit.save(expected,t/'monobit-synthetic.yaff',format='yaff',overwrite=True)
        run(a.binary,t/'monobit-synthetic.yaff',t/'monobit-synthetic-back.yaff')
        reloaded=monobit.load(t/'monobit-synthetic-back.yaff',format='yaff')
        assert [signature(f) for f in expected]==[signature(f) for f in reloaded]
        results.append(dict(source=str(synthetic.relative_to(ROOT)),fonts=len(expected),glyphs=sum(len(f.glyphs) for f in expected),bidirectional_pixels_metrics_all_labels='pass'))
    result=dict(tool='monobit',version=__import__('importlib.metadata',fromlist=['version']).version('monobit'),checks=results)
    text=json.dumps(result,indent=2)+'\n'; print(text)
    if a.output:a.output.write_text(text)
if __name__=='__main__':main()
