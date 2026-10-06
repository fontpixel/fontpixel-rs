"""fontpixel.resources through Python: inputs, resolvers, read contexts, outputs."""
import gzip

import pytest

import fontpixel as fp


def test_resource_paths_and_limits():
    assert fp.resource_path("fonts/a.bdf") == "fonts/a.bdf"
    for bad in ["", "/abs", "a//b", "a/./b", "../x", "a\\b", "c:x", "a\nb", "dir/"]:
        with pytest.raises(fp.InvalidInputError):
            fp.resource_path(bad)
    limits = fp.default_resource_limits()
    assert set(limits) == {
        "max_input_bytes",
        "max_resource_bytes",
        "max_total_bytes",
        "max_resources",
        "max_decompressed_bytes",
        "max_glyphs",
        "max_pixels_per_glyph",
        "max_table_bytes",
    }
    transport = fp.default_transport_limits()
    assert transport["resources"] == limits
    assert transport["max_output_bytes"] == limits["max_input_bytes"]


def test_resource_set_resolver():
    rs = fp.ResourceSet({"a.txt": b"A", "dir/b.txt": bytearray(b"BB"), "dir/sub/c.txt": memoryview(b"C")})
    assert len(rs) == 3 and "dir/b.txt" in rs and "nope" not in rs and "../x" not in rs
    assert rs.paths() == ["a.txt", "dir/b.txt", "dir/sub/c.txt"]
    assert rs.get("dir/b.txt") == b"BB" and rs.get("x") is None
    assert rs.resolve("a.txt") == b"A"
    with pytest.raises(fp.MissingResourceError) as e:
        rs.resolve("missing.txt")
    assert e.value.kind == "MissingResource" and e.value.path == "missing.txt"
    with pytest.raises(fp.ResourceLimitError) as e:
        rs.resolve("dir/b.txt", 1)
    assert (e.value.resource, e.value.limit) == ("resource bytes", 1)
    assert rs.list() == ["a.txt"]
    assert rs.list("dir") == ["dir/b.txt"]
    with pytest.raises(fp.ResourceLimitError):
        rs.list(None, 0)
    rs.insert("z.txt", b"Z")
    assert rs.remove("a.txt") and not rs.remove("a.txt")
    with pytest.raises(fp.InvalidInputError):
        rs.insert("../escape", b"")
    assert "z.txt" in repr(rs)


def test_input_handles():
    i = fp.Input(b"abc", name="font.bdf")
    assert i.name == "font.bdf" and not i.is_directory and i.data == b"abc" and len(i) == 3
    j = i.with_name("other.pcf").with_resources({"x": b"1"})
    assert j.name == "other.pcf" and i.name == "font.bdf"
    d = fp.Input.directory("fontdir", fp.ResourceSet({"a": b"1"}))
    assert d.is_directory and d.data == b""
    with pytest.raises(TypeError):
        fp.Input(b"", resources=42)
    assert "Input(bytes=3" in repr(i)


def test_read_context_accounting():
    rs = fp.ResourceSet({"side.bin": b"x" * 10, "d/one": b"1", "d/two": b"22"})
    ctx = fp.ReadContext(fp.Input(b"primary", name="p.fnt", resources=rs))
    assert ctx.name == "p.fnt" and not ctx.is_directory
    assert ctx.bytes() == b"primary"
    assert ctx.limits == fp.default_resource_limits()
    assert ctx.retained_input_bytes() == 7
    assert ctx.resource("side.bin") == b"x" * 10
    assert ctx.retained_input_bytes() == 17
    with pytest.raises(fp.ResourceLimitError) as e:
        ctx.resource_limited("side.bin", 5)
    assert (e.value.resource, e.value.limit) == ("resource bytes", 5)
    assert ctx.list_resources_limited("d", 10, 10_000) == ["d/one", "d/two"]
    assert ctx.retained_input_bytes() > 17
    with pytest.raises(fp.MissingResourceError):
        ctx.resource("absent")
    # max_resources counts every resource operation, including failures.
    small = fp.ReadContext(fp.Input(b"p", resources=rs), {"max_resources": 2})
    small.resource("d/one")
    with pytest.raises(fp.MissingResourceError):
        small.resource("absent")
    with pytest.raises(fp.ResourceLimitError) as e:
        small.resource("d/one")
    assert (e.value.resource, e.value.limit) == ("resource count", 2)
    with pytest.raises(fp.ResourceLimitError) as e:
        fp.ReadContext(b"12345", {"max_input_bytes": 4})
    assert e.value.resource == "primary input bytes"
    with pytest.raises(fp.MissingResourceError):
        fp.ReadContext(b"x").resource("side.bin")
    assert "ReadContext(name=None" in repr(fp.ReadContext(b"x"))


def test_read_context_decodes_gzip_transport():
    raw = b"STARTFONT 2.1\n" * 20
    ctx = fp.ReadContext(gzip.compress(raw))
    assert ctx.bytes() == raw
    assert ctx.retained_input_bytes() > len(raw)
    with pytest.raises(fp.ResourceLimitError):
        fp.ReadContext(gzip.compress(raw), {"max_decompressed_bytes": 10})


def test_python_resolvers():
    calls = []

    class Resolver:
        def resolve(self, path, max_bytes):
            calls.append((path, max_bytes))
            if path == "boom":
                raise RuntimeError("boom")
            return None if path == "none" else b"data:" + path.encode()

        def list(self, directory, max_entries, max_bytes):
            return ["a", "b"] if directory is None else [f"{directory}/c"]

    ctx = fp.ReadContext(fp.Input(b"p", resources=Resolver()))
    assert ctx.resource("x/y") == b"data:x/y"
    assert calls[0][0] == "x/y"
    with pytest.raises(fp.MissingResourceError):
        ctx.resource("none")
    with pytest.raises(fp.FontpixelIoError) as e:
        ctx.resource("boom")
    assert "boom" in str(e.value) and e.value.kind == "Io"
    assert ctx.list_resources_limited(None, 5, 10_000) == ["a", "b"]
    assert ctx.list_resources_limited("q", 5, 10_000) == ["q/c"]

    def fn(path, max_bytes):
        raise KeyError(path)

    with pytest.raises(fp.MissingResourceError):
        fp.ReadContext(fp.Input(b"p", resources=fn)).resource("k")
    no_list = fp.ReadContext(fp.Input(b"p", resources=lambda p, m: b"1"))
    assert no_list.resource("k") == b"1"
    with pytest.raises(fp.InvalidInputError):
        no_list.list_resources_limited(None, 5, 100)
    too_big = fp.ReadContext(fp.Input(b"p", resources=lambda p, m: b"x" * 100), {"max_resource_bytes": 8})
    with pytest.raises(fp.ResourceLimitError):
        too_big.resource("k")


def test_output_bundles_and_written():
    a = fp.OutputAsset("font.fnt", b"F")
    assert a.path == "font.fnt" and a.data == b"F" and len(a) == 1
    assert a == fp.OutputAsset("font.fnt", bytearray(b"F"))
    bundle = fp.OutputBundle("font.fnt", [a, ("pages/0.png", b"PNG")])
    assert bundle.primary == "font.fnt" and not bundle.is_directory and len(bundle) == 2
    assert [x.path for x in bundle] == ["font.fnt", "pages/0.png"]
    assert [x.path for x in bundle.into_assets()] == [x.path for x in bundle.assets]
    assert bundle.retained_bytes() > 4
    with pytest.raises(fp.ResourceLimitError):
        bundle.retained_bytes(1)
    with pytest.raises(fp.MultipleAssetsError):
        bundle.into_single()
    with pytest.raises(fp.InvalidInputError):
        fp.OutputBundle("missing.fnt", [a])
    with pytest.raises(fp.InvalidInputError):
        fp.OutputBundle("font.fnt", [a, a])
    d = fp.OutputBundle("out", [("out/a", b"1")], directory=True)
    assert d.is_directory and d.primary == "out"
    single = fp.OutputBundle.single("x.bdf", b"BDF")
    assert single.into_single() == b"BDF"
    data = bundle.to_dict()
    assert data["primary"] == "font.fnt" and data["assets"][0] == {"path": "font.fnt", "bytes": [70]}
    assert fp.OutputBundle.from_dict(data) == bundle
    with pytest.raises(ValueError):
        fp.OutputBundle.from_dict({**data, "primary": "../x"})
    w = fp.Written(single, {"entries": []})
    assert w.output == single and w.report == {"entries": []}
    assert w.to_dict()["output"]["primary"] == "x.bdf"
    assert "Written(output=OutputBundle(" in repr(w)
    assert "OutputAsset(path=" in repr(a)
