#!/usr/bin/env python3
"""Reproducible feature-isolated W5.D gates (original MIT test orchestration)."""
import json, os, subprocess, time, sys
from pathlib import Path
root = Path(__file__).resolve().parents[2]
alltargets = "--all-targets-only" in sys.argv
records = []
def run(name, command, env=None):
    start = time.time()
    with (root/'reports/w5.d'/f'{name}.log').open('wb') as log:
        result = subprocess.run(command, cwd=root, stdout=log, stderr=subprocess.STDOUT, env=env)
    record = dict(name=name, command=command, exit=result.returncode, elapsed_seconds=time.time()-start)
    records.append(record)
    (root/'reports/w5.d'/('alltargets-gates.json' if alltargets else 'gates.json')).write_text(json.dumps(records, indent=2)+'\n')
    print(json.dumps(record), flush=True)
    if result.returncode: raise SystemExit(result.returncode)
for feature in ['fbit','lisa','mgtk','qpf','qpf2','nfnt']:
    base = ['-p','fontpixel','--no-default-features','--features',feature]
    if alltargets:
        run(f'alltargets-test-{feature}', ['cargo','test',*base,'--all-targets'])
        continue
    run(f'test-{feature}', ['cargo','test',*base,'--lib','--test',feature])
    run(f'clippy-{feature}', ['cargo','clippy',*base,'--all-targets','--','-D','warnings'])
    env = os.environ.copy(); env['RUSTDOCFLAGS'] = '-D warnings'
    run(f'rustdoc-{feature}', ['cargo','doc',*base,'--no-deps'], env)
if alltargets: raise SystemExit(0)
run('example-build',['cargo','build','-p','fontpixel','--no-default-features','--features','fbit,lisa,mgtk,qpf,qpf2','--example','w5d_timing'])
run('clippy-combined-final',['cargo','clippy','-p','fontpixel','--no-default-features','--features','fbit,lisa,mgtk,qpf,qpf2','--lib','--tests','--example','w5d_timing','--','-D','warnings'])
