#!/usr/bin/env python3
"""Verify every timed output without changing the measured compression runs.

Native transformed blocks must be exactly those of the independently validated
full-font output, using stock fontTools + Google Brotli for stream decoding.
Null-transform blocks must be byte-identical to the original sfnt tables.
C++ and stock-fontTools comparator outputs are decoded by the official C++
implementation and checked against all original glyphs/tables/FreeType pixels.
"""
import argparse,hashlib,json,subprocess
from pathlib import Path
from fontTools.ttLib import TTFont
from woff import equivalent

def raw(path):
    font=TTFont(path)
    return {str(tag):(entry.origLength,entry.transformVersion,hashlib.sha256(entry.loadData(font.reader.transformBuffer)).hexdigest()) for tag,entry in font.reader.tables.items()}

def main():
    p=argparse.ArgumentParser();p.add_argument('--work',type=Path,required=True);p.add_argument('--reference',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();results=[]
    for path in sorted(a.work.glob('native-*.woff2')):
        if 'Tong-SC-square' in path.name:source=a.work/'Tong-SC-square.otf';validated=a.work/'Tong-SC-square.validation.woff2'
        elif 'Tong-SC-dot' in path.name:source=a.work/'Tong-SC-dot.ttf';validated=a.work/'Tong-SC-dot.validation.woff2'
        elif 'Tong-Latin-dot' in path.name:
            # This smaller input uses the unchanged stock fontTools decoder.
            count=equivalent(a.work/'Tong-Latin-dot.ttf',path,path.name,True);results.append({'file':path.name,'glyphs':count,'checks':'stock fontTools and FreeType, every glyph'});continue
        else:raise AssertionError(path)
        if '-x1' in path.stem:assert raw(path)==raw(validated),path.name
        else:
            src=TTFont(source);web=TTFont(path)
            assert set(src.reader.tables)==set(web.reader.tables)
            for tag,entry in web.reader.tables.items():assert entry.loadData(web.reader.transformBuffer)==src.reader[tag],(path,tag)
        results.append({'file':path.name,'checks':'stock Brotli-decoded table streams exactly match validated full-font streams or original null-transform bytes'})
    for path in sorted([*a.work.glob('tong-woff063-*.woff2'),*a.work.glob('fonttools-*.woff2')]):
        source=a.work/('Tong-SC-square.otf' if 'Tong-SC-square' in path.name else 'Tong-SC-dot.ttf' if 'Tong-SC-dot' in path.name else 'Tong-Latin-dot.ttf')
        out=path.with_suffix('.verified.sfnt');subprocess.run([str(a.reference),'decode',str(path),str(out)],check=True)
        count=equivalent(source,out,path.name,True);results.append({'file':path.name,'glyphs':count,'checks':'official C++ decoder, stock fontTools sfnt, FreeType, every glyph'})
        print(path.name,'PASS',count,flush=True)
    a.output.write_text(json.dumps(results,indent=2)+'\n');print('PASS',len(results),'timed outputs',flush=True)
if __name__=='__main__':main()
