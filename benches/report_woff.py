#!/usr/bin/env python3
"""Generate the W2.5 report from complete five-sample observations only."""
import argparse,hashlib,json,statistics
from pathlib import Path
from fontTools.ttLib import TTFont
ROOT=Path(__file__).resolve().parents[1]
def main():
    p=argparse.ArgumentParser();p.add_argument('--input',type=Path,required=True);p.add_argument('--work',type=Path,required=True);p.add_argument('--output',type=Path,default=ROOT/'benches/WOFF-REPORT.md');a=p.parse_args();d=json.loads(a.input.read_text());rows={r['label']:r for r in d['results']}
    assert len(rows)==29,('incomplete or duplicate measurements',len(rows))
    for r in rows.values():assert len(r['seconds'])==5 and statistics.median(r['seconds'])==r['median_seconds']
    def line(label,name):
        r=rows[label];return f"| {name} | {r['median_seconds']:.6f} | {min(r['seconds']):.6f}–{max(r['seconds']):.6f} | {r['bytes']:,} | {r.get('segment',0)} |"
    def table(lines):return '\n'.join(['| Method | Median s | Min–max s | Bytes | Segment |','|---|---:|---:|---:|---:|',*lines])
    out=['# WOFF2 performance — W2.5, 2026-10-03','',
    'All medians use five wall-clock samples, retained in `benches/results/woff-resumed.json`. Input reads/output writes are outside the timer. No other worker-owned build, fuzz or correctness job ran during timed groups. The VM is shared and unpinned; these are observed results, not universal speed guarantees.','',
    f"Machine: {d['machine']['cpu']}; {d['machine']['cpus']} visible CPUs; {d['machine']['memory']}; {d['machine']['platform']}.",
    'Rust 1.99.0, release profile; native brotli 9.0.0 / brotli-decompressor 6.0.1; Tong comparison uses its exact woff 0.6.3 C++ call; stock fontTools '+d['tools']['fonttools']+' and Google Brotli Python '+d['tools']['brotli']+'.', '',
    'An executor control-session interruption occurred at 08:40 UTC after 18 complete groups. These groups are segment 0. Segment 1 restarted incomplete groups under the same reported CPU/RAM/kernel and binary fingerprints; it also repeats native SC-dot q11 for a within-segment C++ comparison. No interrupted partial group is counted. `benchmark-pre-restart.json` and the final JSON preserve both provenance and raw samples. Exploratory `benchmark-before-batch-caps.json` is excluded from every table below.','',
    '## Inputs','', '| Input | Glyphs | sfnt bytes | SHA-256 |','|---|---:|---:|---|']
    for name in ['Tong-SC-square.otf','Tong-SC-dot.ttf','Tong-Latin-dot.ttf']:
        path=a.work/name;raw=path.read_bytes();glyphs=TTFont(path,lazy=True)['maxp'].numGlyphs;out.append(f'| {name} | {glyphs:,} | {len(raw):,} | {hashlib.sha256(raw).hexdigest()} |')
    out+=['','Inputs use all glyphs from the supplied SC or Latin BDF, plus `.notdef` and (for dots) the shared dot glyph. CFF-square and TrueType-dot are PLAN §5.3’s recommended source choices. Full family/locl assembly is outside W2.5; the phase-2 whole-export ≤40-second target is NOT claimed here.','']
    for source in ['square','dot']:
        out+=['## Full SC '+source+': native quality/thread trade-offs','',table([line(f'native-Tong-SC-{source}-q{q}-t{t}-x1',f'q{q}, {t} thread(s)') for q,t in [(5,1),(8,1),(9,1),(11,1),(11,2),(11,4)]])+ '\n',table([line(f'native-Tong-SC-{source}-q5-t1-x0','q5, null transforms')]),'']
        b=rows[f'native-Tong-SC-{source}-q11-t1-x1'];fast=rows[f'native-Tong-SC-{source}-q9-t1-x1'];parallel=rows[f'native-Tong-SC-{source}-q11-t4-x1']
        out+=[f"At q11, four intra-font threads deliver {b['median_seconds']/parallel['median_seconds']:.2f}× the observed single-thread throughput; size changes by {(parallel['bytes']/b['bytes']-1)*100:+.3f}%. Quality 9 is {b['median_seconds']/fast['median_seconds']:.2f}× faster than q11 with {(fast['bytes']/b['bytes']-1)*100:+.2f}% bytes. These ratios use the same input and segment.",'']
    out+=['## Same-input independent encoder comparisons','', '### Full SC CFF square, q11', '',table([line('native-Tong-SC-square-q11-t1-x1','native Rust'),line('tong-woff063-Tong-SC-square-q11','Tong / Google C++'),line('fonttools-Tong-SC-square-q11','stock fontTools')]),'', '### Full SC TrueType dot, q11, post-restart segment', '',table([line('native-Tong-SC-dot-q11-t1-x1-recheck-segment1','native Rust recheck'),line('tong-woff063-Tong-SC-dot-q11','Tong / Google C++')]),'', '### Latin TrueType dot, q11, same-input three-way comparison', '',table([line('native-Tong-Latin-dot-q11-t1-x1','native Rust'),line('tong-woff063-Tong-Latin-dot-q11','Tong / Google C++'),line('fonttools-Tong-Latin-dot-q11','stock fontTools')]),'',
    'The stock full-SC dot correctness pass was canceled after about 20 minutes without finishing its first fontTools/FreeType equivalence pass. That is a censored correctness-run observation, NOT an encoder/decoder benchmark or median. The verification-only memoryview adapter proved byte-identical on representative simple, composite, hinted, affine, explicit-bbox and overlap fixtures and then passed all full-SC checks, corroborated by the official C++ decoder. The adapter was never enabled for these performance measurements. Stock fontTools’ Latin-dot three-way comparison is a separate workload and is not mixed with SC speed ratios.','',
    '## Four independent full-font batch','',
    'Fixed input order/multiset: SC CFF square, SC TrueType dot, then both again. Intra-font threads are one. Batch guards are enabled (1024-font and 256 MiB aggregate input/output limits).', '',table([line(f'native-four-fonts-q{q}-jobs{jobs}',f'q{q}, {jobs} job(s)') for q in [5,11] for jobs in [1,2,4]]),'']
    for q in [5,11]:
        group=[rows[f'native-four-fonts-q{q}-jobs{j}'] for j in [1,2,4]];assert len({r['bytes'] for r in group})==1
        out.append(f"q{q}: four jobs are {group[0]['median_seconds']/group[2]['median_seconds']:.2f}× faster than one in this VM, with identical aggregate output byte counts and deterministic input-order results.")
    out+=['', '## Recommendations and boundaries','',
    '- Keep required default quality 11. Expose lower qualities explicitly: they greatly reduce latency but materially increase bytes for the dot font.',
    '- Independent multi-font jobs preserve each file’s bytes. Tune job count to CPU and concurrent compressor working-memory capacity; input/output/count budgets also bound retained batch data.',
    '- Single-font parallel Brotli is implemented using brotli’s legal single-stream chunk encoder, not concatenated independent streams. It changes compression decisions and may slightly increase or decrease size; fixed options remain deterministic.',
    '- Quality is not a guarantee of monotonically smaller output: the CFF q8 result is smaller than q9 in this dataset. All observed sizes are retained.',
    '- Avoid guessing that Rust is always faster: compare the same-input tables above. C++ and stock fontTools remain competitive on CFF q11; latency gains primarily come from explicit parallelism or lower quality.',
    '- The benchmark covers webfont envelope coding. It does not establish the complete Tong eight-vector/PCF/OTB/locl pipeline target or native CLI/FontSet integration reserved for later work.', '',
    '## Correctness of measured outputs','',
    '`tests/crosscheck/woff_bench_verify.py` independently decodes every native timed stream with stock fontTools/Google Brotli and requires exact transformed-block equality with the already verified full-font output (or original bytes for null transforms). C++/fontTools comparator outputs go through the official C++ decoder and every-glyph fontTools/FreeType comparisons. See `reports/w2.5/benchmark-verification.json`.', '',
    'Reproduction: `benches/WOFF-README.md`, `reports/w2.5/COMMANDS.txt`, `benches/woff.py`, `benches/report_woff.py`. All protected sources remain unchanged.']
    a.output.write_text('\n'.join(out)+'\n')
if __name__=='__main__':main()
