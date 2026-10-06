"""fontpixel.merge compared with the same Rust calls and the frozen CLI."""
import json

import pytest

import fontpixel
from fontpixel import merge, variants
from sfnt_p3_support import E, font, h, norm

OPTIONS = {"family": "P3 Mix"}


def parts():
    """CLI: --part mini.bdf --take U+20-41 --part alt.bdf --take U+42"""
    return [
        {"font": font("mini"), "selection": "U+20-41"},
        {"font": font("alt"), "selection": merge.Selection("U+42")},
    ]


def test_merge_matches_rust_and_cli():
    r = merge.merge(parts(), OPTIONS)
    assert set(r) == {"font", "report"}
    assert isinstance(r["font"], fontpixel.Font)
    # the merged model equals Rust's (serde dict through the Variants handle)
    assert norm(variants.Variants(r["font"]).to_dict()["base"]) == E["merge.font"]
    assert variants.encode_single(r["font"])["bytes"] == h("merge.single")
    text = merge.report_to_json(r["report"])
    assert text == E["merge.report_json"]
    assert json.loads(text) == E["cli.merge-report.json"]["merge"]
    assert r["report"]["family"] == "P3 Mix"
    # explicit defaults give the same result; Font handles work as part fonts
    full = dict(merge.defaults()["MergeOptions"], family="P3 Mix")
    again = merge.merge(
        [
            {"font": variants.Variants(font("mini")).base, "selection": "U+20-41", "dy": 0, "label": None,
             "license": merge.defaults()["PartLicense"]},
            {"font": font("alt"), "selection": "U+42"},
        ],
        full,
    )
    assert merge.report_to_json(again["report"]) == text


def test_merge_part_fields_and_errors():
    p = parts()
    p[1]["dy"] = 1
    p[1]["label"] = "alt part"
    p[1]["license"] = {"license": "OFL-1.1", "modification": "allowed"}
    r = merge.merge(p, OPTIONS)
    rep = r["report"]["parts"][1]
    assert (rep["label"], rep["dy"]) == ("alt part", 1)
    with pytest.raises(fontpixel.InvalidInputError):
        merge.merge([{"font": font("mini"), "selection": "U+41", "take": "x"}], OPTIONS)
    with pytest.raises(fontpixel.InvalidInputError):
        merge.merge([{"selection": "U+41"}], OPTIONS)
    with pytest.raises(ValueError):
        merge.merge(parts(), {"spacing": "prop"})  # family is required
    with pytest.raises(fontpixel.InvalidInputError):
        merge.merge(parts(), {"family": ""})
    with pytest.raises(fontpixel.InvalidInputError):
        merge.merge([{"font": font("mini"), "selection": "U+zz"}], OPTIONS)
    with pytest.raises(fontpixel.StrictLossError) as e:
        merge.merge(parts(), dict(OPTIONS, strict=True))
    assert e.value.kind == "StrictLoss"
    with pytest.raises(fontpixel.ResourceLimitError) as e:
        merge.merge(parts(), dict(OPTIONS, limits={"max_total_bytes": 64}))
    assert e.value.limit == 64


def test_merge_selection_and_enums():
    s = merge.Selection("U+41-5A,-U+45")
    exp = E["merge.selection"]
    assert norm(s.ranges()) == exp["ranges"]
    assert s.scalar_count() == exp["count"]
    assert s.contains(0x45) is exp["contains_45"]
    assert 0x41 in s and 0x45 not in s
    assert s.expression == "U+41-5A,-U+45"
    assert merge.Selection.parse("U+41").scalar_count() == 1
    assert repr(s) == "Selection('U+41-5A,-U+45')" or repr(s) == 'Selection("U+41-5A,-U+45")'
    with pytest.raises(fontpixel.InvalidInputError):
        merge.Selection("not a range")
    assert [merge.spacing_as_str(v) for v in ["auto", "mono", "prop"]] == ["auto", "mono", "prop"]
    assert [merge.geometric_width_as_str(v) for v in ["half", "full"]] == ["half", "full"]
    assert [merge.modification_as_str(v) for v in ["unknown", "allowed", "prohibited"]] == [
        "unknown", "allowed", "prohibited"]
    with pytest.raises(ValueError):
        merge.spacing_as_str("wide")
