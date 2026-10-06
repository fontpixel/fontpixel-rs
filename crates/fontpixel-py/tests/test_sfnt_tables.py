"""fontpixel.sfnt table codecs compared with the same Rust calls."""
import pytest

import fontpixel
from fontpixel import outline, sfnt
from fontpixel.sfnt import bitmap, cmap, gasp, glyf, gsub, metrics, name, os2, post
from sfnt_p3_support import E, GOLDEN, ROOT, h, norm

S = sfnt.Sfnt(GOLDEN)
N = E["sfnt.num_glyphs"]


def test_cmap():
    assert norm(cmap.decode(S.table("cmap"), N)) == E["cmap.decode"]
    mapping = {32: 1, 65: 2, 66: 3, 0x1F600: 4}
    e = cmap.encode(mapping, 5)
    assert [e["data"].hex(), e["has_bmp_subtable"]] == E["cmap.encode"]
    assert cmap.decode(e["data"], 5) == mapping
    bmp = {32: 1, 65: 2}
    assert cmap.encode_records(bmp, 3, True)["data"] == h("cmap.encode_records.bmp_true")
    assert cmap.encode_records(bmp, 3, False)["data"] == h("cmap.encode_records.bmp_false")
    assert cmap.encode_with_records(bmp, 3, "when-needed")["data"] == h("cmap.encode_with_records.when_needed")
    assert cmap.encode_with_records(mapping, 5, "non-bmp")["data"] == h("cmap.encode_with_records.non_bmp")
    with pytest.raises(ValueError):
        cmap.encode_with_records(bmp, 3, "sometimes")
    truncated = (ROOT / "tests/data/sfnt/truncated-format4.cmap").read_bytes()
    with pytest.raises(fontpixel.InvalidInputError):
        cmap.decode(truncated, 10)
    with pytest.raises(fontpixel.ResourceLimitError) as e:
        cmap.encode(mapping, 5, {"max_cmap_mappings": 1})
    assert e.value.limit == 1
    with pytest.raises(fontpixel.InvalidInputError):
        cmap.encode({65: 9}, 5)


def test_name():
    t = name.decode(S.table("name"))
    assert norm(t) == E["name.decode"]
    assert name.encode(t) == h("name.encode")
    assert name.encode_configured(t) == h("name.encode_configured")
    name.validate_configured(t)
    # counts the deserialized copy, spare capacity included (Rust: >= its own decode)
    assert 0 < name.retained_bytes(t, 1 << 20) and E["name.retained_bytes"] > 0
    with pytest.raises(fontpixel.ResourceLimitError):
        name.retained_bytes(t, 8)
    rec = name.record_unicode(4, 0x409, "Mini Test")
    assert norm(rec) == E["name.record_unicode"]
    assert name.record_text(rec) == "Mini Test"
    # record values accept bytes as well as the serde list form
    rec_bytes = dict(rec, value=bytes(rec["value"]))
    assert name.record_text(rec_bytes) == "Mini Test"
    dup = dict(t, records=t["records"] + [t["records"][0]])
    with pytest.raises(fontpixel.InvalidInputError):
        name.validate_configured(dup)
    assert name.language_tag_well_formed("zh-Hant-HK")
    assert not name.language_tag_well_formed("zh_HK")
    with pytest.raises(fontpixel.ResourceLimitError):
        name.decode(S.table("name"), {"max_name_records": 1})


def test_os2():
    t = os2.decode(S.table("OS/2"))
    assert norm(t) == E["os2.decode"]
    assert os2.encode(t) == h("os2.encode")
    assert norm(os2.unicode_ranges({65, 0x4E00, 0x1F600})) == E["os2.unicode_ranges"]
    assert norm(os2.code_page_ranges([65, 0x4E00], [1252, 936])) == E["os2.code_page_ranges"]
    assert os2.average_width([500, 600, 0, 700]) == E["os2.average_width"]
    assert norm(os2.set_unicode_coverage(t, {65, 0x4E00})) == E["os2.set_unicode_coverage"]
    assert norm(t) == E["os2.decode"]  # input dict untouched
    assert [os2.check_embedding_flags(f) for f in [0, 2, 3, 0x0100, 0x0302]] == E["os2.check_embedding_flags"]
    assert [os2.check_selection_flags(f, v) for f, v in [(0x40, 3), (0x80, 3), (0x21, 4), (0x41, 3)]] == E[
        "os2.check_selection_flags"
    ]
    assert [os2.check_code_page_ranges(r) for r in [[0, 0], [1, 0], [0, 0x80000000]]] == E[
        "os2.check_code_page_ranges"
    ]
    with pytest.raises(fontpixel.InvalidInputError):
        os2.decode(S.table("OS/2")[:10])
    with pytest.raises(ValueError):
        os2.encode(dict(t, vendor_id="TOOLONG"))


def test_post_metrics_gasp():
    p = post.decode(S.table("post"), N)
    assert norm(p) == E["post.decode"]
    assert post.encode(p, N) == h("post.encode")
    hdr, mets = metrics.decode("horizontal", S.table("hhea"), S.table("hmtx"), N)
    assert [norm(hdr), norm(mets)] == E["metrics.decode"]
    e = metrics.encode("horizontal", hdr, mets, True)
    assert [e["header"].hex(), e["metrics"].hex(), e["num_long_metrics"]] == E["metrics.encode"]
    assert norm(metrics.header_new("vertical")) == E["metrics.header_new.vertical"]
    assert norm(metrics.header_recompute(hdr, mets, E["metrics.ink"])) == E["metrics.header_recompute"]
    with pytest.raises(fontpixel.InvalidInputError):
        metrics.decode("horizontal", S.table("hhea"), S.table("hmtx")[:2], N)
    with pytest.raises(ValueError):
        metrics.header_new("diagonal")
    vt = sfnt.Sfnt(h("variants.ttf"))
    g = gasp.decode(vt.table("gasp"))
    assert norm(g) == E["gasp.decode"]
    assert gasp.encode(g) == h("gasp.encode")
    with pytest.raises(fontpixel.InvalidInputError):
        gasp.decode(b"\x00")


def test_gsub():
    vt = sfnt.Sfnt(h("variants.ttf"))
    n = E["variants.ttf.num_glyphs"]
    g = gsub.decode(vt.table("GSUB"), n)
    assert norm(g) == E["gsub.decode"]
    assert gsub.encode(g, n) == h("gsub.encode") == vt.table("GSUB")
    assert gsub.closure(g, set(E["gsub.closure.input"])) == E["gsub.closure"]
    assert norm(gsub.subset(g, {i: i for i in range(n)})) == E["gsub.subset"]
    with pytest.raises(fontpixel.InvalidInputError):
        gsub.decode(vt.table("GSUB"), 1)
    with pytest.raises(fontpixel.ResourceLimitError):
        gsub.decode(vt.table("GSUB"), n, {"max_records": 1})


def test_glyf():
    t = glyf.from_sfnt(S)
    assert norm(t) == E["glyf.from_sfnt"]
    head = S.table("head")
    loc_format = int.from_bytes(head[50:52], "big", signed=True)
    assert norm(glyf.decode(S.table("glyf"), S.table("loca"), N, loc_format)) == E["glyf.from_sfnt"]
    e = glyf.encode(t, "auto")
    exp = E["glyf.encode"]
    assert e["glyf"].hex() == exp["glyf"] and e["loca"].hex() == exp["loca"]
    assert e["index_to_loc_format"] == exp["index_to_loc_format"]
    assert norm(e["bounds"]) == exp["bounds"] and norm(e["statistics"]) == exp["statistics"]
    assert glyf.unhinted_maxp(e["statistics"], N) == h("glyf.unhinted_maxp")
    assert norm(glyf.contours(t, 2)) == E["glyf.contours.2"]
    assert norm(glyf.component_translated(3, 10, -20)) == E["glyf.component_translated"]
    assert norm(glyf.glyph_simple([[(0, 0, True), (100, 0, True), (100, 100, True)]])) == E["glyf.glyph_simple"]
    bm = outline.rasterize(E["glyf.contours.2"], 64.0, (0, 0), 5, 7)
    assert norm(glyf.glyph_pixels(bm)) == E["glyf.glyph_pixels"]
    assert norm(glyf.glyph_pixels(bm, {"shape": "dot", "dot_size": 0.9}, 7)) == E["glyf.glyph_pixels.dot"]
    with pytest.raises(fontpixel.InvalidInputError):
        glyf.contours(t, 999)
    with pytest.raises(fontpixel.ResourceLimitError):
        glyf.from_sfnt(S, {"max_total_points": 1})
    with pytest.raises(fontpixel.InvalidInputError):
        glyf.decode(S.table("glyf"), S.table("loca")[:4], N, loc_format)


def test_bitmap():
    es = sfnt.Sfnt(h("variants.embed"))
    n = E["bitmap.num_glyphs"]
    strikes = bitmap.decode(es.table("EBLC"), es.table("EBDT"), "open_type", n)
    assert norm(strikes) == E["bitmap.decode"]
    assert norm(bitmap.decode_with_color(es.table("EBLC"), es.table("EBDT"), "open_type", n, False)) == E[
        "bitmap.decode"
    ]
    t = bitmap.encode(strikes, "open_type", n)
    assert [t["data"].hex(), t["location"].hex()] == E["bitmap.encode"]
    s = bitmap.encode_spanning(strikes, "apple", n)
    assert [s["data"].hex(), s["location"].hex()] == E["bitmap.encode_spanning.apple"]
    r = bitmap.encode_shared_metric_runs(strikes, "open_type", n, 2)
    assert [r["tables"]["data"].hex(), r["tables"]["location"].hex(), r["shared_runs"]] == E["bitmap.runs"]
    assert bitmap.encode_scaling([E["bitmap.scale"]]) == h("bitmap.encode_scaling")
    assert norm(bitmap.decode_scaling(h("bitmap.encode_scaling"))) == [E["bitmap.scale"]]
    assert [bitmap.line_ascender(strikes[0]["horizontal"]), bitmap.line_descender(strikes[0]["horizontal"])] == E[
        "bitmap.line_metrics"
    ]
    assert bitmap.kind_tags("open_type") == ("EBDT", "EBLC")
    assert bitmap.kind_tags("apple") == ("bdat", "bloc")
    assert bitmap.kind_tags("color") == ("CBDT", "CBLC")
    sbix = E["sbix.input"]
    sbix = dict(sbix, strikes=[dict(s, glyphs={int(k): g for k, g in s["glyphs"].items()}) for s in sbix["strikes"]])
    assert bitmap.encode_sbix(sbix, 3) == h("sbix.encode")
    assert norm(bitmap.decode_sbix(h("sbix.encode"), 3)) == E["sbix.decode"]
    with pytest.raises(fontpixel.InvalidInputError):
        bitmap.decode(es.table("EBLC")[:8], es.table("EBDT"), "open_type", n)
    with pytest.raises(fontpixel.ResourceLimitError):
        bitmap.encode(strikes, "open_type", n, {"sfnt": {"resources": {"max_total_bytes": 16}}})
