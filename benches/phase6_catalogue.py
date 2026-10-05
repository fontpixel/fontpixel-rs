#!/usr/bin/env python3
"""Complete catalogue adapter reconstructed after workspace loss.

The handoff does not contain opf_vector's source or binary. The Python comparison
uses the original explicitly named fontTools vector oracle, never the throughput
of the missing production extension. This file is not asserted byte-identical to
the unavailable old source. Its metadata flags require the reconstructed core and CLI to compile and pass
actual complete-workload validation; only standard-library contracts are tested.
"""
from __future__ import annotations
import argparse
import gzip
import json
import os
import tempfile
from pathlib import Path
import subprocess
import sys
import time
import zipfile
from phase6_support import batch_header

PLAN_VERSION = 2


def app_names(family, style, kwargs):
    return {'family': family, 'style': style, 'full_name': f'{family} {style}',
            'postscript_name': f'{family}-{style}'.replace(' ', ''),
            'copyright': kwargs.get('copyright_', ''),
            'font_version': kwargs.get('version', '1.000')}


def setup(reference):
    protected = Path(__file__).resolve().parents[2] / 'reference'
    if reference.resolve().is_relative_to(protected.resolve()):
        raise ValueError('execute reference code only from a writable local-work copy')
    sys.dont_write_bytecode = True
    sys.path.insert(0, str(reference / 'fontpixel/pipeline'))
    from opf import build, downloads, familymeta, licenses, vectorize
    from opf.parsers.bdf import parse_bdf
    from opf.parsers.pcf import parse_pcf
    def parse_font(path, slug):
        return parse_pcf(path, slug) if '.pcf' in path.name.lower() else parse_bdf(path, slug)
    return build, downloads, familymeta, licenses, vectorize, parse_font


def families(samples, api):
    build, _, familymeta, licenses, _, parse = api
    result = []
    for folder in sorted(samples.iterdir()):
        if not folder.is_dir(): continue
        paths = sorted(p for p in folder.iterdir() if p.name.lower().endswith(('.bdf', '.bdf.gz', '.pcf', '.pcf.gz')))
        if not paths: continue
        meta = familymeta.load_family_meta(folder)
        fonts = [parse(path, folder.name) for path in paths]
        variants = [(font, familymeta.resolve_variant(font, meta)) for font in fonts]
        license_info = licenses.detect_license(folder, fonts, meta.license_override)
        license_text, canonical = build._license_text_for(folder, license_info)
        canonical_name = build._canonical_license_file(license_info.spdx or '') if canonical else ''
        license_name = license_info.name_en or license_info.name
        readme = build._readme_text(meta, license_name, canonical_name)
        result.append({'slug': folder.name, 'folder': folder, 'meta': meta, 'fonts': variants,
                       'license_files': [license_info.file] if license_info.file else [],
                       'extra_files': [(canonical_name, license_text)] if canonical_name and license_text else [],
                       'readme': readme, 'copyright': build._copyright_line(meta, license_name)})
    assert len(result) == 8 and sum(len(f['fonts']) for f in result) == 34
    return result


def arguments(family, output):
    return dict(slug=family['slug'], family_dir=family['folder'], fonts=family['fonts'],
                license_files=family['license_files'], readme_text=family['readme'], out=output,
                family_display=family['meta'].export_name or family['meta'].name,
                copyright_line=family['copyright'], rename_bdf=family['meta'].export_name,
                extra_files=family['extra_files'])


def inventory(api, all_families, output):
    _, downloads, _, _, vectorize, _ = api
    tasks, current, renames = [], {}, {}
    for family in all_families:
        export_name = family['meta'].export_name
        if not export_name: continue
        for font, variant in family['fonts']:
            renames[str(font.path.resolve())] = {
                'export_name': export_name, 'size': variant.size,
                'weight': variant.weight, 'width': variant.width,
                'spacing': variant.spacing, 'script_subset': variant.script_subset,
                'weight_capitalized': variant.weight.capitalize(),
            }
    original_source = downloads._source_bdf_bytes
    def source(font):
        path = str(font.path.resolve())
        current.clear(); current.update(inputs=[path])
        if path in renames: current['rename'] = renames[path]
        return original_source(font)
    def bitmap(fonts, family_name, style_name, out, **kwargs):
        current.clear(); current.update(inputs=[str(f.path.resolve()) for f in fonts],
                                        bitmap=True, **app_names(family_name, style_name, kwargs))
        out.write_bytes(b''); return {}
    def vector(font, family_name, style_name, out, *, shape='square', **kwargs):
        current.clear(); current.update(inputs=[str(font.path.resolve())], shape=shape,
                                        pixel_size=font.pixel_size, bitmap=False,
                                        **app_names(family_name, style_name, kwargs))
        out.write_bytes(b''); return {}
    def entry(slug, variant, kind, file, data, out):
        task = dict(current); task.update(slug=slug, variant=variant, kind=kind, file=file)
        if kind != 'zip': tasks.append(task)
        return {'family': slug, 'variantId': variant, 'kind': kind, 'file': file}
    saved = {name: getattr(downloads, name) for name in ['_source_bdf_bytes', '_pcf_bytes', 'build_ttf', '_woff2', '_entry']}
    original_vector = vectorize.build_vector_ttf
    try:
        downloads._source_bdf_bytes = source
        downloads._pcf_bytes = lambda *a, **kw: b''
        downloads.build_ttf = bitmap
        downloads._woff2 = lambda *a, **kw: b''
        downloads._entry = entry
        vectorize.build_vector_ttf = vector
        for family in all_families: downloads.build_downloads(**arguments(family, output))
    finally:
        for name, value in saved.items(): setattr(downloads, name, value)
        vectorize.build_vector_ttf = original_vector
    return tasks


PROFILES = ('default', 'explicit')
VECTOR_KINDS = ('ttf-square', 'ttf-round', 'woff2-square', 'woff2-round')


def is_pcf(path):
    return str(path).lower().endswith(('.pcf', '.pcf.gz'))


def toml_value(value):
    return str(value) if isinstance(value, int) and not isinstance(value, bool) else json.dumps(value)


def explicit_options(task, parsed):
    """Explicit native options reproducing the original download policy.

    Every value is an explicit fontpixel option; no native default changes.
    BDF: ingest/bdfwrite.py:18-21 FONT (source PCF FONT property, otherwise the
    -opf- template, since parsers/bdf.py:78-89 never stores a BDF FONT), :22
    SIZE {px} 75 75, :26-30 drops the FONT property and writes FONT_ASCENT,
    FONT_DESCENT and PIXEL_SIZE, :44 SWIDTH round(DWIDTH*1000/px). PCF:
    downloads.py:261-267 copies an unrenamed, unmerged PCF source (here: a
    direct PCF to PCF conversion without the charset step), otherwise :268-278
    compiles that BDF with bdftopcf, which keeps existing POINT_SIZE and
    RESOLUTION_X/Y properties (explicit-properties precedence). Vector:
    vectorize.py:301-302 empty .notdef with advance max(unit*px//2, 1), LSB 0.
    The original COMMENT lines are NOT dropped (bdfwrite.py writes none); that
    remains a documented original-lossy difference.
    """
    kind = task['kind']
    if kind in ('bdf', 'pcf'):
        source = task['inputs'][0]
        font = parsed[str(Path(source).resolve())]
        if kind == 'pcf' and is_pcf(source) and not task.get('rename') and not font.merged:
            return {'pcf_copy': True}
        px = font.pixel_size
        options = {'swidth_ppem': px, 'point_size': px, 'resolution': '75,75',
                   'set_integer_property': [f'PIXEL_SIZE={px}', f'FONT_ASCENT={font.ascent}',
                                            f'FONT_DESCENT={font.descent}']}
        if not task.get('rename'):
            # The export_name helper already wrote the renamed FONT for those tasks.
            options['font_name'] = str(font.props.get('FONT', '')) or (
                f'-opf-{font.family_slug}-medium-r-normal--{px}-{px * 10}-75-75-p-{px * 10}-iso10646-1')
        if kind == 'bdf':
            options['remove_property'] = ['FONT']
        else:
            options['pcf_metadata_precedence'] = 'explicit-properties'
        return options
    if kind == 'ttf':
        fonts = sorted((parsed[str(Path(p).resolve())] for p in task['inputs']), key=lambda f: f.pixel_size)
        ref = fonts[-1]; scale = UPEM / max(ref.pixel_size, 1)
        return {
            # ttfexport.py:62 sorts the strikes by pixel size (host input order here).
            'inputs': [str(f.path.resolve()) for f in fonts],
            # :103 + fontBuilder.py:440-490 setupNameTable.
            'otb_name_record': fonttools_name_records(task),
            # fontBuilder.py:145,147-148,154 _headDefaults; ttfexport.py:17-20,180.
            'otb_head_flags': 3, 'otb_lowest_rec_ppem': 3,
            'otb_created': FIXED_TIMESTAMP, 'otb_modified': FIXED_TIMESTAMP,
            # :104-105 setupOS2 with fontBuilder.py:267-308 _getOS2Defaults.
            'otb_os2_version': '3', 'otb_vendor': '????', 'otb_fs_type': 4, 'otb_fs_selection': 0,
            'otb_break_char': 32,
            # :107 setupPost(keepGlyphNames=False, isFixedPitch=0); fontBuilder.py:185-186.
            'otb_post_names': 'omit', 'otb_fixed_pitch': 'false', 'otb_underline': '0,0',
            # :75 setupCharacterMap -> fontBuilder.py:399-421 (format 12 only above U+FFFF).
            'otb_cmap_unicode_full': 'when-needed',
            # :65-71 glyph order '.notdef' + sorted code points; :84 .notdef advance, no bitmap.
            'otb_glyph_order': 'codepoint',
            'otb_notdef': 'empty', 'otb_notdef_advance': int(round(ref.pixel_size * scale / 2)),
            # :76-80 empty glyf outlines; fontBuilder.py:167 maxZones=2.
            'otb_outline_tables': 'empty-glyf', 'otb_max_zones': '2',
            # fontTools _h_m_t_x.py:93-104 trailing equal-advance compaction.
            'otb_hmtx': 'compact-trailing',
            # :140-154 line metrics: widthMax = max DWIDTH, caret 0/1, zero extremes,
            # the same record for hori and vert; :164-169 one index subtable per strike.
            'otb_width_max': 'advance', 'otb_caret_slope': '0,1', 'otb_line_extremes': 'zero',
            'otb_vertical_line_metrics': 'mirror-horizontal', 'otb_index_layout': 'spanning',
        }
    if kind in VECTOR_KINDS:
        px = task['pixel_size']; unit = 1000 // max(px, 1)
        return {'notdef': 'empty', 'notdef_advance': max(unit * px // 2, 1), 'notdef_lsb': 0,
                'vendor': '????',
                # vectorize.py:309 setupNameTable(name_records(...)), fontBuilder.py:440-490.
                'name_record': fonttools_name_records(task),
                # vectorize.py:310-311 setupOS2 -> fontBuilder.py:267-308 _getOS2Defaults
                # (fsType 4, fsSelection 0, usBreakChar 32, code pages 0).
                'fs_type': 4, 'fs_selection': 0, 'break_char': 32, 'code_page_ranges': '0,0',
                # vectorize.py:312 isFixedPitch=0; :315-317 FIXED_TIMESTAMP.
                'fixed_pitch': 'false', 'created': FIXED_TIMESTAMP, 'modified': FIXED_TIMESTAMP,
                # fontBuilder.py:167 maxZones=2; FontBuilder never writes gasp.
                'max_zones': '2', 'gasp': 'omit',
                # vectorize.py:286 setupCharacterMap -> fontBuilder.py:399-421.
                'cmap_unicode_full': 'when-needed',
                # vectorize.py:290-292 '.notdef', [dot], then sorted code points.
                'glyph_order': 'codepoint',
                **({'dot_glyph': 'after-notdef'} if task['shape'] == 'round' else {})}
    return {}


UPEM = 1000  # ttfexport.py:16
FIXED_TIMESTAMP = 3660681600  # ttfexport.py:17-20
# ttfexport.py:24-41 name_records() keys in name ID order 0..6.
NAME_KEYS = ('copyright', 'familyName', 'styleName', 'uniqueFontIdentifier', 'fullName', 'version', 'psName')


def fonttools_name_records(task):
    """Exact records of fontBuilder.py:440-490 setupNameTable(name_records(...)).

    addMultilingualName writes a Windows 3/1/0x409 record and, through
    _n_a_m_e.py:448-483 _makeMacName, a Mac 1/0/0 record when Mac Roman encodes
    the text. The ltag fallback for other text is not reproduced: it raises.
    """
    names = {'copyright': task['copyright'], 'familyName': task['family'], 'styleName': task['style'],
             'uniqueFontIdentifier': f"{task['family']}-{task['style']}-{task['font_version']}",
             'fullName': f"{task['family']} {task['style']}",
             'psName': f"{task['family']}-{task['style']}".replace(' ', ''), 'version': task['font_version']}
    records = []
    for name_id, key in enumerate(NAME_KEYS):
        text = names[key]
        text.encode('mac_roman')
        records += [f'3/1/0x409/{name_id}={text}', f'1/0/0/{name_id}={text}']
    return records


def attach_profile(tasks, all_families, profile):
    if profile == 'default':
        return tasks
    parsed = {str(font.path.resolve()): font for family in all_families for font, _ in family['fonts']}
    result = []
    for task in tasks:
        explicit = explicit_options(task, parsed)
        # The bitmap TTF strike order is an input order, not a writer option.
        result.append(dict(task, explicit=explicit, inputs=explicit.pop('inputs', task['inputs'])))
    return result


def jobfile(tasks, target):
    lines = ['version = 1']
    for index, task in enumerate(tasks):
        lines.extend(['', '[[tasks]]', 'id = ' + json.dumps(f'catalogue.{index}.{task["kind"]}')])
        kind = task['kind']; path = target / task['file']
        if kind in ['bdf', 'pcf', 'ttf']:
            lines.extend(['kind = "convert"', 'inputs = ' + json.dumps(task['inputs']),
                          'from = ' + json.dumps('pcf' if task['inputs'][0].lower().endswith(('.pcf', '.pcf.gz')) else 'bdf'),
                          'to = ' + json.dumps('otb' if kind == 'ttf' else kind),
                          'output = ' + json.dumps(str(path))])
            if kind == 'ttf':
                lines.extend(['family = ' + json.dumps(task['family']), '[tasks.options]'])
                for key in ['style', 'full_name', 'postscript_name', 'font_version', 'copyright']:
                    lines.append(key + ' = ' + json.dumps(task[key]))
                lines.extend(['otb_upm = 1000', 'otb_reference = "largest"', 'otb_rounding = "ties-even"'])
                lines.extend(f'{key} = {toml_value(value)}' for key, value in task.get('explicit', {}).items())
            elif task.get('explicit') and not task['explicit'].get('pcf_copy'):
                lines.append('[tasks.options]')
                lines.extend(f'{key} = {toml_value(value)}' for key, value in task['explicit'].items())
        else:
            assert kind in ['ttf-square', 'ttf-round', 'woff2-square', 'woff2-round']
            pixel = task['pixel_size']; ppu = 1000 // pixel
            lines.extend(['kind = "variants"', 'base = ' + json.dumps(task['inputs'][0]),
                          'output = ' + json.dumps(str(path)), 'family = ' + json.dumps(task['family']),
                          '[tasks.options]', 'style = ' + json.dumps(task['style']),
                          'legacy_style = ' + json.dumps(task['style']),
                          'full_name = ' + json.dumps(task['full_name']),
                          'postscript_name = ' + json.dumps(task['postscript_name']),
                          'shape = ' + json.dumps('dot' if task['shape'] == 'round' else 'square'),
                          'dotsize = 1.0', 'dot_reuse = "literal"', 'ppu = ' + str(ppu), 'upm = ' + str(ppu * pixel),
                          'woff2_source = "ttf"', 'font_version = ' + json.dumps(task['font_version']),
                          'copyright = ' + json.dumps(task['copyright'])])
            lines.extend(f'{key} = {toml_value(value)}' for key, value in task.get('explicit', {}).items())
    return '\n'.join(lines) + '\n'


def prepare_native_inputs(tasks, binary, rename_binary, output):
    if any(task.get('rename') for task in tasks) and rename_binary is None:
        raise ValueError('native catalogue requires --rename-binary for export_name policy')
    normalized = output / 'normalized'; normalized.mkdir()
    remapped = {}
    # The explicit profile converts an unrenamed PCF download directly from the
    # source PCF (downloads.py:261-267), so only other PCF uses need Unicode.
    sources = sorted({name for task in tasks for name in task['inputs'] if '.pcf' in name.lower()
                      and not task.get('explicit', {}).get('pcf_copy')})
    for index, source in enumerate(sources):
        target = normalized.resolve() / f'charset-{index}.bdf'
        command = [str(binary), 'charset', source, '--source-charset', 'iso8859-1',
                   '--to', 'unicode', '-o', str(target), '--report', 'json']
        with (normalized / f'charset-{index}.stdout').open('xb') as stdout, (normalized / f'charset-{index}.stderr').open('xb') as stderr:
            status = subprocess.run(command, stdout=stdout, stderr=stderr)
        if status.returncode:
            raise RuntimeError((status.returncode, (normalized / f'charset-{index}.stderr').read_text()))
        remapped[source] = str(target)
    jobs, renamed = [], {}
    for task in tasks:
        if not task.get('rename'): continue
        assert task['kind'] in ['bdf', 'pcf'] and len(task['inputs']) == 1
        source = task['inputs'][0]; key = (source, json.dumps(task['rename'], sort_keys=True))
        if key not in renamed:
            target = normalized.resolve() / f'export-name-{len(jobs)}.bdf'
            renamed[key] = str(target)
            jobs.append(dict(task['rename'], input=remapped.get(source, source), output=str(target)))
    if jobs:
        plan = normalized / 'export-name.manifest.json'
        plan.write_text(json.dumps({'version': 1, 'jobs': jobs}, indent=2) + '\n')
        with (normalized / 'export-name.stdout').open('xb') as stdout, (normalized / 'export-name.stderr').open('xb') as stderr:
            status = subprocess.run([str(rename_binary), str(plan.resolve())], stdout=stdout, stderr=stderr)
        if status.returncode:
            raise RuntimeError((status.returncode, (normalized / 'export-name.stderr').read_text()))
    prepared = []
    for task in tasks:
        task = dict(task)
        if task.get('rename'):
            source = task['inputs'][0]
            task['inputs'] = [renamed[source, json.dumps(task['rename'], sort_keys=True)]]
        elif not task.get('explicit', {}).get('pcf_copy'):
            task['inputs'] = [remapped.get(name, name) for name in task['inputs']]
        prepared.append(task)
    return prepared


def native_archives(all_families, tasks, output):
    for family in all_families:
        members = [(task['variant'] + '.bdf', gzip.decompress((output / task['file']).read_bytes()))
                   for task in tasks if task['slug'] == family['slug'] and task['kind'] == 'bdf']
        members.extend((name, (family['folder'] / name).read_bytes()) for name in family['license_files'])
        members.extend(family['extra_files']); members.append(('README.txt', family['readme'].encode()))
        with zipfile.ZipFile(output / (family['slug'] + '.zip'), 'w', zipfile.ZIP_DEFLATED) as zipped:
            for name, data in members:
                info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED; info.external_attr = 0o644 << 16
                zipped.writestr(info, data)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('engine', choices=['prepare', 'python', 'native'])
    p.add_argument('--reference', type=Path, required=True); p.add_argument('--samples', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True); p.add_argument('--binary', type=Path)
    p.add_argument('--rename-binary', type=Path); p.add_argument('--plan', type=Path)
    p.add_argument('--profile', choices=PROFILES, default='default',
                   help='native option profile; explicit adds the application projection options')
    a = p.parse_args()
    if a.engine == 'native' and (a.binary is None or a.plan is None):
        p.error('native requires --binary and --plan')
    if a.output.exists() and any(a.output.iterdir()):raise ValueError('fresh output directory required')
    a.output.mkdir(parents=True, exist_ok=True)
    temporary = a.output.resolve() / 'tmp'; temporary.mkdir()
    os.environ['TMPDIR'] = str(temporary); tempfile.tempdir = str(temporary)
    api = setup(a.reference); destination = a.output / 'downloads'; destination.mkdir()
    started = time.perf_counter_ns(); all_families = families(a.samples.resolve(), api)
    parse_ns = time.perf_counter_ns() - started
    if a.engine == 'prepare':
        tasks = inventory(api, all_families, destination)
        (a.output / 'plan.json').write_text(json.dumps({'version': PLAN_VERSION, 'tasks': tasks, 'families': 8, 'inputs': 34,
            'explicit_policy': 'Original app names/copyright/version; bitmap UPM 1000, largest physical strike, ties-even; export_name migration for BDF/PCF only',
            'separate_native_policies': 'Source .notdef, bearings/extents, line gap/underline, post/glyph identities, timestamps, name-platform records and inherited library flags are independently inventoried'}, indent=2) + '\n')
        print(json.dumps({'status': 'prepared', 'tasks': len(tasks), 'download_files': len(tasks) + 8})); return
    metrics = {'catalogue_parse': parse_ns}; stage = time.perf_counter_ns()
    if a.engine == 'python':
        build, downloads, _, _, vectorize, _ = api
        vectorize.build_vector_ttf = vectorize._build_vector_ttf_reference
        entries = []
        for family in all_families: entries.extend(downloads.build_downloads(**arguments(family, destination)))
        metrics['download_export'] = time.perf_counter_ns() - stage
        result = {'engine': 'original-python-downloads+fontTools-vector-oracle', 'entries': entries,
                  'warnings': {f['slug']: {v.id: font.warnings for font, v in f['fonts'] if font.warnings} for f in all_families}}
    else:
        assert a.binary and a.plan
        plan = json.loads(a.plan.read_text())
        if plan.get('version') != PLAN_VERSION:
            raise ValueError('regenerate catalogue plan: historical plans omit required application metadata')
        tasks = prepare_native_inputs(attach_profile(plan['tasks'], all_families, a.profile),
                                      a.binary, a.rename_binary, a.output)
        jobs = a.output / 'jobs.toml'; jobs.write_text(jobfile(tasks, destination.resolve()))
        with (a.output / 'batch.stdout').open('xb') as stdout, (a.output / 'batch.stderr').open('xb') as stderr:
            process = subprocess.run([str(a.binary), 'batch', str(jobs)], stdout=stdout, stderr=stderr)
        if process.returncode: raise RuntimeError((process.returncode, (a.output / 'batch.stderr').read_text()[-6000:]))
        metrics['batch_export'] = time.perf_counter_ns() - stage
        stage = time.perf_counter_ns(); native_archives(all_families, tasks, destination)
        metrics['download_archives'] = time.perf_counter_ns() - stage
        result = {'engine': 'native-batch+native-export-name-helper+same-family-packaging', 'profile': a.profile,
                  'batch': batch_header(a.output / 'batch.stdout', len(tasks)),
                  'full_json_report_file': 'batch.stdout', 'full_json_report_bytes': (a.output / 'batch.stdout').stat().st_size}
    metrics['pipeline_wall'] = time.perf_counter_ns() - started
    if a.plan:
        expected = {task['file'] for task in json.loads(a.plan.read_text())['tasks']} | {f['slug'] + '.zip' for f in all_families}
        actual = {p.name for p in destination.iterdir() if p.is_file()}
        if actual != expected: raise ValueError({'missing': sorted(expected - actual), 'extra': sorted(actual - expected)})
    result.update(metrics_ns=metrics, source_fonts=34, families=8, download_files=len(list(destination.iterdir())),
                  scope='all actual samples; source parsing/variant resolution, charset/export-name preprocessing, exports and packaging are timed. Native task grouping uses the frozen original-policy inventory; Python groups each run. Python vector compiler is the supplied fontTools oracle because opf_vector source was absent')
    print(json.dumps(result))

if __name__ == '__main__': main()
