"""Registry, module functions, errors, strict mode, limits and resources."""
import gzip

import pytest

import fontpixel
from formats_support import SOURCES, assets_of, case, load_golden, sha, single

UNI3 = open(SOURCES["uni3"], "rb").read()
UNI5 = open(SOURCES["uni5"], "rb").read()
LEG3 = open(SOURCES["leg3"], "rb").read()
GOLDEN = load_golden()


def yaff(loaded):
    return single(fontpixel.write(loaded.fonts, "yaff", report=loaded.report))


# --- catalogue queries ------------------------------------------------------


def test_registry_formats_status_and_names():
    reg = fontpixel.Registry()
    assert reg.formats() == fontpixel.list_formats() == GOLDEN["listing"]
    assert reg.status("bdf") == GOLDEN["listing"][0]
    with pytest.raises(fontpixel.UnknownFormatError) as e:
        reg.status("nope")
    assert e.value.kind == "UnknownFormat" and e.value.format == "nope"
    # Longest suffix wins; collisions stay visible.
    assert [f["id"] for f in reg.extension_candidates("x.bdf.gz")] == ["bdf"]
    fnt = [f["id"] for f in reg.extension_candidates("X.FNT")]
    assert {"raw", "fnt", "fontx2"} <= set(fnt)
    assert reg.format_from_name("a.pcf")["id"] == "pcf"
    with pytest.raises(fontpixel.AmbiguousFormatError) as e:
        reg.format_from_name("a.fnt")
    assert set(e.value.candidates) == set(fnt)
    with pytest.raises(fontpixel.UnrecognizedFormatError):
        reg.format_from_name("a.unknown-ext")
    assert "99 formats" in repr(reg)


def test_planned_registry_has_no_codecs():
    reg = fontpixel.Registry(builtin=False)
    statuses = reg.formats()
    assert len(statuses) == 99
    assert not any(s["read"] or s["write"] or s["detect"] for s in statuses)
    with pytest.raises(fontpixel.NotImplementedFormatError) as e:
        reg.ensure_writable("bdf")
    assert e.value.kind == "NotImplemented"
    # error::Operation travels as the `operation` attribute.
    assert e.value.operation == "Write" and e.value.format == "bdf"
    with pytest.raises(fontpixel.NotImplementedFormatError) as e:
        reg.read(UNI3, "bdf")
    assert e.value.operation == "Read"
    with pytest.raises(fontpixel.UnrecognizedFormatError):
        reg.detect(UNI3)
    fontpixel.Registry().ensure_writable("bdf")
    assert isinstance(fontpixel.registry(), fontpixel.Registry)


# --- detection --------------------------------------------------------------


def test_detect_content_then_name_hint():
    d = fontpixel.detect(UNI3)
    assert d["format"]["id"] == "bdf" and d["confidence"] == "certain"
    assert d == fontpixel.Registry().detect(UNI3)
    # No detector matches arbitrary bytes; the name hint decides.
    junk = b"\x00\x01\x02 not a font"
    with pytest.raises(fontpixel.UnrecognizedFormatError):
        fontpixel.detect(junk)
    hinted = fontpixel.detect(junk, "font.pcf")
    assert hinted["format"]["id"] == "pcf" and hinted["confidence"] == "extension"
    with pytest.raises(fontpixel.AmbiguousFormatError):
        fontpixel.detect(junk, "font.fnt")
    # Content beats the name.
    assert fontpixel.detect(UNI3, "font.pcf")["format"]["id"] == "bdf"
    # gzip transport is decoded before detection.
    assert fontpixel.detect(gzip.compress(UNI3))["format"]["id"] == "bdf"
    with pytest.raises(fontpixel.InvalidInputError):
        fontpixel.Registry().detect(directory="dir", resources={"dir/a": b""})
    for buffer in (bytearray(UNI3), memoryview(UNI3)):
        assert fontpixel.detect(buffer)["format"]["id"] == "bdf"
    with pytest.raises(TypeError):
        fontpixel.detect("not bytes")


# --- reading ----------------------------------------------------------------


def test_read_matches_cli_and_reports():
    loaded = fontpixel.read(UNI3)
    assert loaded.format == "bdf" and isinstance(loaded.fonts, fontpixel.FontSet)
    assert loaded.report == {"entries": []}
    spleen = fontpixel.read(UNI5)
    assert sha(yaff(spleen)) == GOLDEN["fixtures"]["bdf-spleen"]["yaff"]
    one = spleen.into_single()
    assert isinstance(one.fonts, fontpixel.Font) and one.format == "bdf"
    assert len(one.fonts) == 472
    with pytest.raises(fontpixel.InvalidInputError):
        one.into_single()
    assert "Loaded(format=\"bdf\"" in repr(loaded)


def test_read_with_options_and_explicit_format():
    w = fontpixel.write(fontpixel.read(UNI3).fonts, "hex", {"geometry": {"height": 6, "descent": 1}})
    data = single(w)
    # Wrong geometry is an error, the matching one reads.
    with pytest.raises(fontpixel.FontpixelError):
        fontpixel.read(data, "hex")
    ok = fontpixel.read(data, "hex", {"height": 6, "descent": 1})
    assert ok.format == "hex"
    # Options without a format apply to the detected format.
    assert fontpixel.read(data, options={"height": 6, "descent": 1}).format == "hex"
    with pytest.raises(ValueError, match="unknown field"):
        fontpixel.read(data, "hex", {"height": 6, "bogus": 1})
    with pytest.raises(fontpixel.UnknownFormatError):
        fontpixel.read(data, "no-such-format")


def test_read_errors_and_resource_limits():
    with pytest.raises(fontpixel.UnrecognizedFormatError):
        fontpixel.read(b"\x00" * 7)
    with pytest.raises(fontpixel.ResourceLimitError) as e:
        fontpixel.read(UNI3, limits={"max_input_bytes": 100})
    assert e.value.resource == "primary input bytes" and e.value.limit == 100
    with pytest.raises(fontpixel.ResourceLimitError) as e:
        fontpixel.read(UNI5, limits={"max_glyphs": 10})
    assert e.value.limit == 10
    with pytest.raises(fontpixel.ResourceLimitError):
        fontpixel.read(gzip.compress(UNI3), limits={"max_decompressed_bytes": 1000})
    with pytest.raises(ValueError):
        fontpixel.read(UNI3, limits={"max_glyphs": -1})
    with pytest.raises(ValueError, match="unknown field"):
        fontpixel.read(UNI3, limits={"max_gylphs": 1})
    with pytest.raises(fontpixel.BdfParseError) as e:
        fontpixel.read(b"STARTFONT 2.1\nSIZE x\n", "bdf")
    assert e.value.kind == "BdfParse" and e.value.line == 2


# --- writing ----------------------------------------------------------------


def test_write_strict_mode_and_reports():
    loaded = fontpixel.read(UNI3)
    w = fontpixel.write(loaded.fonts, "psf")
    report = w.report
    assert any(e["impact"] == "loss" for e in report["entries"])
    with pytest.raises(fontpixel.StrictLossError) as e:
        fontpixel.write(loaded.fonts, "psf", strict=True)
    assert e.value.kind == "StrictLoss"
    assert e.value.report["entries"] == report["entries"]
    # Loss-free output passes strict mode.
    assert single(fontpixel.write(loaded.fonts, "bdf", strict=True))
    # Read diagnostics carried into the write make strict mode fail early.
    lossy_read = {"entries": [{"level": "warning", "impact": "loss", "code": "test.loss",
                               "location": {"font_index": None, "glyph_index": None,
                                            "codepoint": None, "asset": None},
                               "message": "earlier loss"}]}
    reg = fontpixel.Registry()
    with pytest.raises(fontpixel.StrictLossError) as e:
        reg.write_with_report(loaded.fonts, "bdf", lossy_read, strict=True)
    assert e.value.report == lossy_read
    w = reg.write_with_report(loaded.fonts, "bdf", lossy_read)
    assert w.report["entries"][0]["code"] == "test.loss"
    error_read = {"entries": [dict(lossy_read["entries"][0], level="error")]}
    with pytest.raises(fontpixel.ReportedErrorsError) as e:
        reg.write(loaded.fonts, "bdf", report=error_read)
    assert e.value.report == error_read
    with pytest.raises(fontpixel.ReportedErrorsError) as e:
        fontpixel.write(loaded.fonts, "fzx")
    assert e.value.report["entries"][0]["code"] == "fzx.write.unrepresentable_mapping"


def test_write_variants_agree_and_owned_moves_the_model():
    reg = fontpixel.Registry()
    loaded = fontpixel.read(UNI3)
    plain = reg.write(loaded.fonts, "pcf")
    reserved = reg.write_reserved_with_report(loaded.fonts, "pcf", loaded.report,
                                              transport_limits={})
    assert assets_of(plain) == assets_of(reserved)
    assert plain.report == reserved.report
    moved = loaded.fonts.copy()
    owned = reg.write_owned(moved, "pcf", transport_limits={})
    assert assets_of(owned) == assets_of(plain)
    assert len(moved) == 0 and len(loaded.fonts) == 1
    # An owned outline write takes the dedicated owned route.
    out = reg.write_owned(loaded.fonts.copy(), "ttf")
    assert assets_of(out) == assets_of(reg.write(loaded.fonts, "ttf"))
    # A Font argument is accepted as a one-strike set.
    font = loaded.fonts[0]
    assert assets_of(reg.write(font, "pcf")) == assets_of(plain)


def test_transport_limits_and_gzip_envelope():
    loaded = fontpixel.read(UNI3)
    with pytest.raises(fontpixel.ResourceLimitError) as e:
        fontpixel.write(loaded.fonts, "bdf", transport_limits={"max_output_bytes": 64})
    # The BDF writer's own cap is intersected with the common limit.
    assert e.value.resource == "BDF output bytes" and e.value.limit == 64
    # A writer without a native cap hits the registry's aggregate check.
    with pytest.raises(fontpixel.ResourceLimitError) as e:
        fontpixel.write(loaded.fonts, "c", transport_limits={"max_output_bytes": 64})
    assert e.value.limit == 64
    with pytest.raises(ValueError, match="unknown field"):
        fontpixel.write(loaded.fonts, "bdf", transport_limits={"max_output": 1})
    gz = fontpixel.write(loaded.fonts, "bdf", primary_name="x.bdf.gz")
    data = assets_of(gz)["x.bdf.gz"]
    assert data[:2] == b"\x1f\x8b"
    assert gzip.decompress(data) == single(fontpixel.write(loaded.fonts, "bdf"))
    back = fontpixel.read(gz)
    assert back.format == "bdf" and yaff(back) == yaff(loaded)
    with pytest.raises(fontpixel.ResourceLimitError):
        fontpixel.write(loaded.fonts, "bdf", primary_name="x.bdf.gz",
                        transport_limits={"max_output_bytes": 32})
    with pytest.raises(fontpixel.InvalidInputError):
        fontpixel.write(loaded.fonts, "bdf", primary_name="../escape.bdf")


# --- sidecars, multi-asset and directory output -----------------------------


def test_sidecar_formats_with_every_resource_kind():
    loaded = fontpixel.read(UNI5)
    w = fontpixel.write(loaded.fonts, "hbf", primary_name="font.hbf")
    files = assets_of(w)
    assert set(files) == {"font.hbf", "font.bin"}
    with pytest.raises(fontpixel.MultipleAssetsError) as e:
        w.output.into_single()
    assert e.value.count == 2
    expected = yaff(fontpixel.read(w, "hbf"))
    header = files["font.hbf"]
    # dict, ResourceSet, callable and resolver object
    assert yaff(fontpixel.read(header, "hbf", resources={"font.bin": files["font.bin"]})) == expected
    rs = fontpixel.ResourceSet({"font.bin": files["font.bin"]})
    assert yaff(fontpixel.read(header, resources=rs, name="font.hbf")) == expected
    calls = []

    def resolve(path, max_bytes):
        calls.append((path, max_bytes))
        return files[path]

    assert yaff(fontpixel.read(header, "hbf", resources=resolve)) == expected
    assert calls and calls[0][0] == "font.bin"

    class Resolver:
        def resolve(self, path, max_bytes):
            return files[path]

    assert yaff(fontpixel.Registry().read(header, "hbf", resources=Resolver())) == expected
    assert yaff(fontpixel.read(fontpixel.Input(header, "font.hbf", {"font.bin": files["font.bin"]}))) == expected
    # Missing sidecar: dict without it, and a resolver raising KeyError.
    with pytest.raises(fontpixel.MissingResourceError):
        fontpixel.read(header, "hbf")
    with pytest.raises(fontpixel.MissingResourceError):
        fontpixel.read(header, "hbf", resources=lambda p, m: {}[p])
    # Per-resource budget and resource count.
    with pytest.raises(fontpixel.ResourceLimitError):
        fontpixel.read(header, "hbf", resources=files, limits={"max_resource_bytes": 16})
    with pytest.raises(fontpixel.ResourceLimitError) as e:
        fontpixel.read(header, "hbf", resources=files, limits={"max_resources": 0})
    assert e.value.resource == "resource count"
    with pytest.raises(fontpixel.InvalidInputError):
        fontpixel.read(w, "hbf", name="x")


def test_directory_output_round_trip():
    loaded = fontpixel.read(LEG3)
    w = fontpixel.write(loaded.fonts, "consoleet", primary_name="font.txt")
    assert w.output.is_directory and w.output.primary == "font.txt"
    files = assets_of(w)
    assert all(p.startswith("font.txt/") for p in files)
    back = fontpixel.read(w, "consoleet")
    relative = {p[len("font.txt/"):]: b for p, b in files.items()}
    same = fontpixel.read(format="consoleet", directory="font.txt", resources=relative)
    assert yaff(same) == yaff(back)
    assert yaff(fontpixel.read(fontpixel.Input.directory("font.txt", relative), "consoleet")) == yaff(back)
    with pytest.raises(fontpixel.InvalidInputError):
        fontpixel.read(directory="font.txt", resources=relative)  # needs a format
    with pytest.raises(fontpixel.InvalidInputError):
        fontpixel.read(b"x", "consoleet", directory="font.txt")
    g = GOLDEN["formats"]["consoleet"]
    assert {k: sha(v) for k, v in files.items()} == g["write"]["assets"]


# --- Python codecs ----------------------------------------------------------


def test_python_codec_registration():
    reg = fontpixel.Registry()
    seen = {}

    def detector(ctx):
        return "certain" if ctx.bytes().startswith(b"PYFONT\n") else "no_match"

    def reader(ctx, options):
        seen["options"] = options
        seen["name"] = ctx.name
        side = ctx.resource("side.txt")
        seen["side"] = side
        bdf = ctx.bytes()[len(b"PYFONT\n"):]
        inner = fontpixel.read(bdf)
        return inner.fonts, {"entries": []}

    class Writer:
        def write(self, fonts, options):
            seen["write_options"] = options
            body = b"PYFONT\n" + single(fontpixel.write(fonts, "bdf"))
            return fontpixel.OutputBundle(options["primary_name"], [(options["primary_name"], body), ("side.txt", b"hi")])

        def transport_limits(self):
            return {"max_output_bytes": 1 << 20}

    reg.register({"id": "py-test", "name": "Python test", "extensions": ["pyf"]},
                 detector=detector, reader=reader, writer=Writer())
    st = reg.status("py-test")
    assert st == {"format": {"id": "py-test", "name": "Python test", "extensions": ["pyf"],
                             "feature_enabled": True}, "detect": True, "read": True, "write": True}
    loaded = fontpixel.read(UNI3)
    w = reg.write(loaded.fonts, "py-test")
    assert seen["write_options"] == {"strict": False, "primary_name": "font.pyf", "transport_limits": None}
    assert set(assets_of(w)) == {"font.pyf", "side.txt"}
    assert reg.detect(assets_of(w)["font.pyf"])["format"]["id"] == "py-test"
    back = reg.read(w)
    assert back.format == "py-test" and seen["side"] == b"hi" and seen["name"] == "font.pyf"
    assert seen["options"]["format"] is None
    assert yaff(back) == yaff(loaded)
    # The Python exception raised by a hook propagates unchanged.

    def broken(ctx, options):
        raise KeyError("custom failure")

    reg2 = fontpixel.Registry()
    reg2.register({"id": "py-broken", "name": "Broken"}, reader=broken)
    with pytest.raises(KeyError, match="custom failure"):
        reg2.read(b"data", "py-broken")
    # Planned registration, duplicate and invalid IDs.
    reg2.register({"id": "py-planned", "name": "Planned"})
    assert reg2.status("py-planned")["read"] is False
    with pytest.raises(fontpixel.InvalidInputError):
        reg2.register("bdf", reader=reader)  # live entry
    with pytest.raises(fontpixel.InvalidInputError):
        reg2.register({"id": "Bad_ID", "name": "x"})
    planned = fontpixel.Registry(builtin=False)
    planned.register("bdf", writer=lambda fonts, o: fontpixel.OutputBundle.single(o["primary_name"], b"x"))
    assert assets_of(planned.write(loaded.fonts, "bdf")) == {"font.bdf": b"x"}
    # gzip output of a Python writer uses the registry envelope.
    gz = planned.write(loaded.fonts, "bdf", primary_name="a.bdf.gz")
    assert gzip.decompress(assets_of(gz)["a.bdf.gz"]) == b"x"


def test_case_table_covers_option_examples():
    assert case("hzk").write == {"preset": "asc16"}
