#!/usr/bin/env python3
"""Reproduce public OpenType data from fontTools (MIT), never its algorithms.

Requires fonttools; generator-only dependency. No FontForge-derived code-page
heuristic is imported/copied. --check compares both static tables and boundary
oracle. See LICENSES/fontTools-MIT.txt and THIRD_PARTY_NOTICES.md.
"""
import argparse,json
from pathlib import Path
from fontTools.ttLib.tables.O_S_2f_2 import OS2_UNICODE_RANGES,intersectUnicodeRanges
from fontTools.ttLib.standardGlyphOrder import standardGlyphOrder
ROOT=Path(__file__).resolve().parents[1]
def outputs():
    s='// OpenType OS/2 block assignments and Macintosh standard names.\n// Data transcribed from fontTools (MIT); see THIRD_PARTY_NOTICES.md.\n\n'
    s+='#[rustfmt::skip]\npub(super) const UNICODE_RANGES: &[(u32, u32, u8)] = &[\n'
    boundaries={0,0xffff,0x10000,0x10ffff}
    for bit,ranges in enumerate(OS2_UNICODE_RANGES):
        if bit==57:continue
        for name,(start,end) in ranges:
            s+=f'    (0x{start:X}, 0x{end:X}, {bit}), // {name}\n'
            boundaries.update(x for x in [start-1,start,end,end+1] if 0<=x<=0x10ffff and not 0xd800<=x<=0xdfff)
    s+='];\n\n#[rustfmt::skip]\npub(super) const POST_NAMES: &[&str] = &[\n'
    for name in standardGlyphOrder:s+=f'    "{name}",\n'
    s+='];\n'
    cases=[]
    for cp in sorted(boundaries):
        bits=intersectUnicodeRanges({cp})
        fields=[sum(1<<(bit%32) for bit in bits if bit//32==word) for word in range(4)]
        cases.append([cp,fields])
    return {'crates/fontpixel/src/sfnt/data.rs':s,'tests/data/sfnt/unicode-ranges.json':json.dumps(cases,separators=(',',':'))+'\n'}
def main():
    parser=argparse.ArgumentParser();parser.add_argument('--check',action='store_true');a=parser.parse_args()
    for path,text in outputs().items():
        dest=ROOT/path
        if a.check:
            if dest.read_text()!=text:raise SystemExit(f'stale generated file: {path}')
        else:dest.write_text(text)
        print(('checked ' if a.check else 'wrote ')+path)
if __name__=='__main__':main()
