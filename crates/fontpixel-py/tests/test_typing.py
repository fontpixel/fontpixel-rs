"""Type stubs, the generated reference and docstrings.

- the typed smoke file runs, and it plus every example pass ``mypy --strict``;
- ``mypy.stubtest`` accepts the stubs against the built extension;
- the committed stubs, ``fontpixel/types.py``, the stubtest allowlist and the
  API reference are what the generators produce now (``--check``);
- the ``Literal``/``TypedDict`` shapes of ``fontpixel.types`` agree with the
  serde shapes the extension accepts;
- every public module, class, function, method and property has a docstring.

The mypy-based tests are skipped when mypy is not installed; the stub check
needs the rustdoc JSON built by ``tools/bindings/gate.sh`` (directory from
``FONTPIXEL_RUSTDOC_TARGET``, else ``target/rustdoc-json``).
"""
from __future__ import annotations

import os
import re
import subprocess
import sys
import typing
from pathlib import Path

import pytest

import fontpixel as fp
import fontpixel.types as T

CRATE = Path(__file__).resolve().parents[1]
REPO = CRATE.parents[1]
TOOLS = REPO / "tools" / "bindings"
sys.path.insert(0, str(TOOLS))

import pyapi  # noqa: E402

try:
    import mypy  # noqa: F401
    HAVE_MYPY = True
except ImportError:  # pragma: no cover - depends on the environment
    HAVE_MYPY = False
needs_mypy = pytest.mark.skipif(not HAVE_MYPY, reason="mypy is not installed")


def rustdoc_dir() -> Path:
    target = os.environ.get("FONTPIXEL_RUSTDOC_TARGET") or os.path.join(
        os.environ.get("CARGO_TARGET_DIR", str(REPO / "target")), "rustdoc-json")
    return Path(target) / "doc"


def run(args: list[str], cwd: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, *args], cwd=cwd, capture_output=True, text=True)


def test_typing_smoke_runs():
    sys.path.insert(0, str(Path(__file__).parent))
    import typing_smoke
    typing_smoke.main()


@needs_mypy
def test_mypy_strict_accepts_the_smoke_file_and_examples(tmp_path):
    files = sorted(str(p) for p in (CRATE / "examples").glob("*.py"))
    files.append(str(CRATE / "tests" / "typing_smoke.py"))
    proc = run(["-m", "mypy", "--strict", "--cache-dir", str(tmp_path), *files], cwd=tmp_path)
    assert proc.returncode == 0, proc.stdout + proc.stderr
    assert "Success: no issues found" in proc.stdout


@needs_mypy
def test_mypy_strict_rejects_wrong_option_shapes(tmp_path):
    bad = tmp_path / "bad.py"
    bad.write_text(
        "import fontpixel as fp\n"
        "from fontpixel.formats import psf\n"
        "fs = fp.FontSet()\n"
        "psf.write(fs, {'version': '3'})\n"
        "fp.Font().draw_all('codepoint')\n"
        "fp.write(fs, 'pcf', strict='yes')\n")
    proc = run(["-m", "mypy", "--strict", "--cache-dir", str(tmp_path / "cache"), str(bad)], cwd=tmp_path)
    assert proc.returncode == 1
    assert proc.stdout.count("error:") == 3, proc.stdout


@needs_mypy
def test_stubtest_accepts_the_stubs():
    proc = run([str(TOOLS / "stubtest.py"), "--concise"], cwd=REPO)
    assert proc.returncode == 0, proc.stdout + proc.stderr


def test_generated_stubs_are_current():
    doc = rustdoc_dir()
    if not (doc / "fontpixel.json").exists():
        pytest.skip(f"no rustdoc JSON in {doc}; run tools/bindings/gate.sh")
    proc = run([str(TOOLS / "gen_pyi.py"), "--rustdoc", str(doc), "--check", "--strict"], cwd=REPO)
    assert proc.returncode == 0, proc.stdout + proc.stderr


def test_api_reference_is_current():
    proc = run([str(TOOLS / "gen_pyref.py"), "--check"], cwd=REPO)
    assert proc.returncode == 0, proc.stdout + proc.stderr


def test_every_public_callable_has_a_docstring():
    entries = list(pyapi.walk())
    documented = [e for e in entries if e.kind not in ("constant", "alias")]
    assert len(documented) > 1000
    missing = [f"{e.kind} {e.name}" for e in documented if not e.doc.strip()]
    assert missing == []


def test_every_native_module_has_a_stub():
    import gen_pyi
    stubs = {rel for _, (_, rel) in gen_pyi.native_modules().items()}
    pkg = CRATE / "python" / "fontpixel"
    assert all((pkg / rel).exists() for rel in stubs)
    assert {"_native/__init__.pyi", "formats/__init__.pyi", "sfnt/cmap.pyi", "woff/woff2.pyi"} <= stubs


# ---------------------------------------------------------------- serde shapes

def serde_expected(message: str) -> set[str]:
    """Names a serde error message lists after "expected" (none for empty structs)."""
    if "there are no fields" in message:
        return set()
    tail = message.split("expected", 1)[1]
    return set(re.findall(r"`([^`]*)`", tail))


def test_literal_aliases_match_the_serde_variants():
    checked = 0
    for name in fp.formats.option_types():
        alias = T.SERDE_TYPES[name]
        if typing.get_origin(alias) is not typing.Literal:
            continue
        with pytest.raises(ValueError) as info:
            fp.formats.normalize(name, "zz-not-a-variant")
        assert set(typing.get_args(alias)) == serde_expected(str(info.value)), name
        for value in typing.get_args(alias):     # input aliases normalize to a main name
            assert fp.formats.normalize(name, value) in typing.get_args(alias)
        checked += 1
    assert checked > 100


def test_typeddicts_match_the_serde_fields():
    checked = 0
    for name in fp.formats.option_types():
        shape = T.SERDE_TYPES[name]
        if not typing.is_typeddict(shape):
            continue
        keys = shape.__required_keys__ | shape.__optional_keys__
        with pytest.raises(ValueError) as info:
            fp.formats.normalize(name, {"zz_not_a_field": 1})
        if "unknown field" in str(info.value):
            assert serde_expected(str(info.value)) == set(keys), name
        try:
            defaults = fp.formats.normalize(name)
        except ValueError:
            continue          # required fields: no defaults to compare
        assert set(defaults) <= keys and shape.__required_keys__ <= set(defaults), name
        checked += 1
    assert checked > 150


def test_every_option_type_is_indexed():
    assert set(fp.formats.option_types()) <= set(T.SERDE_TYPES)
    assert T.SERDE_TYPES["formats::pcf::PcfWriteOptions"] is T.PcfWriteOptions
    assert typing.get_args(T.SERDE_TYPES["formats::psf::Version"]) == ("1", "2")
