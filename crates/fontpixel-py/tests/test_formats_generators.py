"""The generated format tables stay in sync with the rustdoc inventory.

tools/bindings/gen_formats_types.py writes src/formats_types.rs (the serde
`normalize` table) and tools/bindings/gen_formats_coverage.py writes
bindings/coverage/formats.toml. Their `--check` modes run in
tools/bindings/gate.sh; these tests exercise the failure paths on synthetic
inventories and, when a real inventory has been built, the up-to-date check.
"""
import json
import os
import subprocess
import sys

import pytest

from formats_support import REPO

TYPES = os.path.join(REPO, "tools", "bindings", "gen_formats_types.py")
COVERAGE = os.path.join(REPO, "tools", "bindings", "gen_formats_coverage.py")


def run(script, inventory, *args):
    return subprocess.run([sys.executable, script, str(inventory), *args], capture_output=True, text=True)


def write_inventory(tmp_path, items):
    path = tmp_path / "inventory.json"
    path.write_text(json.dumps({"items": items}))
    return path


def real_inventory():
    target = os.environ.get("FONTPIXEL_RUSTDOC_TARGET") or os.path.join(
        os.environ.get("CARGO_TARGET_DIR", os.path.join(REPO, "target")), "rustdoc-json")
    path = os.path.join(target, "inventory.json")
    return path if os.path.exists(path) else None


NEW_TYPE = {"path": "fontpixel::formats::fzx::FzxExtraOptions", "kind": "struct",
            "parent": "fontpixel::formats::fzx", "serde": ["Deserialize", "Serialize"]}
FZX_MODULE = {"path": "fontpixel::formats::fzx", "kind": "module", "parent": "fontpixel::formats"}


def test_types_check_fails_when_a_serde_type_is_added(tmp_path):
    inv = write_inventory(tmp_path, [FZX_MODULE, NEW_TYPE])
    r = run(TYPES, inv, "--check")
    assert r.returncode == 1 and "out of date" in r.stderr


def test_coverage_refuses_types_missing_from_the_normalize_table(tmp_path):
    inv = write_inventory(tmp_path, [FZX_MODULE, NEW_TYPE])
    r = run(COVERAGE, inv, "--check")
    assert r.returncode == 2 and "missing from formats_types.rs" in r.stderr


def test_coverage_refuses_non_serde_types_and_unbound_functions(tmp_path):
    handle = dict(NEW_TYPE, serde=[])
    r = run(COVERAGE, write_inventory(tmp_path, [FZX_MODULE, handle]), "--check")
    assert r.returncode == 2 and "does not derive Deserialize" in r.stderr
    fn = {"path": "fontpixel::formats::fzx::encode_more", "kind": "function", "parent": "fontpixel::formats::fzx"}
    r = run(COVERAGE, write_inventory(tmp_path, [FZX_MODULE, fn]), "--check")
    assert r.returncode == 2 and "no binding entry" in r.stderr


@pytest.mark.skipif(real_inventory() is None, reason="no rustdoc inventory; run tools/bindings/gate.sh")
def test_committed_tables_match_the_inventory():
    inv = real_inventory()
    for script in (TYPES, COVERAGE):
        r = run(script, inv, "--check")
        assert r.returncode == 0, r.stderr
