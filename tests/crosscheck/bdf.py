#!/usr/bin/env python3
"""Canonical byte parity for every supplied BDF plus bidirectional monobit checks.

Use the supplied references/samples read-only; generated output is temporary.
Run after `cargo build --release -p fontpixel-cli`.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import warnings
sys.dont_write_bytecode=True
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT.parent/"reference/bdfparser/src"))
from bdfparser import Font


def cross_monobit(binary,source,tmp):
    import monobit
    rust=tmp/"rust.bdf";back=tmp/"monobit.bdf";result=tmp/"monobit-back.bdf"
    subprocess.run([binary,"convert",str(source),"-o",str(rust)],check=True,capture_output=True)
    original=monobit.load(source,format="bdf")[0]
    read=monobit.load(rust,format="bdf")[0]
    def signature(f):
        pixels=lambda g:(g.as_matrix(),g.advance_width,g.left_bearing,g.shift_up)
        return {"encoded":{int(g.codepoint):pixels(g) for g in f.glyphs if g.codepoint},"unencoded":[pixels(g) for g in f.glyphs if not g.codepoint]}
    assert signature(original)==signature(read),source
    monobit.save(read,back,format="bdf",overwrite=True)
    subprocess.run([binary,"convert",str(back),"-o",str(result)],check=True,capture_output=True)
    assert signature(monobit.load(back,format="bdf")[0])==signature(monobit.load(result,format="bdf")[0]),source
    after=monobit.load(back,format="bdf")[0]
    return {"source":str(source.relative_to(ROOT)),"glyphs":len(read.glyphs),"monobit_written_glyphs":len(after.glyphs),"pixels_metrics_including_unencoded":"pass", "monobit_changes_tags": [str(g.tags) for g in read.glyphs] != [str(g.tags) for g in after.glyphs],"result":"pass"}


def main():
    parser=argparse.ArgumentParser();parser.add_argument("--binary",default=str(ROOT/"target/release/fontpixel"));parser.add_argument("--output",type=Path);parser.add_argument("--monobit",action="store_true");args=parser.parse_args()
    results=[]
    sources=sorted((ROOT.parent/"reference/bdfparser/tests/fonts").glob("*.bdf"))+sorted((ROOT.parent/"samples").rglob("*.bdf"))
    with tempfile.TemporaryDirectory(prefix="fontpixel-bdf-crosscheck-") as directory:
        tmp=Path(directory)
        for i,source in enumerate(sources):
            with warnings.catch_warnings(record=True) as seen:
                warnings.simplefilter("always")
                f=Font(source);expected=f.tobdf().encode()
            target=tmp/f"{i}.bdf"
            p=subprocess.run([args.binary,"convert",str(source),"-o",str(target),"--report","json"],capture_output=True)
            if p.returncode:raise RuntimeError(f"{source}: {p.stderr.decode()}")
            actual=target.read_bytes();assert actual==expected,f"bytes differ: {source}"
            info=json.loads(p.stdout)
            parse_warnings=[e["message"] for e in info["report"]["entries"] if e["code"].startswith("bdf.parse.")]
            assert parse_warnings==[str(w.message) for w in seen],f"warnings differ: {source}"
            results.append({"source":str(source.relative_to(ROOT.parent)),"bytes":source.stat().st_size,"glyphs":len(f.glyphs),"unencoded":len(f.unencoded),"sha256":hashlib.sha256(actual).hexdigest(),"canonical_matches_original":actual==source.read_bytes(),"result":"pass"})
            print(f"PASS {source.name}: {len(f.glyphs)} encoded + {len(f.unencoded)} unencoded",flush=True)
            target.unlink()
        monobit_results=[]
        if args.monobit:
            for source in sorted((ROOT/"tests/data/bdf").glob("*.bdf")):
                if source.name.endswith(".golden.bdf"):continue
                monobit_results.append(cross_monobit(args.binary,source,tmp))
    result={"python":sys.version,"corpus":results,"monobit":monobit_results}
    if args.output:args.output.write_text(json.dumps(result,indent=2)+"\n")
    print(f"PASS: {len(results)} corpus byte/warning comparisons; {len(monobit_results)} bidirectional monobit fonts")
if __name__=="__main__":main()
