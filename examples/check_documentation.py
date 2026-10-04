#!/usr/bin/env python3
"""Inventory documentation; optionally query CLI help/registry, without font smokes."""
from __future__ import annotations
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]
EXPECTED_COMMANDS = {"formats", "info", "convert", "preview", "render", "vectorize",
                     "variants", "subset", "diff", "validate", "charset", "merge", "batch"}


def check():
    catalogue = (ROOT / "FORMATS.md").read_text()
    ids = [name for row in catalogue.splitlines() if row.startswith("| `")
           for name in re.findall(r"`([^`]+)`", row.split("|")[1])]
    assert len(ids) == len(set(ids)) == 99, "canonical catalogue count changed"
    index = (ROOT / "docs/formats/README.md").read_text()
    for name in ids:
        document = ROOT / "docs/formats" / f"{name}.md"
        assert document.is_file() and len(document.read_text()) >= 500, name
        assert f"[`{name}`]({name}.md)" in index, f"index missing {name}"
    readme = (ROOT / "README.md").read_text()
    for command in EXPECTED_COMMANDS:
        assert f"fontpixel {command}" in readme, f"README missing {command} example"
    spec = importlib.util.spec_from_file_location("tong_batch_example", ROOT / "examples/tong_batch.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    for sizes, kwargs, expected in [(["Large", "Small"], {}, 84),
                                   (["Large", "Small"], {"only": "otb"}, 20),
                                   (["Large", "Small"], {"only": "square"}, 72),
                                   (["Large", "Small"], {"only": "dot"}, 72),
                                   (["Large", "Small"], {"skip_otf": True}, 76),
                                   (["Large"], {}, 42), (["Small"], {}, 42),
                                   (["Small"], {"only": "dot", "skip_otf": True}, 34)]:
        rows = module.tasks(Path("input"), Path("output"), "1.0", sizes, **kwargs)
        assert len(rows) == expected, (kwargs,len(rows))
        assert len({row['id'] for row in rows}) == expected
        assert len({row['output'] for row in rows}) == expected
        text = "version = 1\njobs = 2\n"
        for row in rows:
            text += "\n[[tasks]]\n" + "".join(f"{key} = {module.value(value)}\n" for key,value in row.items())
        assert tomllib.loads(text)['tasks'] == rows
    manifest = ROOT.parent / "reference/fontpixel/ingest/manifest.toml"
    inventory = json.loads((ROOT / "docs/migration-manifest-inventory.json").read_text())
    migration = (ROOT / "docs/migration-fontpixel.md").read_text()
    for key in inventory['family_keys']:
        assert re.search(r"`" + re.escape(key) + r"(?:`|\s*=)", migration), f"migration missing family key {key}"
    for key in inventory['instance_keys']:
        assert f"`instances[].{key}`" in migration, f"migration missing instance key {key}"
    for key in inventory['license_keys']:
        assert f"`license.{key}`" in migration, f"migration missing license key {key}"
    for key in inventory['variant_override_keys']:
        assert f"`{key}`" in migration, f"migration missing variant key {key}"
    if manifest.is_file():
        assert hashlib.sha256(manifest.read_bytes()).hexdigest() == inventory['manifest_sha256'], 'reference manifest changed'
        data = tomllib.loads(manifest.read_text())
        assert len(data['family']) == inventory['active_families']
        assert len(data.get('todo',[])) == inventory['pending_entries']
        assert sorted({key for family in data['family'] for key in family}) == inventory['family_keys']
    return ids


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,help='optional current all-feature fontpixel executable')
    args=parser.parse_args()
    ids=check()
    if args.binary:
        binary=args.binary.resolve()
        helptext=subprocess.run([str(binary),'--help'],check=True,capture_output=True,text=True).stdout
        for command in EXPECTED_COMMANDS:
            assert re.search(rf'^  {command}\s',helptext,re.M) or (command == 'vectorize' and 'vectorize' in helptext),f'CLI missing {command}'
        formats=json.loads(subprocess.run([str(binary),'formats','--json'],check=True,capture_output=True,text=True).stdout)['formats']
        assert {row['format']['id'] for row in formats} == set(ids)
        assert all(row['read'] and row['write'] for row in formats), 'binary must have all-format features'
        for command in sorted(EXPECTED_COMMANDS):
            subprocess.run([str(binary),command,'--help'],check=True,capture_output=True,text=True)
    print('Documentation source inventory: 99 format documents, 13 command examples, manifest keys, Tong task matrix.')
    print('Document existence/length and source text are not semantic profile or native font validation.')
    if args.binary:
        print('CLI help and registry queries passed; no font conversion smoke was run.')
    else:
        print('Native CLI execution: not run.')


if __name__ == '__main__':
    main()
