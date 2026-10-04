#!/usr/bin/env python3
"""UNTESTED RECOVERY of independent verifier; no recorder imports.
Reconstruct five-run medians from raw logs, verify every frozen file and every
archive member. No raw evidence is supplied by this recovery packet.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import zipfile


def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('result', type=Path)
    parser.add_argument('--allow-smoke', action='store_true')
    args = parser.parse_args()
    result = json.loads(args.result.read_text())
    runs = result['runs']
    assert runs == 5 and result['formal'] or args.allow_smoke and runs == 1 and not result['formal']
    assert sha(result['manifest']) == result['manifest_sha256']
    manifest = json.loads(Path(result['manifest']).read_text())
    frozen = 0
    for name, identity in manifest['files'].items():
        path = Path(name)
        assert path.stat().st_size == identity['bytes'] and sha(path) == identity['sha256'], name
        frozen += 1
    seen, groups, file_count = set(), {}, 0
    for record in result['records']:
        assert record['exit'] == 0
        for kind in ['stdout', 'stderr']:
            path = Path(record[kind]).resolve()
            assert path not in seen, f'reused stream {path}'
            seen.add(path)
            assert sha(path) == record[kind + '_sha256'], str(path)
        metrics = {'process_wall_ns': record['process_wall_ns']}
        if record['internal_json']:
            raw = json.loads(Path(record['stdout']).read_text())
            assert 'process_wall_ns' not in raw['metrics_ns']
            metrics.update(raw['metrics_ns'])
        assert all(not isinstance(n, bool) and isinstance(n, (int, float)) and math.isfinite(n) and n >= 0 for n in metrics.values())
        assert metrics == record['metrics_ns']
        archive = record['outputs']
        assert sha(archive['path']) == archive['sha256']
        with zipfile.ZipFile(archive['path']) as zipped:
            assert len(zipped.namelist()) == len(archive['files']) and set(zipped.namelist()) == set(archive['files'])
            for name, identity in archive['files'].items():
                digest, length = hashlib.sha256(), 0
                with zipped.open(name) as source:
                    for chunk in iter(lambda: source.read(1024 * 1024), b''):
                        digest.update(chunk); length += len(chunk)
                assert length == identity['bytes'] and digest.hexdigest() == identity['sha256'], name
                file_count += 1
        groups.setdefault((record['case'], record['engine']), []).append((record, metrics))
    independent = []
    for (case, engine), observations in sorted(groups.items()):
        assert len(observations) == runs and sorted(r['repetition'] for r, _ in observations) == list(range(1, runs + 1))
        keys = set(observations[0][1])
        assert all(set(values) == keys for _, values in observations)
        summary = {}
        for key in keys:
            values = sorted(values[key] for _, values in observations)
            summary[key] = {'median': values[len(values) // 2], 'min': values[0], 'max': values[-1]}
        rss = sorted(record['peak_rss_kib'] for record, _ in observations)
        independent.append({'case': case, 'engine': engine, 'metrics_ns': summary,
                            'peak_rss_kib': rss[-1], 'median_peak_rss_kib': rss[len(rss) // 2]})
    assert independent == result['summary'], 'independent summary differs'
    assert {(c['id'], c['engine']) for c in manifest['config']['cases']} == set(groups)
    print(json.dumps({'status': 'pass', 'result_sha256': sha(args.result), 'formal': result['formal'],
                      'frozen_files_sha_verified': frozen, 'invocations': len(result['records']),
                      'unique_raw_streams_sha_verified': len(seen), 'archived_output_streams_sha_verified': file_count,
                      'independently_recomputed_rows': len(independent)}))

if __name__ == '__main__': main()
