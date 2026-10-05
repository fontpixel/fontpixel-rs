#!/usr/bin/env python3
"""Six lightweight reconstructed catalogue adapter regressions.
These do not execute production codecs or independent font consumers.
"""
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import tomllib
import unittest
from unittest.mock import patch
import phase6_catalogue as c

class CataloguePolicyTests(unittest.TestCase):
    def names(self):
        return c.app_names('FontPixel Sample', 'Bold u12b', {'copyright_': 'Source — licence (Author)'})

    def test_explicit_names_and_version(self):
        names = self.names()
        self.assertEqual(names['copyright'], 'Source — licence (Author)')
        self.assertEqual(names['font_version'], '1.000')
        self.assertEqual(names['full_name'], 'FontPixel Sample Bold u12b')
        self.assertEqual(names['postscript_name'], 'FontPixelSample-Boldu12b')
        self.assertEqual(c.app_names('X', 'Y', {'version': '2.345'})['font_version'], '2.345')

    def test_bitmap_task_preserves_all_application_fields_and_metric_profile(self):
        task = dict(self.names(), kind='ttf', inputs=['/input/a.bdf', '/input/b.bdf'], file='multi.ttf')
        actual = tomllib.loads(c.jobfile([task], Path('/output')))['tasks'][0]
        self.assertEqual(actual['family'], task['family']); self.assertEqual(actual['to'], 'otb')
        for key in ['style', 'full_name', 'postscript_name', 'font_version', 'copyright']:
            self.assertEqual(actual['options'][key], task[key])
        self.assertEqual(actual['options']['otb_upm'], 1000)
        self.assertEqual(actual['options']['otb_reference'], 'largest')
        self.assertEqual(actual['options']['otb_rounding'], 'ties-even')
        self.assertNotIn('fs_type', actual['options'])

    def test_vector_legacy_style_matches_resolved_application_style(self):
        task = dict(self.names(), kind='woff2-round', inputs=['/input/a.bdf'], file='v.woff2', shape='round', pixel_size=14)
        actual = tomllib.loads(c.jobfile([task], Path('/output')))['tasks'][0]
        self.assertEqual(actual['options']['legacy_style'], task['style'])
        self.assertEqual(actual['options']['postscript_name'], 'FontPixelSample-Boldu12b')
        self.assertEqual(actual['options']['full_name'], task['full_name'])
        self.assertEqual(actual['options']['upm'], 994)
        self.assertEqual(actual['options']['font_version'], '1.000')
        self.assertEqual(actual['options']['copyright'], task['copyright'])

    def test_rename_runs_once_for_bdf_and_pcf_and_never_changes_sfnt_input(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            rename = dict(export_name='FontPixel Sample', size=14, weight='regular',
                          weight_capitalized='Regular', width='normal', spacing='monospaced', script_subset=None)
            tasks = [dict(inputs=['/input/original.bdf'], kind=kind, rename=rename) for kind in ['bdf', 'pcf']]
            tasks += [dict(inputs=['/input/original.bdf'], kind='ttf')]
            with patch.object(c.subprocess, 'run', return_value=SimpleNamespace(returncode=0)) as run:
                actual = c.prepare_native_inputs(tasks, Path('/bin/fontpixel'), Path('/bin/rename'), root)
            self.assertEqual(run.call_count, 1); self.assertEqual(actual[0]['inputs'], actual[1]['inputs'])
            self.assertNotEqual(actual[0]['inputs'], tasks[0]['inputs'])
            self.assertEqual(actual[2]['inputs'], tasks[2]['inputs'])
            plan = json.loads((root / 'normalized/export-name.manifest.json').read_text())
            self.assertEqual(plan['version'], 1); self.assertEqual(len(plan['jobs']), 1)
            self.assertEqual(plan['jobs'][0]['weight_capitalized'], 'Regular')
            self.assertEqual(plan['jobs'][0]['input'], '/input/original.bdf')

    def test_missing_helper_fails_before_any_subprocess(self):
        with tempfile.TemporaryDirectory() as td, patch.object(c.subprocess, 'run') as run:
            with self.assertRaisesRegex(ValueError, 'rename-binary'):
                c.prepare_native_inputs([dict(inputs=['x.bdf'], kind='bdf', rename={'export_name': 'X'})],
                                        Path('/bin/fontpixel'), None, Path(td))
            run.assert_not_called()

    def test_inventory_captures_bitmap_copyright_and_resolved_rename_policy(self):
        font = SimpleNamespace(path=Path('/input/a.bdf'), pixel_size=14)
        variant = SimpleNamespace(size=14, weight='regular', width='normal', spacing='monospaced', script_subset=None)
        meta = SimpleNamespace(export_name='FontPixel Sample', name='Reserved')
        family = dict(slug='sample', folder=Path('/input'), meta=meta, fonts=[(font, variant)],
                      license_files=[], extra_files=[], readme='', copyright='Sample licence (Author)')
        vectorize = SimpleNamespace(build_vector_ttf=lambda *a, **k: None)
        downloads = SimpleNamespace(_source_bdf_bytes=lambda f: b'source', _pcf_bytes=lambda *a, **kw: None,
                                    build_ttf=lambda *a, **kw: None, _woff2=lambda *a, **kw: None,
                                    _entry=lambda *a, **kw: None)
        def build_downloads(**args):
            downloads._source_bdf_bytes(font)
            downloads._entry('sample', 'v', 'bdf', 'sample.bdf.gz', b'', args['out'])
            downloads._entry('sample', 'v', 'pcf', 'sample.pcf.gz', b'', args['out'])
            downloads.build_ttf([font], 'FontPixel Sample', 'Monospaced', args['out'] / 'fake.ttf', copyright_=args['copyright_line'])
            downloads._entry('sample', None, 'ttf', 'sample.ttf', b'', args['out'])
        downloads.build_downloads = build_downloads
        api = (None, downloads, None, None, vectorize, None)
        with tempfile.TemporaryDirectory() as td: tasks = c.inventory(api, [family], Path(td))
        self.assertEqual(tasks[0]['rename']['export_name'], 'FontPixel Sample')
        self.assertEqual(tasks[1]['rename'], tasks[0]['rename']); self.assertNotIn('rename', tasks[2])
        self.assertEqual(tasks[2]['copyright'], family['copyright'])
        self.assertEqual(tasks[2]['font_version'], '1.000')

    def explicit_tasks(self):
        bdf = SimpleNamespace(path=Path('/input/a.bdf'), pixel_size=13, ascent=10, descent=3,
                              props={}, family_slug='sample', merged=False)
        pcf = SimpleNamespace(path=Path('/input/b.pcf.gz'), pixel_size=13, ascent=11, descent=2,
                              props={'FONT': '-w-b-medium-r-normal--13-80-96-96-c-70-iso8859-1'},
                              family_slug='sample', merged=False)
        families = [dict(fonts=[(bdf, None), (pcf, None)])]
        tasks = [dict(inputs=['/input/a.bdf'], kind='bdf', file='a.bdf.gz'),
                 dict(inputs=['/input/a.bdf'], kind='pcf', file='a.pcf.gz'),
                 dict(inputs=['/input/b.pcf.gz'], kind='bdf', file='b.bdf.gz'),
                 dict(inputs=['/input/b.pcf.gz'], kind='pcf', file='b.pcf.gz'),
                 dict(self.names(), inputs=['/input/a.bdf'], kind='ttf-round', file='v.ttf', shape='round',
                      pixel_size=13)]
        return c.attach_profile(tasks, families, 'explicit')

    def test_default_profile_adds_nothing(self):
        tasks = [dict(inputs=['/input/a.bdf'], kind='bdf', file='a.bdf.gz')]
        self.assertIs(c.attach_profile(tasks, [], 'default'), tasks)
        self.assertNotIn('options', tomllib.loads(c.jobfile(tasks, Path('/output')))['tasks'][0])

    def test_explicit_profile_reproduces_original_bitmap_and_vector_policy(self):
        tasks = self.explicit_tasks()
        jobs = tomllib.loads(c.jobfile(tasks, Path('/output')))['tasks']
        template = '-opf-sample-medium-r-normal--13-130-75-75-p-130-iso10646-1'
        self.assertEqual(jobs[0]['options'], {
            'swidth_ppem': 13, 'point_size': 13, 'resolution': '75,75', 'font_name': template,
            'set_integer_property': ['PIXEL_SIZE=13', 'FONT_ASCENT=10', 'FONT_DESCENT=3'],
            'remove_property': ['FONT']})
        self.assertEqual(jobs[1]['options']['pcf_metadata_precedence'], 'explicit-properties')
        self.assertNotIn('remove_property', jobs[1]['options'])
        # A PCF source keeps its own FONT property as the BDF FONT name.
        self.assertEqual(jobs[2]['options']['font_name'], '-w-b-medium-r-normal--13-80-96-96-c-70-iso8859-1')
        # The original copies an unrenamed PCF: direct PCF conversion, no options.
        self.assertEqual((jobs[3]['from'], jobs[3]['to']), ('pcf', 'pcf'))
        self.assertNotIn('options', jobs[3])
        vector = jobs[4]['options']
        self.assertEqual((vector['notdef'], vector['notdef_advance'], vector['notdef_lsb'], vector['vendor']),
                         ('empty', 494, 0, '????'))

    def test_explicit_profile_skips_charset_step_only_for_the_copied_pcf(self):
        with tempfile.TemporaryDirectory() as td, \
                patch.object(c.subprocess, 'run', return_value=SimpleNamespace(returncode=0)) as run:
            prepared = c.prepare_native_inputs(self.explicit_tasks(), Path('/bin/fontpixel'), None, Path(td))
        self.assertEqual(run.call_count, 1)
        self.assertEqual(prepared[3]['inputs'], ['/input/b.pcf.gz'])
        self.assertTrue(prepared[2]['inputs'][0].endswith('charset-0.bdf'))

if __name__ == '__main__': unittest.main()
