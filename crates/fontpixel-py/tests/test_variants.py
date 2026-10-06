"""fontpixel.variants compared with the same Rust calls and the frozen CLI."""
import pytest

import fontpixel
from fontpixel import sfnt, variants
from fontpixel.woff import woff2
from sfnt_p3_support import E, font, h, norm


def sample():
    """CLI: --base mini.bdf --lang zh-Hant=alt.bdf --script latn=latin.bdf"""
    v = variants.Variants(font("mini"))
    v.add_language("zh-Hant", font("alt"))
    v.add_script(["latn"], font("latin"))
    return v


def test_variants_cli_bytes():
    v = sample()
    ttf = v.encode()
    assert ttf["bytes"] == h("variants.ttf") == h("cli.variants.ttf")
    assert norm(ttf["report"]) == E["variants.ttf.report"]
    assert variants.encode(v)["bytes"] == ttf["bytes"]
    otf = v.encode({"flavor": "cff"})
    assert otf["bytes"] == h("variants.otf") == h("cli.variants.otf")
    assert sfnt.Sfnt(otf["bytes"]).flavor == "cff"
    # CLI --shape dot uses dot_size 0.9 and encode_owned
    d = variants.Variants(font("mini"))
    d.add_language("zh-Hant", font("alt"))
    dot = d.encode_owned({"shape": "dot", "dot_size": 0.9})
    assert dot["bytes"] == h("variants.dot") == h("cli.variants-dot.ttf")
    assert d.consumed
    with pytest.raises(fontpixel.InvalidInputError):
        d.encode()
    # CLI --embed-bitmaps --family --vendor
    e = variants.Variants(font("mini")).encode_owned({"embed_bitmaps": True, "family": "P3 Test", "vendor": "TEST"})
    assert e["bytes"] == h("variants.embed.owned") == h("cli.variants-embed.ttf")
    # CLI -o x.woff2 (non-dot shapes default to CFF inside WOFF2)
    w = variants.Variants(font("mini"))
    w.add_language("zh-Hant", font("alt"))
    assert woff2.encode(w.encode({"flavor": "cff"})["bytes"])["bytes"] == h("cli.variants.woff2")


def test_variants_prepared():
    v = sample()
    assert norm(v.plan()) == E["variants.plan"]
    p = v.prepare()
    assert norm(p.plan()) == E["variants.plan"]
    assert p.retained_bytes() > 0
    total = sfnt.defaults()["gsub.LayoutLimits"]["sfnt"]["resources"]["max_total_bytes"]
    assert p.encode_allowance() == total - p.retained_bytes()
    assert p.encode()["bytes"] == h("variants.ttf")
    assert p.encode({"flavor": "cff"})["bytes"] == h("variants.otf")
    assert not v.consumed
    owned = v.into_prepared()
    assert v.consumed
    assert owned.encode()["bytes"] == h("variants.ttf")
    with pytest.raises(fontpixel.InvalidInputError):
        v.into_prepared()
    with pytest.raises(fontpixel.InvalidInputError):
        p.encode({"limits": {"max_records": 5}})
    with pytest.raises(fontpixel.ResourceLimitError):
        sample().prepare({"max_records": 1})


def test_variants_model():
    v = sample()
    langs = v.languages
    assert [(l["language"], l["scripts"]) for l in langs] == [("ZHT ", ["DFLT", "hani", "kana", "hang", "bopo"])]
    assert isinstance(langs[0]["font"], fontpixel.Font)
    assert [s["scripts"] for s in v.scripts] == [["latn"]]
    d = v.to_dict()
    assert norm(d["base"]) == font("mini")
    assert variants.Variants.from_dict(d).encode()["bytes"] == h("variants.ttf")
    # Font handles are accepted wherever fonts are
    w = variants.Variants(v.base)
    w.languages = [{"language": "ZHT ", "scripts": ["DFLT", "hani", "kana", "hang", "bopo"], "font": langs[0]["font"]}]
    w.scripts = [{"scripts": ["latn"], "font": v.scripts[0]["font"]}]
    assert w.encode()["bytes"] == h("variants.ttf")
    x = variants.Variants(font("latin"))
    x.base = font("mini")
    x.add_language_variant("ZHT ", ["DFLT", "hani", "kana", "hang", "bopo"], font("alt"))
    x.add_script([b"latn"], font("latin"))
    assert x.encode()["bytes"] == h("variants.ttf")
    assert v.retained_usage(1 << 30) > 0
    with pytest.raises(fontpixel.ResourceLimitError) as e:
        v.retained_usage(10)
    assert (e.value.resource, e.value.limit) == ("retained variants bytes", 10)


def test_variants_single_and_tags():
    assert variants.encode_single(font("mini"))["bytes"] == h("variants.single")
    assert variants.encode_single_owned(font("mini"))["bytes"] == h("variants.single")
    emb = {"embed_bitmaps": True, "family": "P3 Test", "vendor": "TEST"}
    assert variants.encode_single(font("mini"), emb)["bytes"] == h("variants.embed")
    assert [variants.language_tag(t) for t in ["zh-Hant", "zh-HK", "ja", "ko", "ZHS"]] == E["variants.language_tag"]
    assert variants.script_tag("latn") == "latn"
    with pytest.raises(fontpixel.InvalidInputError):
        variants.script_tag("latin")
    with pytest.raises(fontpixel.InvalidInputError):
        variants.language_tag("xx-not-a-language")
    assert variants.notdef_empty_metrics({"empty": {"advance_width": 600, "left_side_bearing": -5}}) == tuple(
        E["variants.notdef_empty"]
    )
    assert variants.notdef_empty_metrics("preserve") is None


def test_variants_errors_and_options():
    d = variants.defaults()
    assert d["Options"]["units_per_pixel"] == 100 and d["Options"]["vendor"] == "NONE"
    assert d["Options"]["notdef"] == "preserve" == d["NotdefPolicy"]
    assert d["Options"]["metadata"] == d["MetadataOptions"]
    assert variants.Variants(font("mini")).encode(d["Options"]) == variants.Variants(font("mini")).encode()
    assert variants.encode_single(font("mini"), d["Options"])["bytes"] == h("variants.single")
    variants.validate_metadata(d["MetadataOptions"], "true_type", "square")
    with pytest.raises(fontpixel.InvalidInputError):
        variants.validate_metadata({"selection_flags": 0x80}, "true_type", "square")
    with pytest.raises(ValueError):
        variants.validate_metadata({}, "true_type", "circle")
    with pytest.raises(ValueError):
        variants.Variants(font("mini")).encode({"no_such_option": True})
    with pytest.raises(fontpixel.InvalidInputError):
        variants.Variants(font("mini")).encode({"shape": "square", "dot_size": 0.5})
    with pytest.raises(fontpixel.InvalidInputError):
        variants.Variants(font("mini")).add_language("not a tag", font("alt"))
    bigger = dict(font("alt"), metrics=dict(font("alt")["metrics"], pixel_size=9))
    v = variants.Variants(font("mini"))
    v.add_language("ja", bigger)
    with pytest.raises(fontpixel.InvalidInputError):
        v.encode()
    with pytest.raises(fontpixel.ResourceLimitError):
        sample().encode({"limits": {"sfnt": {"resources": {"max_total_bytes": 1000}}}})
