"""fontpixel.sfnt: container, builder, collections, CFF and subsetting."""
import pytest

import fontpixel
import fontpixel.sfnt.cff as cff
import fontpixel.sfnt.collection as collection
import fontpixel.sfnt.subset as subset
import fontpixel.woff.woff1 as woff1
import fontpixel.woff.woff2 as woff2
from fontpixel import sfnt
from sfnt_p3_support import CIRCLE, E, GOLDEN, h, norm


def test_sfnt_container():
    s = sfnt.Sfnt(GOLDEN)
    assert sfnt.Sfnt.parse(GOLDEN).tables() == s.tables()
    assert s.flavor == "true_type"
    assert s.data == GOLDEN
    assert s.limits == sfnt.defaults()["Limits"]
    assert [[t["tag"], t["checksum"], t["offset"], len(t["data"])] for t in s.tables()] == E["sfnt.tables"]
    for t in s.tables():
        assert s.table(t["tag"]) == t["data"]
        assert s.table(t["tag"].encode("latin-1")) == t["data"]
        assert t["data"] == GOLDEN[t["offset"]:t["offset"] + len(t["data"])]
    assert s.table("ZZZZ") is None
    s.validate_checksums()
    assert repr(s) == f"<fontpixel.sfnt.Sfnt true_type tables=11 bytes={len(GOLDEN)}>"
    # bytearray and memoryview inputs behave like bytes
    assert sfnt.Sfnt(bytearray(GOLDEN)).tables() == s.tables()
    assert sfnt.Sfnt(memoryview(GOLDEN)).tables() == s.tables()


def test_sfnt_builder_and_checksum():
    # Rust golden_container_reassembly_is_byte_exact
    s = sfnt.Sfnt(GOLDEN)
    b = sfnt.SfntBuilder(s.flavor)
    for t in reversed(s.tables()):
        b.insert(t["tag"], t["data"])
    assert b.build() == GOLDEN
    assert sfnt.checksum(GOLDEN) == 0xB1B0AFBA == E["sfnt.checksum"]
    # Rust checksum_padding_uses_big_endian_words
    assert sfnt.checksum(bytes([1, 2, 3, 4, 5])) == 0x06020304
    assert sfnt.checksum(b"\xff" * 8) == 0xFFFFFFFE
    assert sfnt.flavor_version("cff") == E["sfnt.flavor_version.cff"] == 0x4F54544F
    assert sfnt.flavor_version("true_type") == 0x10000
    assert sfnt.flavor_version("apple_true_type") == 0x74727565
    # unknown tables and every flavor rebuild with valid checksums
    for flavor in ["true_type", "apple_true_type", "cff"]:
        b = sfnt.SfntBuilder(flavor)
        for t in s.tables():
            b.insert(t["tag"], t["data"])
        b.insert("ZZZZ", b"\x01\x02\x03")
        out = sfnt.Sfnt(b.build())
        out.validate_checksums()
        assert out.flavor == flavor
        assert out.table("ZZZZ") == b"\x01\x02\x03"


def test_sfnt_errors():
    for end in range(12):
        with pytest.raises(fontpixel.InvalidInputError) as e:
            sfnt.Sfnt(GOLDEN[:end])
        assert e.value.kind == "InvalidInput"
    with pytest.raises(fontpixel.ResourceLimitError) as e:
        sfnt.Sfnt(GOLDEN, {"max_tables": 1})
    assert (e.value.kind, e.value.resource, e.value.limit) == ("ResourceLimit", "sfnt tables", 1)
    with pytest.raises(fontpixel.ResourceLimitError) as e:
        sfnt.Sfnt(GOLDEN, {"resources": {"max_table_bytes": 1}})
    assert e.value.limit == 1
    bad = bytearray(GOLDEN)
    bad[16] ^= 1
    with pytest.raises(fontpixel.InvalidInputError):
        sfnt.Sfnt(bytes(bad)).validate_checksums()
    ttc = bytearray(GOLDEN)
    ttc[:4] = b"ttcf"
    with pytest.raises(fontpixel.InvalidInputError):
        sfnt.Sfnt(bytes(ttc))
    with pytest.raises(ValueError):
        sfnt.Sfnt(GOLDEN, {"no_such_limit": 1})
    with pytest.raises(fontpixel.InvalidInputError):
        sfnt.Sfnt(GOLDEN).table("toolong")
    with pytest.raises(fontpixel.InvalidInputError):
        sfnt.Sfnt(GOLDEN).table("Āabc")
    # Rust writer_rejects_duplicates_missing_head_and_budgets
    b = sfnt.SfntBuilder("cff")
    with pytest.raises(fontpixel.InvalidInputError):
        b.build()
    b.insert("head", bytes(12))
    with pytest.raises(fontpixel.InvalidInputError):
        b.insert("head", b"")
    with pytest.raises(fontpixel.InvalidInputError):
        b.insert(b"\0\0\0\0", b"")
    with pytest.raises(fontpixel.ResourceLimitError):
        b.build({"resources": {"max_total_bytes": 20}})
    with pytest.raises(ValueError):
        sfnt.SfntBuilder("opentype")


def test_sfnt_defaults():
    d = sfnt.defaults()
    assert d["Limits"]["max_tables"] == 4095
    assert d["Limits"]["max_cmap_mappings"] == 0x110000
    assert d["Limits"]["max_name_records"] == 8192
    assert d["gsub.LayoutLimits"]["max_records"] == 1_000_000
    assert d["cff.DotOptions"] == {"cell_size": 100.0, "diameter": 90.0}
    assert d["subset.Options"]["codepoints"] == []
    assert set(d) == {
        "Limits", "collection.Limits", "glyf.Limits", "cff.Limits", "cff.DotOptions",
        "bitmap.Limits", "gsub.LayoutLimits", "subset.Options",
    }
    # every default dict is accepted back
    sfnt.Sfnt(GOLDEN, d["Limits"])
    collection.Collection(collection.encode([GOLDEN], 0x10000, d["collection.Limits"])[0])


def test_collection():
    data, report = collection.encode([GOLDEN, CIRCLE], 0x10000)
    exp_bytes, exp_report = E["collection.encode"]
    assert data == bytes.fromhex(exp_bytes)
    assert norm(report) == exp_report
    c = collection.Collection(data)
    assert collection.Collection.parse(data).version == c.version == 0x10000
    assert c.data == data
    assert [[f["flavor"], f["offset"], len(f["tables"])] for f in c.faces] == E["collection.faces"]
    assert c.signature is None
    c.validate_checksums()
    one, rep = c.extract(1)
    assert [one.hex(), norm(rep)] == E["collection.extract1"]
    sfnt.Sfnt(one).validate_checksums()
    with pytest.raises(fontpixel.InvalidInputError):
        c.extract(2)
    with pytest.raises(fontpixel.ResourceLimitError) as e:
        collection.Collection(data, {"max_faces": 1})
    assert e.value.resource and e.value.limit == 1
    with pytest.raises(fontpixel.InvalidInputError):
        collection.Collection(GOLDEN)
    with pytest.raises(fontpixel.InvalidInputError):
        collection.encode([b"not a font"], 0x10000)


def _otf():
    return sfnt.Sfnt(h("variants.otf"))


def test_cff_reader():
    info = E["cff.info"]
    c = cff.Cff.from_sfnt(_otf())
    assert len(c) == c.len() == info["len"]
    assert c.is_empty() is False
    assert c.is_cid() == info["is_cid"]
    assert c.glyph_name(1).hex() == info["glyph_name1"]
    assert c.glyph_name(60000) is None
    assert c.identifier(1) == info["identifier1"]
    assert c.global_subr_count() == info["global_subr_count"]
    assert c.data.hex() == info["data"]
    assert norm(c.glyph(1)) == E["cff.glyph1"]
    assert norm(c.read_all()) == E["cff.read_all"]
    assert c.subset([0, 1]) == h("cff.subset01")
    raw = cff.Cff.parse(c.data, E["cff.upm"])
    assert norm(raw.read_all()) == E["cff.read_all"]
    with pytest.raises(fontpixel.InvalidInputError):
        cff.Cff.from_sfnt(sfnt.Sfnt(GOLDEN))
    with pytest.raises(fontpixel.InvalidInputError):
        cff.Cff.parse(b"\x01\x00\x04", 1000)
    with pytest.raises(fontpixel.ResourceLimitError):
        c.read_all({"max_total_commands": 1})
    with pytest.raises(fontpixel.InvalidInputError):
        c.glyph(60000)


def test_cff_writer():
    e = cff.encode(E["cff.font"])
    exp = E["cff.encode"]
    assert e["data"].hex() == exp["data"]
    assert e["maxp"].hex() == exp["maxp"]
    assert e["bounds"] == tuple(exp["bounds"])
    parsed = cff.Cff.parse(e["data"], 1000)
    assert len(parsed) == 3 and parsed.glyph_name(1) == b"A"
    bad = dict(E["cff.font"])
    bad["glyphs"] = bad["glyphs"][1:]
    with pytest.raises(fontpixel.InvalidInputError):
        cff.encode(bad)
    with pytest.raises(fontpixel.ResourceLimitError):
        cff.encode(E["cff.font"], {"max_total_commands": 2})
    bitmap = fontpixel.outline.rasterize(E["glyf.contours.2"], 64.0, (0, 0), 5, 7)
    assert norm(cff.glyph_from_bitmap("A", 600.0, bitmap)) == E["cff.glyph_from_bitmap"]
    with pytest.raises(fontpixel.InvalidInputError):
        cff.glyph_from_bitmap("A", 600.0, bitmap, {"units_per_pixel": 0})


def test_subset():
    exp_bytes, exp_mapping, exp_report = E["subset.text_A"]
    opts = subset.options_text("BA")
    assert norm(opts) == E["subset.options_text"]
    r = subset.subset(GOLDEN, subset.options_text("A"))
    assert r["bytes"] == bytes.fromhex(exp_bytes) == h("cli.subset-A.ttf")
    assert norm(r["glyph_mapping"]) == exp_mapping
    assert norm(r["report"]) == exp_report
    # CLI `subset --unicodes U+41-42 -o x.woff2`: subset, then WOFF2 defaults
    ab = subset.subset(GOLDEN, {"codepoints": [0x41, 0x42]})
    assert woff2.encode(ab["bytes"])["bytes"] == h("cli.subset-AB.woff2")
    # CLI `subset --unicodes U+42 -o x.woff`: subset, then WOFF1 defaults
    b = subset.subset(GOLDEN, {"codepoints": {0x42}})
    assert woff1.encode(b["bytes"])["bytes"] == h("cli.subset-B.woff")
    with pytest.raises(fontpixel.InvalidInputError):
        subset.subset(b"junk", opts)
    with pytest.raises(ValueError):
        subset.subset(GOLDEN, {"drop_tables": ["toolong"]})
    with pytest.raises(fontpixel.ResourceLimitError) as e:
        subset.subset(GOLDEN, {"codepoints": [65], "limits": {"sfnt": {"resources": {"max_total_bytes": 64}}}})
    assert e.value.limit == 64
    with pytest.raises(ValueError):
        subset.subset(GOLDEN, {"text": "A"})
