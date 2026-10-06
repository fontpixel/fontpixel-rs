"""fontpixel.model through Python: FontSet, Font, Glyph, Codepoint, Properties."""
import copy

import pytest

import fontpixel as fp


def mono(text):
    return fp.Bitmap.from_text(text)


def glyph(text, *cps, name=None):
    return fp.Glyph(mono(text), codepoints=list(cps), name=name)


# Codepoint ------------------------------------------------------------------


def test_codepoint_class():
    a = fp.Codepoint.unicode(0x41)
    assert a == fp.Codepoint(0x41) == fp.Codepoint(0x41, "unicode")
    assert a.value == 0x41 and int(a) == 0x41
    assert a.kind == "unicode" and a.is_unicode
    assert a.scalar() == "A"
    legacy = fp.Codepoint.legacy(0x80)
    assert legacy.kind == "legacy" and not legacy.is_unicode
    assert legacy.scalar() is None
    assert fp.Codepoint.unicode(0xD800).scalar() is None
    assert a != fp.Codepoint.legacy(0x41)
    # Unicode orders before legacy (derived Ord on the Rust enum).
    assert sorted([legacy, a, fp.Codepoint.unicode(0x20)]) == [fp.Codepoint(0x20), a, legacy]
    assert len({a, fp.Codepoint.unicode(0x41)}) == 1
    assert a.to_dict() == {"Unicode": 65}
    assert legacy.to_dict() == {"Legacy": 128}
    assert fp.Codepoint.from_dict({"Legacy": 128}) == legacy
    assert repr(a) == "Codepoint.unicode(0x0041)"
    with pytest.raises(fp.InvalidInputError):
        fp.Codepoint(1, "other")


# Font -----------------------------------------------------------------------


def test_font_glyph_lookup():
    font = fp.Font([glyph("#", "A", name="A"), glyph("##", 0x42)])
    assert len(font) == 2 and font.slot_count == 2 and not font.is_empty()
    assert font.glyph_ids() == [0, 1]
    assert font.glyph_id("A") == 0
    assert font.glyph_id(fp.Codepoint.unicode(66)) == 1
    assert font.glyph_id({"Unicode": 67}) is None
    g = font.glyph_for_codepoint(66)
    assert g.is_view and g.id == 1 and g.bitmap.width == 2
    assert font.glyph_for_codepoint("Z") is None
    assert font.glyph(0).name == "A"
    assert font.glyph(7) is None
    assert font[1].id == 1
    with pytest.raises(KeyError):
        font[9]
    assert [g.id for g in font] == [0, 1]
    pairs = font.glyphs()
    assert [(i, g.name) for i, g in pairs] == [(0, "A"), (1, None)]
    assert font.codepoints() == [(fp.Codepoint(65), 0), (fp.Codepoint(66), 1)]
    with pytest.raises(TypeError):
        font.glyph_id(True)
    with pytest.raises(TypeError):
        font.glyph_id("AB")


def test_font_duplicate_codes_and_ranges():
    font = fp.Font()
    a = font.add_glyph(glyph("#", 65))
    b = font.add_glyph(glyph("##", 65, fp.Codepoint.legacy(3)))
    c = font.add_glyph(glyph("###", 90))
    assert (a, b, c) == (0, 1, 2)
    # Later glyph wins; both IDs stay listed in order.
    assert font.glyph_id(65) == 1
    assert font.glyph_ids_for_codepoint(65) == [0, 1]
    assert font.codepoint_range(65, 80) == [(fp.Codepoint(65), 1)]
    assert font.codepoint_range(66, 65) == []
    # Unicode precedes legacy in codepoint order.
    everything = font.codepoint_range(fp.Codepoint(0), fp.Codepoint.legacy(0xFFFFFFFF))
    assert [cp for cp, _ in everything] == [
        fp.Codepoint(65),
        fp.Codepoint(90),
        fp.Codepoint.legacy(3),
    ]


def test_font_sequences():
    g = glyph("#", name="fi")
    g.unicode_sequences = [[0x66, 0x69]]
    font = fp.Font([g])
    assert font.glyph_for_sequence("fi").name == "fi"
    assert font.glyph_for_sequence([0x66, 0x69]).id == 0
    assert font.glyph_for_sequence("ff") is None
    g.unicode_sequences = [[]]
    with pytest.raises(fp.InvalidInputError):
        g.validate()
    with pytest.raises(fp.InvalidInputError):
        font.add_glyph(g)


def test_font_add_replace_remove_edit():
    font = fp.Font([glyph("#", 65), glyph("##", 66)])
    old = font.replace_glyph(0, glyph("###", 67))
    assert not old.is_view and old.codepoints == [fp.Codepoint(65)]
    assert font.glyph_id(65) is None and font.glyph_id(67) == 0
    removed = font.remove_glyph(1)
    assert removed.bitmap.width == 2
    assert font.remove_glyph(1) is None
    assert len(font) == 1 and font.slot_count == 2
    # IDs are stable and never reused.
    assert font.add_glyph(glyph("#", 68)) == 2
    with pytest.raises(fp.InvalidInputError) as e:
        font.replace_glyph(1, glyph("#"))
    assert e.value.kind == "InvalidInput"

    def edit(g):
        g.name = "edited"
        g.shift(2, -1)

    font.edit_glyph(0, edit)
    assert font.glyph(0).name == "edited"
    assert font.glyph(0).metrics["bounds"] == {"width": 3, "height": 1, "x": 2, "y": -1}

    def failing(g):
        g.name = "lost"
        raise RuntimeError("stop")

    with pytest.raises(RuntimeError):
        font.edit_glyph(0, failing)
    assert font.glyph(0).name == "edited"
    with pytest.raises(fp.InvalidInputError):
        font.edit_glyph(1, edit)
    # A glyph whose bounds disagree with its bitmap is rejected.
    bad = glyph("#")
    with pytest.raises(fp.InvalidInputError):
        m = bad.metrics
        m["bounds"]["width"] = 5
        bad.metrics = m


def test_font_views_edit_in_place():
    s = fp.FontSet([fp.Font([glyph("#", 65)])])
    g = s[0].glyph(0)
    g.bitmap = mono("##\n##")
    assert s.fonts[0].glyph(0).bitmap.to_rows() == [[1, 1], [1, 1]]
    assert s[0].glyph(0).metrics["bounds"] == {"width": 2, "height": 2, "x": 0, "y": 0}
    g.codepoints = ["B"]
    assert s[0].glyph_id("B") == 0 and s[0].glyph_id("A") is None
    # copy() detaches.
    c = g.copy()
    c.name = "copy"
    assert g.name is None
    removed = s[0].remove_glyph(0)
    assert removed.name is None
    with pytest.raises(fp.InvalidInputError):
        g.name


def test_glyph_bitmap_set_and_cell():
    g = fp.Glyph()
    assert g.bitmap.width == 0 and g.id is None
    g.set_bitmap(mono("...\n.#.\n..."), 1, 2, trim=True)
    assert g.metrics["bounds"] == {"width": 1, "height": 1, "x": 2, "y": 3}
    cell = g.bitmap_in_cell({"width": 4, "height": 4, "x": 0, "y": 0})
    assert str(cell) == "..#.\n....\n....\n...."
    g.set_bitmap(mono("##"), -1, 0)
    assert g.metrics["bounds"]["x"] == -1
    with pytest.raises(fp.InvalidInputError):
        g.shift(-(2**63), 0)
    g.validate()


def test_glyph_metric_dicts():
    g = glyph("##", 65)
    m = g.metrics
    assert m == {"bounds": {"width": 2, "height": 1, "x": 0, "y": 0}}
    m.update(
        advance={"x": 3, "y": 0},
        scalable_advance={"x": 500, "y": 0},
        abc={"a": 0, "b": 2, "c": 1},
        left_bearing=0,
        image_offset={"x": 1, "y": -1},
    )
    g.metrics = m
    assert g.metrics == m
    with pytest.raises(ValueError):
        g.metrics = {**m, "abc": {"a": 0, "b": 2}}
    with pytest.raises(ValueError):
        g.metrics = {**m, "advance": {"x": 1, "y": 0, "z": 0}}
    font = fp.Font([g])
    fm = font.metrics
    fm.update(
        point_size={"numerator": 21, "denominator": 2},
        resolution={"x": 75, "y": 75},
        spacing="Monospaced",
        ascent=7,
        descent=1,
    )
    font.metrics = fm
    assert font.metrics["point_size"] == {"numerator": 21, "denominator": 2}
    assert font.metrics["spacing"] == "Monospaced"
    with pytest.raises(ValueError, match="denominator must be nonzero"):
        font.metrics = {**fm, "point_size": {"numerator": 1, "denominator": 0}}
    with pytest.raises(ValueError):
        font.metrics = {**fm, "spacing": "monospaced"}
    assert fp.Font.from_dict(font.to_dict()) == font


def test_font_metrics_names_charset():
    font = fp.Font([glyph("#\n#", 65), glyph(".#", 66)])
    font.update_bounds()
    assert font.metrics["bounds"] == {"width": 2, "height": 2, "x": 0, "y": 0}
    names = font.names
    names["family"] = "Demo"
    font.names = names
    assert font.names["family"] == "Demo"
    with pytest.raises(ValueError):
        font.names = {"family": "x", "nope": 1}
    font.charset = {"registry": "ISO8859", "encoding": "1", "code_ranges": []}
    assert font.charset_canonical_name() == "iso88591"
    metrics = font.metrics
    metrics["default_glyph"] = 1
    font.metrics = metrics
    with pytest.raises(fp.InvalidInputError):
        metrics["default_glyph"] = 9
        font.metrics = metrics
    font.remove_glyph(1)
    assert "default_glyph" not in font.metrics
    cell = font.glyph_in_cell(0)
    assert cell.to_rows() == [[1, 0], [1, 0]]
    with pytest.raises(fp.InvalidInputError):
        font.glyph_in_cell(1)
    with pytest.raises(fp.InvalidInputError):
        fp.Font().glyph_in_cell(0)
    font.source_format = "bdf"
    assert font.source_format == "bdf"


def test_font_validate_and_usage():
    font = fp.Font([glyph("##\n##", 65), glyph("#", 66)])
    font.validate()
    with pytest.raises(fp.ResourceLimitError) as e:
        font.validate({"max_glyphs": 1})
    assert (e.value.kind, e.value.resource, e.value.limit) == ("ResourceLimit", "glyph count", 1)
    with pytest.raises(fp.ResourceLimitError) as e:
        font.validate({"max_pixels_per_glyph": 3})
    assert e.value.resource == "glyph pixels"
    with pytest.raises(ValueError):
        font.validate({"unknown_limit": 1})
    usage = font.retained_usage()
    assert usage["fonts"] == 1 and usage["glyphs"] == 2 and usage["glyph_slots"] == 2
    assert usage["bytes"] > 0
    cloned = font.cloned_usage()
    assert cloned["glyphs"] == 2 and 0 < cloned["bytes"] <= usage["bytes"]
    with pytest.raises(fp.ResourceLimitError):
        font.retained_usage(10)
    with pytest.raises(fp.ResourceLimitError):
        font.cloned_usage(10)


def test_font_dict_roundtrip_and_protocols():
    font = fp.Font([glyph("#", 65), glyph("##", 66)])
    font.remove_glyph(0)
    data = font.to_dict()
    assert data["glyphs"][0] is None
    again = fp.Font.from_dict(data)
    assert again == font and again.slot_count == 2 and again.glyph_id(66) == 1
    with pytest.raises(ValueError):
        fp.Font.from_dict({**data, "extra": 1})
    c = copy.copy(font)
    d = copy.deepcopy(font)
    c.add_glyph(glyph("#"))
    assert len(font) == 1 and len(c) == 2 and d == font
    assert font != c and font != "font"
    assert "Font(glyphs=1" in repr(font)
    g = fp.Glyph.from_dict(font.glyph(1).to_dict())
    assert g == font.glyph(1)
    assert copy.deepcopy(g) == g
    assert "Glyph(id=None" in repr(g)


# FontSet --------------------------------------------------------------------


def test_fontset_api():
    a = fp.Font([glyph("#", 65)])
    b = fp.Font([glyph("##", 66)])
    s = fp.FontSet([a], source_format="bdf")
    assert len(s) == 1 and s.source_format == "bdf"
    assert s.append(b) == 1
    assert [len(f) for f in s] == [1, 1]
    assert s[-1].glyph_id(66) == 0
    with pytest.raises(IndexError):
        s[2]
    with pytest.raises(fp.InvalidInputError) as e:
        s.single()
    assert str(e.value) == "expected one font, found 2"
    with pytest.raises(fp.InvalidInputError):
        s.into_single()
    popped = s.pop()
    assert not popped.is_view and popped == b
    single = s.single()
    assert single.is_view
    owned = s.into_single()
    assert not owned.is_view and owned == a and len(s) == 1
    single.add_glyph(glyph("#", 67))
    assert len(s[0]) == 2 and len(owned) == 1
    s.fonts = [b, a]
    assert s[0] == b
    s.validate()
    with pytest.raises(fp.ResourceLimitError) as e:
        s.validate({"max_glyphs": 1})
    assert e.value.resource == "glyph count" and e.value.limit == 1
    usage = s.retained_usage()
    assert usage["fonts"] == 2 and usage["glyphs"] == 2
    with pytest.raises(fp.ResourceLimitError):
        s.retained_usage(1)
    d = fp.FontSet.from_dict(s.to_dict())
    assert d == s
    assert copy.deepcopy(s) == s and s.copy() == s
    assert repr(s) == "FontSet(fonts=2, source_format='bdf')"


# Properties -----------------------------------------------------------------


def test_properties_list():
    p = fp.Properties()
    assert len(p) == 0 and p.is_empty()
    p.set("FOUNDRY", "Misc")
    p.push("COMMENT", "one")
    p.push("COMMENT", "two", raw=b"two")
    p.set("pixel_size", 13)
    p.set("BIG", 2**64 - 1)
    p.set("FLAG", None)
    p.set("BLOB", b"\x00\x01")
    p.set("LIST", [1, "x"])
    p.set("DEC", {"Decimal": "1.5"})
    assert len(p) == 9
    assert p.get("COMMENT") == {"Text": "two"}
    assert p.get("PIXEL_SIZE") is None
    assert p.get_case_insensitive("PIXEL_SIZE") == {"Integer": 13}
    assert p.get("BIG") == {"Unsigned": 2**64 - 1}
    assert p.get("FLAG") == "Flag"
    assert p.get("BLOB") == {"Bytes": [0, 1]}
    assert p.get("LIST") == {"List": [{"Integer": 1}, {"Text": "x"}]}
    p.set("foundry", "Other")  # case-insensitive replace keeps spelling/position
    assert p[0] == {"name": "FOUNDRY", "value": {"Text": "Other"}}
    assert p[2]["raw"] == [116, 119, 111]
    assert p.remove("comment") == 2
    assert [q["name"] for q in p] == ["FOUNDRY", "pixel_size", "BIG", "FLAG", "BLOB", "LIST", "DEC"]
    assert p == p.to_list()
    with pytest.raises(TypeError):
        p.set("X", True)
    with pytest.raises(IndexError):
        p[50]


def test_properties_are_live_views():
    font = fp.Font()
    font.properties.set("FONT_ASCENT", 7)
    font.headers.push("STARTFONT", "2.1")
    font.localized_names.set("ja", "テスト")
    assert font.properties.get("FONT_ASCENT") == {"Integer": 7}
    assert font.to_dict()["headers"] == [{"name": "STARTFONT", "value": {"Text": "2.1"}}]
    assert font.names["localized"] == [{"name": "ja", "value": {"Text": "テスト"}}]
    s = fp.FontSet([font])
    s.properties.set("SET", 1)
    s[0].properties.set("FONT_DESCENT", 2)
    assert s.to_dict()["properties"] == [{"name": "SET", "value": {"Integer": 1}}]
    assert s[0].properties.get("FONT_DESCENT") == {"Integer": 2}
    assert font.properties.get("FONT_DESCENT") is None
    g = fp.Glyph(mono("#"))
    g.properties.set("X", "y")
    assert g.properties.get("X") == {"Text": "y"}
    detached = g.properties.copy()
    detached.set("X", "z")
    assert g.properties.get("X") == {"Text": "y"}
    g.properties = detached
    assert g.properties.get("X") == {"Text": "z"}
    font.properties = [{"name": "A", "value": {"Integer": 1}}]
    assert len(font.properties) == 1
    s.properties = fp.Properties()
    assert s.properties.is_empty()
    assert "FONT_ASCENT" in repr(s[0].properties)
