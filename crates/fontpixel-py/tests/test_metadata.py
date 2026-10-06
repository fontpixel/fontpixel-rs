"""fontpixel.metadata, fontpixel.projection, fontpixel.report and error mapping."""
import pytest

import fontpixel as fp


def sample():
    font = fp.Font()
    font.names = {
        "family": "Original",
        "style": "Regular",
        "full": "Original Regular",
        "postscript": "Original-Regular",
        "xlfd_fields": ["source", "original"],
        "localized": [],
    }
    g = fp.Glyph(fp.Bitmap.from_text("#.\n##"), codepoints=[65, 66], name="A")
    m = g.metrics
    m["advance"] = {"x": 4, "y": 0}
    g.metrics = m
    font.add_glyph(g)
    return fp.FontSet([font, font], source_format="source-container")


def test_apply_names_is_atomic_and_bounded():
    fonts = sample()
    before = fonts.copy()
    for value in ["", " \t\n", " "]:
        with pytest.raises(fp.InvalidInputError) as e:
            fp.apply_names(fonts, {"family": value, "style": "Bold"})
        assert e.value.kind == "InvalidInput" and e.value.exit_code == 1
        assert fonts == before
    with pytest.raises(fp.ResourceLimitError) as e:
        fp.apply_names(fonts, {"family": "字母"}, {"max_table_bytes": 5})
    assert (e.value.resource, e.value.limit) == ("metadata name bytes", 5)
    assert fonts == before
    report = fp.apply_names(fonts, {"family": "字母"}, {"max_table_bytes": 6})
    assert fonts[0].names["family"] == "字母" and fonts[1].names["family"] == "字母"
    assert isinstance(report["entries"], list)
    with pytest.raises(ValueError):
        fp.apply_names(fonts, {"famliy": "typo"})
    assert fp.apply_names(fonts) == {"entries": []}
    assert fp.name_overrides_is_empty({})
    assert not fp.name_overrides_is_empty({"family": "x"})


def test_apply_names_doc_example():
    fonts = fp.FontSet([fp.Font()])
    report = fp.apply_names(fonts, {"family": "Example Pixel"})
    assert fonts[0].names["family"] == "Example Pixel"
    assert not fp.report_has_loss(report)


def projection_set():
    g = fp.Glyph(fp.Bitmap.from_text("#"))
    m = g.metrics
    m["advance"] = {"x": 6, "y": 0}
    m["scalable_advance"] = {"x": 500, "y": 0}
    g.metrics = m
    return fp.FontSet([fp.Font([g])])


def test_bitmap_projection_scalable_from_device():
    s = projection_set()
    p = fp.BitmapProjection.scalable_from_device(1000, 13)
    assert not p.is_identity()
    assert p.scalable_advance == {"from_device": {"units_per_em": 1000, "reference_ppem": 13}}
    outcome = fp.apply_bitmap_projection(s, p, 1 << 20)
    assert s[0].glyph(0).metrics["scalable_advance"] == {"x": 462, "y": 0}
    assert len(outcome["scalable_advance_changes"]) == 1
    change = outcome["scalable_advance_changes"][0]
    assert change["glyph"] == 0 and change["after"] == {"x": 462, "y": 0}
    assert fp.report_has_loss(outcome["report"])
    assert outcome["retained_bytes"] > 0


def test_bitmap_projection_metadata_and_edits():
    s = fp.FontSet([fp.Font()])
    p = fp.BitmapProjection(
        font_name="-opf-demo-medium-r-normal--13-130-75-75-p-130-iso10646-1",
        size={"point_size": {"numerator": 13, "denominator": 1}, "resolution": {"x": 75, "y": 75}},
        properties=[{"set": {"name": "PIXEL_SIZE", "value": 13}}],
    )
    assert p.property_names() == ["PIXEL_SIZE"]
    assert p.properties == [{"set": {"name": "PIXEL_SIZE", "value": 13}}]
    assert p.font_name.startswith("-opf-demo") and p.size["resolution"] == {"x": 75, "y": 75}
    outcome = p.apply(s, 1 << 20)
    font = s[0]
    assert font.names["xlfd_fields"][6] == "13"
    assert font.metrics["pixel_size"] == 13
    assert not fp.report_has_loss(outcome["report"])
    assert len(outcome["metadata_changes"]) == 5
    # Remove edits and the dict form of the function.
    out = fp.apply_bitmap_projection(
        s, {"properties": [{"remove": {"name": "pixel_size"}}, {"set": {"name": "WEIGHT_NAME", "value": "Bold"}}]}
    )
    assert s[0].properties.get_case_insensitive("PIXEL_SIZE") is None
    assert s[0].properties.get("WEIGHT_NAME") == {"Text": "Bold"}
    assert out["retained_bytes"] >= 0


def test_bitmap_projection_validation_is_transactional():
    s = projection_set()
    before = s.copy()
    assert fp.BitmapProjection().is_identity()
    assert fp.apply_bitmap_projection(s, fp.BitmapProjection())["metadata_changes"] == []
    with pytest.raises(fp.InvalidInputError):
        fp.apply_bitmap_projection(s, fp.BitmapProjection.scalable_from_device(1000, 0))
    with pytest.raises(fp.InvalidInputError):
        fp.apply_bitmap_projection(s, {"font_name": " padded "})
    edits = [{"set": {"name": f"P{i}", "value": i}} for i in range(fp.MAX_PROPERTY_EDITS + 1)]
    with pytest.raises(fp.InvalidInputError):
        fp.apply_bitmap_projection(s, {"properties": edits})
    with pytest.raises(fp.InvalidInputError):
        fp.apply_bitmap_projection(s, {"font_name": "x" * (fp.MAX_FONT_NAME_BYTES + 1)})
    with pytest.raises(fp.InvalidInputError):
        long_name = "N" * (fp.MAX_PROPERTY_NAME_BYTES + 1)
        fp.apply_bitmap_projection(s, {"properties": [{"set": {"name": long_name, "value": 1}}]})
    with pytest.raises(fp.InvalidInputError):
        big = "t" * (fp.MAX_PROPERTY_TEXT_BYTES + 1)
        fp.apply_bitmap_projection(s, {"properties": [{"set": {"name": "T", "value": big}}]})
    with pytest.raises(fp.ResourceLimitError):
        fp.apply_bitmap_projection(s, fp.BitmapProjection.scalable_from_device(1000, 13), 1)
    assert s == before
    with pytest.raises(fp.InvalidInputError):
        fp.BitmapProjection(properties=[{"rename": {"name": "X"}}])
    with pytest.raises(fp.InvalidInputError):
        fp.BitmapProjection(properties=[{"set": {"name": "X", "value": 1.5}}])
    with pytest.raises(fp.InvalidInputError):
        fp.apply_bitmap_projection(s, {"fontname": "x"})
    assert (fp.MAX_PROPERTY_EDITS, fp.MAX_PROPERTY_NAME_BYTES) == (64, 255)
    assert (fp.MAX_PROPERTY_TEXT_BYTES, fp.MAX_FONT_NAME_BYTES) == (65535, 4096)
    assert "BitmapProjection(" in repr(fp.BitmapProjection())


ENTRY = {
    "level": "warning",
    "impact": "loss",
    "code": "test.loss",
    "location": {"font_index": 0, "glyph_index": None, "codepoint": 65, "asset": None},
    "message": "lost",
}


def test_report_helpers():
    empty = {"entries": []}
    assert not fp.report_has_loss(empty) and not fp.report_has_errors(empty)
    one = fp.report_push(empty, ENTRY)
    assert one == {"entries": [ENTRY]} and empty == {"entries": []}
    assert fp.report_has_loss(one) and not fp.report_has_errors(one)
    err = dict(ENTRY, level="error", impact="none", code="test.error")
    both = fp.report_extend(one, {"entries": [err]})
    assert [e["code"] for e in both["entries"]] == ["test.loss", "test.error"]
    assert fp.report_has_errors(both)
    assert fp.report_to_text(both) == "Warning [test.loss] Loss: lost\nError [test.error] None: lost\n"
    with pytest.raises(ValueError):
        fp.report_push(empty, dict(ENTRY, level="fatal"))
    with pytest.raises(ValueError):
        fp.report_has_loss({"entries": [], "extra": 1})


def test_error_classes_carry_structured_fields():
    assert issubclass(fp.ResourceLimitError, fp.FontpixelError)
    with pytest.raises(fp.ResourceLimitError) as e:
        fp.Bitmap(10, 10, limits={"max_pixels": 5})
    err = e.value
    assert err.kind == "ResourceLimit" and err.exit_code == 1
    assert (err.resource, err.limit) == ("bitmap pixels", 5)
    assert str(err) == "bitmap pixels exceeds configured limit 5"
    with pytest.raises(fp.StrictLossError) as e:
        fp.Font().draw("A", {"strict": True})
    assert e.value.exit_code == 3 and e.value.kind == "StrictLoss"
    assert fp.report_has_loss(e.value.report)
    with pytest.raises(fp.MultipleAssetsError) as e:
        fp.OutputBundle("a", [("a", b"1"), ("b", b"2")]).into_single()
    assert e.value.count == 2 and e.value.kind == "MultipleAssets"
