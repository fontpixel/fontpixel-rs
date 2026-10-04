#!/usr/bin/env python3
"""Reconstructed complete case preparation, not historical raw evidence."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import sys
ROOT = Path(__file__).resolve().parents[1]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--work', type=Path, required=True)
    p.add_argument('--target', type=Path, required=True)
    p.add_argument('--catalogue-plan', type=Path, required=True)
    p.add_argument('--woff2-square-source', type=Path, required=True)
    p.add_argument('--woff2-dot-source', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--section', choices=['all', 'codec', 'tong', 'catalogue', 'fonttools', 'merge'], default='all')
    p.add_argument('--tong-cli-variant-bytes', type=int, default=0)
    a = p.parse_args(); work, target = a.work.resolve(), a.target.resolve()
    py = ROOT.parent / 'local-work/oracles/bin/python'
    node = Path(shutil.which('node')).resolve(); bdftopcf = Path(shutil.which('bdftopcf')).resolve()
    sample = ROOT.parent / 'samples/tong-pixel-sans/TongPixelSans14Large-SC.bdf'
    baseline = ROOT.parent / 'validation-inputs/phase2/bdf'; reference = work / 'reference'
    cli = target / 'release/fontpixel'; driver = target / 'release/examples/phase6_timing'
    tong = target / 'release/examples/phase6_tong_export'; original = target / 'release/tong-export'
    rename = target / 'release/examples/phase6_catalogue_export_name'
    cases = []
    def add(section, name, engine, command, **kwargs):
        if a.section not in ['all', section]: return
        cases.append({'id': name, 'engine': engine, 'command': list(map(str, command)), **kwargs})
    for mode, engine in [('native', 'rust-native'), ('compat', 'rust-compat')]:
        add('codec', 'tong38653.drawall-mode1.reconstructed-v2.bdf', engine, [driver, mode, sample, '{output}'])
    add('codec', 'tong38653.drawall-mode1.reconstructed-v2.bdf', 'python', [py, ROOT / 'benches/phase6_bdfparser.py', reference / 'bdfparser', sample, '{output}'])
    add('codec', 'tong38653.drawall-mode1.reconstructed-v2.bdf', 'node', [node, '--expose-gc', ROOT / 'benches/phase6_bdfparser.mjs', work / 'bdfparser.mjs', sample, '{output}'])
    common = ['--reference-root', work, '--bdf', baseline, '--output', '{output}']
    for label, options in [('default', []), ('quality8', ['--quality', '8']), ('quality5', ['--quality', '5']),
                           ('serial11', ['--jobs', '1']), ('intra4', ['--jobs', '2', '--threads', '4']),
                           ('literal11', ['--dot-reuse', 'literal']), ('rows11', ['--dot-reuse', 'rows'])]:
        add('tong', 'tong-full.reconstructed-v2.' + label, 'rust-native-api', [py, ROOT / 'benches/phase6_tong.py', 'native', '--binary', tong, *common, *options])
    cli_label = 'default' if a.tong_cli_variant_bytes == 0 else 'cli-' + str(a.tong_cli_variant_bytes // (1024 * 1024)) + 'MiB'
    cli_extra = [] if a.tong_cli_variant_bytes == 0 else ['--variant-max-total-bytes', a.tong_cli_variant_bytes]
    add('tong', 'tong-full.reconstructed-v2.' + cli_label, 'rust-batch-cli', [py, ROOT / 'benches/phase6_tong.py', 'batch', '--binary', cli, *common, *cli_extra])
    add('tong', 'tong-full.reconstructed-v2.default', 'original-export-rs', [py, ROOT / 'benches/phase6_tong.py', 'reference', '--binary', original, *common])
    for engine, label in [('python', 'original-python-fonttools'), ('native', 'rust-batch-cli-with-native-helper')]:
        add('catalogue', 'catalogue-all181.explicit-metadata-v2', label, [py, ROOT / 'benches/phase6_catalogue.py', engine,
            '--reference', reference, '--samples', ROOT.parent / 'samples/fontpixel-catalogue',
            '--binary', cli, '--rename-binary', rename, '--plan', a.catalogue_plan.resolve(), '--output', '{output}'])
    for mode, engine in [('vector-native', 'rust-cli'), ('vector-fonttools', 'original-tong-fonttools')]:
        add('fonttools', 'tong38653.square.ttf', engine, [py, ROOT / 'benches/phase6_fonttools.py', mode,
            '--reference-root', work, '--source', sample, '--binary', cli, '--output', '{output}'])
    for label, source in [('square', a.woff2_square_source.resolve()), ('dot', a.woff2_dot_source.resolve())]:
        add('fonttools', 'same-sfnt.' + label + '.woff2', 'rust', [driver, 'woff2', source, '{output}'])
        add('fonttools', 'same-sfnt.' + label + '.woff2', 'fonttools', [py, ROOT / 'benches/phase6_fonttools.py', 'woff2-fonttools',
            '--reference-root', work, '--source', source, '--output', '{output}'])
    unifont = reference / 'bdfparser/tests/fonts/unifont-13.0.04.bdf'
    latin = ROOT.parent / 'samples/tong-pixel-sans/TongPixelSans14Large-Latin.bdf'
    add('merge', 'unifont57086-plus-latin.bdf', 'rust-cli', [cli, 'merge', '--family', 'Unifont Western Mix',
        '--part', latin, '--take', 'western', '--part', unifont, '--take', 'all', '--spacing', 'prop',
        '-o', '{output}/merged.bdf', '--report', 'json'], internal_json=False)
    for case in cases:
        if case['id'].startswith('tong38653.drawall-mode1'):
            case['expected_stdout'] = {'glyphs': 38653, 'input_bytes': 5746021, 'profile': 'reconstructed-full-input-v2'}
        elif case['id'].startswith('tong-full.'):
            case['expected_stdout'] = {'artifact_count': 84, 'export_count': 64, 'source_bdf_copies': 20}
        elif case['id'].startswith('catalogue-all181'):
            case['expected_stdout'] = {'source_fonts': 34, 'families': 8, 'download_files': 181}
    identities = [py.resolve(), node, bdftopcf, driver, tong, cli, original, rename,
                  work / 'bdfparser.mjs', work / 'js-tools/package-lock.json', reference,
                  ROOT.parent / 'samples', baseline, a.catalogue_plan.resolve(),
                  a.woff2_square_source.resolve(), a.woff2_dot_source.resolve(),
                  ROOT.parent / 'local-work/oracles/lib/python3.12/site-packages']
    oracle_code = """import importlib.metadata,json,pathlib,sys
import fontTools,freetype,uharfbuzz,ots,brotli,PIL,monobit
libraries=sorted({line.split()[-1] for line in pathlib.Path('/proc/self/maps').read_text().splitlines() if line.split()[-1].startswith('/') and '.so' in line.split()[-1]})
print(json.dumps({'python':sys.version,'packages':{name:importlib.metadata.version(name) for name in ['fonttools','freetype-py','uharfbuzz','opentype-sanitizer','brotli','Pillow','monobit']},'freetype_runtime':freetype.version(),'harfbuzz_runtime':uharfbuzz.version_string(),'loaded_native_libraries':libraries}))"""
    oracle_identity = json.loads(subprocess.check_output([str(py), '-c', oracle_code], text=True))
    oracle_identity['esbuild'] = subprocess.check_output([str(work / 'js-tools/node_modules/.bin/esbuild'), '--version'], text=True).strip()
    v = subprocess.run([str(bdftopcf), '-version'], capture_output=True, text=True)
    oracle_identity['bdftopcf'] = {'binary': str(bdftopcf), 'exit': v.returncode, 'version_output': v.stdout + v.stderr}
    identities.extend(Path(p) for p in oracle_identity['loaded_native_libraries'])
    identities.append(Path(subprocess.check_output(['rustup', 'which', 'rustc'], text=True).strip()))
    config = {'schema': 1, 'scope': 'PLAN10 complete current-machine five-process workloads', 'cases': cases,
              'oracle_identity': oracle_identity,
              'tong_cli_profile': {'variant_max_total_bytes': a.tong_cli_variant_bytes,
                                   'default_profile': a.tong_cli_variant_bytes == 0,
                                   'default_probe_evidence': 'see separately retained final-binary correctness probe; earlier candidates may differ'},
              'identity_paths': list(map(str, identities)),
              'rust_build_command': 'cargo build --locked --release --all-features -p fontpixel -p fontpixel-cli --bin fontpixel --example phase6_timing --example phase6_tong_export --example phase6_catalogue_export_name',
              'original_build_command': 'cargo build --locked --release --manifest-path local-work/phase6-performance/reference/tong-pixel-sans/tools/export-rs/Cargo.toml',
              'reference_vector_limit': 'Supplied catalogue production opf_vector extension absent; original _build_vector_ttf_reference is explicit replacement, never production extension throughput.'}
    a.output.parent.mkdir(parents=True, exist_ok=True)
    with a.output.open('x') as out: json.dump(config, out, indent=2); out.write('\n')
    print(json.dumps({'cases': len(cases), 'section': a.section}))

if __name__ == '__main__': main()
