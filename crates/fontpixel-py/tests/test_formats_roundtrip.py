"""Every catalogue format: write -> read back, compared with the frozen CLI.

Expected values are the SHA-256 hashes in data/formats/cli_golden.json, written by
the frozen fontpixel CLI (see data/formats/regen_cli_golden.py) for the same
source font and options. Each test performs the operation through the Python
bindings and requires byte-identical assets and an identical read-back model
(compared as the YAFF both sides write for it).
"""
import inspect

import pytest

import fontpixel
import fontpixel.formats as F
from formats_support import (
    CLI_IMPLICIT,
    FIXTURES,
    REPO,
    SOURCES,
    case,
    load_golden,
    primary_name,
    sha,
    source_bytes,
)

GOLDEN = load_golden()
STATUS = {s["format"]["id"]: s for s in fontpixel.list_formats()}
FORMAT_IDS = list(STATUS)


def namespace(fid):
    for name in F.MODULES:
        if fid in getattr(F, name).FORMAT_IDS:
            return getattr(F, name)
    raise AssertionError(f"no namespace serves {fid}")


def yaff_of(loaded):
    return fontpixel.write(loaded.fonts, "yaff", report=loaded.report).into_single()


def load_source(fid):
    c = case(fid)
    data = source_bytes(fid)
    if c.source == "native":
        return fontpixel.read(data, fid, c.source_read or None, name=primary_name(STATUS[fid]))
    return fontpixel.read(data)


def write_case(fid):
    c = case(fid)
    loaded = load_source(fid)
    return fontpixel.write(
        loaded.fonts,
        fid,
        c.write or None,
        primary_name=primary_name(STATUS[fid]),
        report=loaded.report,
    )


def test_catalogue_matches_cli_listing():
    assert len(FORMAT_IDS) == 99
    assert set(GOLDEN["formats"]) == set(FORMAT_IDS)
    assert fontpixel.list_formats() == GOLDEN["listing"]
    assert all(s["read"] and s["write"] for s in STATUS.values())


@pytest.mark.parametrize("fid", FORMAT_IDS)
def test_roundtrip_matches_cli(fid):
    g = GOLDEN["formats"][fid]
    c = case(fid)
    if "source" in g:
        assert sha(yaff_of(load_source(fid))) == g["source"]["yaff"]
    written = write_case(fid)
    assert {k: sha(v) for k, v in written.assets.items()} == g["write"]["assets"]
    back = fontpixel.read(written, fid, c.read or None)
    assert back.format == fid
    assert sha(yaff_of(back)) == g["readback"]["yaff"]


@pytest.mark.parametrize("fid", FORMAT_IDS)
def test_plain_conversion_matches_cli(fid):
    """Flag-free conversion of the Unicode 3x5 font: same assets or both fail."""
    g = GOLDEN["formats"][fid]["plain"]
    loaded = fontpixel.read(open(SOURCES["uni3"], "rb").read())
    args = dict(primary_name=primary_name(STATUS[fid]), report=loaded.report)
    if "assets" in g:
        w = fontpixel.write(loaded.fonts, fid, CLI_IMPLICIT.get(fid), **args)
        assert {k: sha(v) for k, v in w.assets.items()} == g["assets"]
    else:
        with pytest.raises(fontpixel.FontpixelError) as e:
            fontpixel.write(loaded.fonts, fid, CLI_IMPLICIT.get(fid), **args)
        assert e.value.kind in ("InvalidInput", "ReportedErrors", "StrictLoss")


def _params(fn):
    return inspect.signature(fn).parameters


@pytest.mark.parametrize("fid", FORMAT_IDS)
def test_direct_namespace_matches_registry(fid):
    """`fontpixel.formats.<module>` detect/read/write agree with the registry."""
    mod = namespace(fid)
    c = case(fid)
    g = GOLDEN["formats"][fid]
    written = write_case(fid)
    primary = written.output.primary
    assets = written.assets
    data = assets.get(primary)
    resources = {k: v for k, v in assets.items() if k != primary}
    if written.output.is_directory:
        resources = {k[len(primary) + 1:]: v for k, v in assets.items()}

    # detect
    if hasattr(mod, "detect") and STATUS[fid]["detect"] and data is not None:
        params = _params(mod.detect)
        if mod is F.embedded:
            kind = {"otb": "open_type", "bdat": "apple", "color": "color"}[fid]
            got = mod.detect(data, kind)
        elif mod is F.vector_raster:
            got = mod.detect(data, fid)
        else:
            got = mod.detect(data)
        assert got in ("possible", "certain"), (fid, got, list(params))

    # read
    if mod is F.embedded:
        kind = {"otb": "open_type", "bdat": "apple", "color": "color"}[fid]
        model, report = mod.read(data, kind, c.read or None)
    elif mod is F.vector_raster:
        model, report = mod.read(data, c.read or None)
    else:
        params = _params(mod.read)
        kwargs = {}
        if "options" in params:
            kwargs["options"] = c.read or None
        if "resources" in params:
            if written.output.is_directory:
                kwargs["directory"] = primary
            else:
                kwargs["name"] = primary
            kwargs["resources"] = resources or None
            model, report = mod.read(None if written.output.is_directory else data, **kwargs)
        else:
            model, report = mod.read(data, **kwargs)
    assert isinstance(report, dict) and "entries" in report
    registry_back = fontpixel.read(written, fid, c.read or None)
    if isinstance(model, fontpixel.Font):
        # Single-strike readers return a Font; the registry wraps it in a set.
        registry_back = registry_back.into_single()
        assert isinstance(registry_back.fonts, fontpixel.Font)
    else:
        assert isinstance(model, fontpixel.FontSet)
    # Registry codecs may add the asset name to entry locations.
    def entries(rep):
        return [(e["level"], e["impact"], e["code"], e["message"]) for e in rep["entries"]]

    assert entries(report) == entries(registry_back.report)
    direct = fontpixel.write(model, "yaff", report=report).into_single()
    assert direct == yaff_of(registry_back)

    # write: the registry wraps the same codec, so the bytes agree
    loaded = load_source(fid)
    if mod is F.embedded:
        kind = {"otb": "open_type", "bdat": "apple", "color": "color"}[fid]
        if fid == "color":
            out, _ = mod.write_color(loaded.fonts, c.write or None)
        else:
            out, _ = mod.write(loaded.fonts, kind, c.write or None)
        assert sha(out) == g["write"]["assets"][primary]
        return
    if mod is F.vector_raster:
        out, _ = mod.write(loaded.fonts, fid, c.write or None)
        assert sha(out) == g["write"]["assets"][primary]
        return
    params = _params(mod.write)
    fonts = loaded.fonts
    kwargs = {}
    if "primary_name" in params:
        kwargs["primary_name"] = primary
    result = mod.write(fonts, c.write or None, **kwargs)
    if isinstance(result, tuple):
        out, rep = result
        assert isinstance(rep, dict)
        assert sha(out) == g["write"]["assets"][primary]
    else:
        assert {k: sha(v) for k, v in result.assets.items()} == g["write"]["assets"]


@pytest.mark.parametrize("key", sorted(FIXTURES))
def test_fixture_reads_match_cli(key):
    """Real fixture files from tests/data and crates/fontpixel/tests."""
    rel, fmt, options, _ = FIXTURES[key]
    path = f"{REPO}/{rel}"
    name = rel.rsplit("/", 1)[-1]
    resources = None
    if name.endswith(".pil"):
        resources = {name[:-4] + ".pbm": open(path[:-4] + ".pbm", "rb").read()}
    loaded = fontpixel.read(open(path, "rb").read(), fmt, options or None, resources, name=name)
    assert sha(yaff_of(loaded)) == GOLDEN["fixtures"][key]["yaff"]
