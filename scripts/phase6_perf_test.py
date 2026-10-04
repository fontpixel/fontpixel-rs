#!/usr/bin/env python3
"""Recorder and adapter contracts, with tiny synthetic subprocesses only."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'benches'))
import phase6_perf as p
from phase6_support import batch_header


class RecorderTests(unittest.TestCase):
    def test_dotted_case_ids_and_five_observations_are_unique(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); records = []
            for case in ['case.sfn', 'case.asc']:
                for i in range(1, 6):
                    command = [sys.executable, '-c', 'import json;print(json.dumps({"metrics_ns":{"one_call":12}}))']
                    r = p.run_command(command, root / f'{case}-rust-{i}')
                    r.update(case=case, engine='rust', repetition=i, internal_json=True)
                    r['metrics_ns'] = p.metrics_from_log(r); records.append(r)
            self.assertEqual(p.verify_logs(records)['unique_streams'], 20)
            self.assertEqual(len(p.summarize(records, 5)), 2)
            with self.assertRaises(ValueError): p.verify_logs(records + [records[0]])
            with self.assertRaises(ValueError): p.summarize(records[:-1], 5)
            Path(records[0]['stdout']).write_text('tampered')
            with self.assertRaises(ValueError): p.verify_logs(records)

    def test_ambiguous_case_engine_stems_fail_before_freezing(self):
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); config = root / 'config.json'
            config.write_text(json.dumps({'cases': [{'id': 'a-b', 'engine': 'c'}, {'id': 'a', 'engine': 'b-c'}]}))
            with self.assertRaisesRegex(ValueError, 'collide'):
                p.freeze(SimpleNamespace(config=config, output=root / 'manifest.json'))
            self.assertFalse((root / 'manifest.json').exists())

    def test_existing_stderr_is_rejected_without_truncating_stdout(self):
        with tempfile.TemporaryDirectory() as temporary:
            stem = Path(temporary) / 'case.sfn'
            Path(str(stem) + '.stderr').write_text('keep')
            with self.assertRaises(FileExistsError): p.run_command([sys.executable, '-c', 'print(1)'], stem)
            self.assertFalse(Path(str(stem) + '.stdout').exists())
            self.assertEqual(Path(str(stem) + '.stderr').read_text(), 'keep')

    def test_snapshot_rejects_changed_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'input'; path.write_text('a')
            frozen = p.snapshot([path]); self.assertEqual(p.verify_snapshot(frozen), 1)
            path.write_text('b')
            with self.assertRaises(ValueError): p.verify_snapshot(frozen)

    def test_archive_hash_and_member_inventory(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); data = root / 'data'; data.mkdir()
            (data / 'font').write_bytes(b'actual bytes'); archive = p.archive_outputs(data, root / 'out.zip')
            self.assertEqual(p.verify_archive({'outputs': archive}), 1)
            archive['files']['font']['sha256'] = '0' * 64
            with self.assertRaises(ValueError): p.verify_archive({'outputs': archive})

    def test_invalid_metrics_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'stdout'; rec = dict(stdout=str(path), internal_json=True, process_wall_ns=1)
            for value in [True, -1, float('nan'), float('inf'), '12']:
                path.write_text(json.dumps({'metrics_ns': {'call': value}}))
                with self.assertRaises(ValueError): p.metrics_from_log(rec)
            path.write_text(json.dumps({'metrics_ns': {'process_wall_ns': 2}}))
            with self.assertRaises(ValueError): p.metrics_from_log(rec)

    def test_independent_audit_from_raw_streams(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); input = root / 'input'; input.write_text('frozen')
            manifest = root / 'manifest.json'
            manifest.write_text(json.dumps({'files': p.snapshot([input]), 'config': {'cases': [{'id': 'case.sfn', 'engine': 'fake'}]}}))
            records = []
            for i, n in enumerate([5, 1, 8, 3, 2], 1):
                stdout, stderr = root / f'case.sfn-{i}.stdout', root / f'case.sfn-{i}.stderr'
                stdout.write_text(json.dumps({'metrics_ns': {'call': n}})); stderr.write_text('')
                directory = root / str(i); directory.mkdir(); (directory / 'output').write_text(str(n))
                records.append(dict(case='case.sfn', engine='fake', repetition=i, exit=0, internal_json=True,
                    stdout=str(stdout), stderr=str(stderr), stdout_sha256=p.digest(stdout), stderr_sha256=p.digest(stderr),
                    process_wall_ns=n+10, metrics_ns={'process_wall_ns': n+10, 'call': n}, peak_rss_kib=10,
                    outputs=p.archive_outputs(directory, root / f'{i}.zip')))
            result = dict(formal=True, runs=5, manifest=str(manifest), manifest_sha256=p.digest(manifest),
                          records=records, summary=p.summarize(records, 5))
            result_path = root / 'result.json'; result_path.write_text(json.dumps(result))
            command = [sys.executable, str(Path(__file__).with_name('phase6_perf_audit.py')), str(result_path)]
            good = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(good.returncode, 0, good.stderr)
            result['summary'][0]['metrics_ns']['call']['median'] = 999
            result_path.write_text(json.dumps(result)); bad = subprocess.run(command, capture_output=True)
            self.assertNotEqual(bad.returncode, 0)


class HeaderTests(unittest.TestCase):
    def header(self):
        return {'version': 1, 'success': True, 'exit_code': 0, 'jobs': 9, 'workers': 4,
                'summary': dict(total=173, succeeded=173, failed=0, committed=173)}

    def test_bounded_header_and_incomplete_utf8_tail(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'batch.stdout'; header = self.header()
            prefix = (json.dumps(header)[:-1] + ',"tasks":[').encode()
            path.write_bytes(prefix + b'"' + b'x' * (65535-len(prefix)-1) + '永'.encode() + b'"]}')
            self.assertEqual(batch_header(path, 173), header)

    def test_failed_or_wrong_count_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'batch.stdout'; header = self.header(); header['success'] = False
            path.write_text(json.dumps(dict(header, tasks=[])))
            with self.assertRaises(ValueError): batch_header(path, 173)
            header['success'] = True; path.write_text(json.dumps(dict(header, tasks=[])))
            with self.assertRaises(ValueError): batch_header(path, 172)

if __name__ == '__main__': unittest.main()
