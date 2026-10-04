#!/usr/bin/env python3
"""Standard-library complete-task coverage checks, without font operations."""
import ast
import json
from pathlib import Path
import tempfile
import tomllib
from types import SimpleNamespace
import unittest
from phase6_tong import expected_fonts, jobs_toml

class WorkloadTests(unittest.TestCase):
    def test_tong_has_every_font_and_default_task_without_budget_override(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); bdf = root / 'bdf'; bdf.mkdir()
            for name in expected_fonts():
                if name.startswith('bdf/'): (bdf / Path(name).name).write_text('fixture path only')
            reference = SimpleNamespace(family_of=lambda spacing,size: f'Tong {spacing} {size}',
                names=lambda *a: dict(copyright='notice',licenseDescription='license',licenseInfoURL='url'))
            document = tomllib.loads(jobs_toml(bdf, root / 'out', reference))
            self.assertEqual(len(expected_fonts()), 84); self.assertEqual(len(document['tasks']), 64)
            self.assertNotIn('jobs', document)
            self.assertEqual(sum(t['kind']=='convert' for t in document['tasks']), 40)
            self.assertEqual(sum(t['kind']=='variants' for t in document['tasks']), 24)
            self.assertEqual(len({t['id'] for t in document['tasks']}), 64)
            for task in document['tasks']:
                self.assertNotIn('max_total_bytes', task.get('options', {}))
                self.assertNotIn('woff2_threads', task.get('options', {}))
                if task['kind'] == 'variants':
                    self.assertEqual(sorted(task['languages']), ['ZHT,ZHH', 'ja', 'ko'])
                    self.assertTrue(task['languages']['ZHT,ZHH'].endswith('-TC.bdf'))
            changed = tomllib.loads(jobs_toml(bdf, root / 'out', reference, 536870912))
            self.assertEqual(sum(t.get('options',{}).get('max_total_bytes')==536870912 for t in changed['tasks']),24)

    def test_outline_rotation_preserves_winding(self):
        from phase6_validate import minimal_rotation
        from itertools import product
        for length in range(7):
            for seq in product(range(3), repeat=length):
                values = list(seq)
                brute = min((tuple(values[i:]+values[:i]) for i in range(length)), default=())
                self.assertEqual(minimal_rotation(values), brute)
        self.assertNotEqual(minimal_rotation([1,2,3,4]),minimal_rotation([4,3,2,1]))

    def test_explicit_metadata_missing_copyright_and_ps_spaces_are_rejected(self):
        from phase6_catalogue_metadata import expected_names
        task = dict(file='fixture.ttf',copyright='Actual notice',family='Family',style='Bold u12b',
                    full_name='Family Bold u12b',postscript_name='Family-Boldu12b',font_version='1.000')
        values = {0:task['copyright'],1:task['family'],2:task['style'],4:task['full_name'],5:'Version 1.000',6:task['postscript_name']}
        def font(fields):
            return {'name': SimpleNamespace(names=[SimpleNamespace(nameID=k,toUnicode=lambda v=v:v) for k,v in fields.items()])}
        expected_names(task,font(values),'fixture')
        wrong=dict(values);wrong.pop(0)
        with self.assertRaises(AssertionError):expected_names(task,font(wrong),'fixture')
        wrong=dict(values);wrong[6]='Family-Bold u12b'
        with self.assertRaises(AssertionError):expected_names(task,font(wrong),'fixture')

    def test_python_sources_parse_without_executing_oracles(self):
        root = Path(__file__).resolve().parents[1]
        for source in [*root.joinpath('scripts').glob('phase6*.py'), *root.joinpath('benches').glob('phase6*.py')]:
            ast.parse(source.read_text(),filename=str(source))

if __name__=='__main__': unittest.main()
