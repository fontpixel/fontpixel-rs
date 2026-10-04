#!/usr/bin/env python3
"""Portable workload preparation: no native subprocess or real font operation."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import phase6_perf_prepare as prepare
from phase6_perf_setup import PACKAGES, required_versions
import phase6_perf_setup as setup


class PreparationTests(unittest.TestCase):
    def test_merge_section_requires_only_its_actual_inputs(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);repo=root/'repo';repo.mkdir();work=root/'work';target=root/'frozen'
            for path in [target/'release/fontpixel',work/'reference/bdfparser/tests/fonts/unifont-13.0.04.bdf',
                         root/'samples/tong-pixel-sans/TongPixelSans14Large-Latin.bdf',root/'rustc']:
                path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(b'identity fixture only')
            out=root/'merge.json'
            argv=['prepare','--section','merge','--work',str(work),'--target',str(target),'--output',str(out)]
            with patch.object(prepare,'ROOT',repo),patch.object(prepare.sys,'argv',argv),patch.object(prepare.subprocess,'check_output',return_value=str(root/'rustc')+'\n') as call:
                prepare.main()
            self.assertEqual(call.call_args.args[0],['rustup','which','rustc'])
            self.assertEqual(call.call_count,1)
            config=json.loads(out.read_text());self.assertEqual(len(config['cases']),1)
            self.assertEqual(config['cases'][0]['engine'],'rust-cli')
            self.assertFalse(any('site-packages' in path or 'node_modules' in path for path in config['identity_paths']))
            self.assertNotIn('--max-total-bytes',config['cases'][0]['command'])

    def test_all_direct_oracle_pins_are_required_and_duplicates_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary)/'requirements.txt'
            text='\n'.join(name+'==1.2.3' for name in PACKAGES)+'\n'
            path.write_text(text);self.assertEqual(len(required_versions(path)),9)
            path.write_text(text+'fonttools==1.2.3\n')
            with self.assertRaises(ValueError):required_versions(path)
            path.write_text('fonttools==1.2.3\n')
            with self.assertRaises(ValueError):required_versions(path)

    def test_codec_section_uses_selected_pins_and_interpreter_package_directory(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);repo=root/'repo';repo.mkdir();work=root/'work';target=root/'frozen'
            python=root/'selected-python';node=root/'selected-node';rustc=root/'rustc'
            package_dir=root/'venv/lib/python3.99/site-packages';package_dir.mkdir(parents=True)
            for path in [python,node,rustc,target/'release/examples/phase6_timing',work/'bdfparser.mjs',
                         work/'js-tools/package-lock.json',work/'js-tools/node_modules/.bin/esbuild',
                         root/'samples/tong-pixel-sans/TongPixelSans14Large-SC.bdf']:
                path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(b'identity fixture only')
            (work/'reference/bdfparser').mkdir(parents=True)
            pins={name:'1.2.3' for name in PACKAGES};pin_file=work/'selected-pins.txt'
            pin_file.write_text('\n'.join(name+'=='+version for name,version in pins.items())+'\n')
            (root/'requirements-oracles.txt').write_text('old unrelated incomplete pins\n')
            oracle={'python':'test interpreter','package_directories':[str(package_dir)],'packages':pins,'loaded_native_libraries':[]}
            def output(command,**kwargs):
                if command[0]==str(python):return json.dumps(oracle)
                if command[-1]=='--version':return '0.27.7\n'
                self.assertEqual(command,['rustup','which','rustc']);return str(rustc)+'\n'
            destination=root/'codec.json'
            argv=['prepare','--section','codec','--work',str(work),'--target',str(target),'--python',str(python),
                  '--node',str(node),'--requirements',str(pin_file),'--output',str(destination)]
            with patch.object(prepare,'ROOT',repo),patch.object(prepare.sys,'argv',argv),patch.object(prepare.subprocess,'check_output',side_effect=output):
                prepare.main()
            config=json.loads(destination.read_text())
            self.assertEqual(len(config['cases']),4)
            self.assertIn(str(pin_file),config['identity_paths']);self.assertIn(str(package_dir),config['identity_paths'])
            self.assertNotIn(str(root/'requirements-oracles.txt'),config['identity_paths'])
            self.assertFalse(any('tong-export' in path or 'catalogue' in path for path in config['identity_paths']))

    def test_setup_explicit_requirements_do_not_change_existing_sibling_pins(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);python=root/'python';python.write_text('identity fixture')
            old=root/'requirements-oracles.txt';old.write_text('older protected requirements\n')
            pins={name:'1.2.3' for name in PACKAGES};selected=root/'selected.txt'
            selected.write_text('\n'.join(name+'=='+version for name,version in pins.items())+'\n')
            oracle={'python':'test','python_version':[3,12,0],'packages':pins,'prefix':'test','purelib':'test','platlib':'test'}
            out=root/'inventory.json'
            argv=['setup','--package',str(root),'--work',str(root/'work'),'--python',str(python),'--requirements',str(selected),'--output',str(out)]
            with patch.object(setup.sys,'argv',argv),patch.object(setup.subprocess,'check_output',return_value=json.dumps(oracle)):
                setup.main()
            result=json.loads(out.read_text());self.assertEqual(result['inputs']['requirements'],str(selected))
            self.assertEqual(result['required_versions'],pins);self.assertEqual(result['version_mismatches'],{})
            self.assertEqual(old.read_text(),'older protected requirements\n')


if __name__=='__main__':unittest.main()
