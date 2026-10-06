"""Typed per-format options as dicts: normalization, setters and codecs."""
import re

import pytest

import fontpixel
import fontpixel.formats as F
from formats_support import SOURCES, assets_of, case, single

TYPES = F.option_types()
# Plain-data types whose fields have no defaults.
REQUIRED_FIELDS = {
    "formats::WriteOptions": {"primary_name": "font.bdf"},
    "formats::c::ArrayOptions": None,
    "formats::hzk::Geometry": None,
    "formats::raw::RawLayout": None,
    "formats::embedded::EmbeddedCodec": None,
    "formats::image::Cell": None,
    "formats::image::NativeGlyph": None,
    "formats::image::NativeMetadata": None,
    "resources::OutputAsset": None,
    "resources::OutputBundle": None,
    "resources::Written": None,
    "resources::ResourcePath": None,
}


def test_every_format_option_type_is_listed():
    assert len(TYPES) == len(set(TYPES)) == 374
    # Every per-format read/write options and codec struct of every module.
    names = {t.rsplit("::", 1)[1] for t in TYPES}
    for mod in F.MODULES:
        if mod in ("x11", "embedded", "vector_raster"):
            continue
        assert any(t.startswith(f"formats::{mod}::") for t in TYPES) or mod in ("bbc",), mod
    assert {"ReadOptions", "WriteOptions", "Confidence", "Options", "EmbeddedWriteOptions"} <= names


@pytest.mark.parametrize("type_name", TYPES)
def test_option_type_round_trips_and_rejects_unknown(type_name):
    try:
        defaults = F.normalize(type_name)
    except ValueError as e:
        message = str(e)
        if "enum" in message:
            # Enums: every unit variant named in the error round-trips.
            with pytest.raises(ValueError) as bad:
                F.normalize(type_name, "definitely-not-a-variant")
            variants = re.findall(r"`([^`]+)`", str(bad.value).split("expected", 1)[-1])
            assert variants, str(bad.value)
            for v in variants:
                try:
                    # Aliases (e.g. raw presets) normalize to a canonical name.
                    assert F.normalize(type_name, v) in variants
                except ValueError as data_variant:
                    # Variants carrying data need an object, not a name.
                    assert "invalid type" in str(data_variant) or "expected" in str(data_variant)
            return
        assert type_name in REQUIRED_FIELDS, (type_name, message)
        sample = REQUIRED_FIELDS[type_name]
        if sample is None:
            return
        defaults = F.normalize(type_name, sample)
    # Defaults round-trip and are stable.
    assert F.normalize(type_name, defaults) == defaults
    if isinstance(defaults, dict):
        with pytest.raises(ValueError, match="unknown field"):
            F.normalize(type_name, {**defaults, "not_a_field": 1})
        # Partial dicts fill in the documented defaults.
        if defaults and type_name not in REQUIRED_FIELDS:
            key = next(iter(defaults))
            assert F.normalize(type_name, {key: defaults[key]}) == defaults


def test_normalize_validation():
    with pytest.raises(KeyError):
        F.normalize("formats::nope::Options")
    with pytest.raises(ValueError):
        F.normalize("formats::fzx::FzxWriteOptions", {"gray_threshold": 256})
    with pytest.raises(ValueError):
        F.normalize("formats::fzx::FzxWriteOptions", {"height": -1})
    with pytest.raises(ValueError):
        F.normalize("formats::iigs::Version", "V1_5")
    assert F.normalize("formats::iigs::Version", "1.5") == "1.5"
    assert F.normalize("formats::pcf::GlyphPad", "4") == "4"
    with pytest.raises(ValueError):
        F.normalize("formats::fbit::FbitReadOptions", {"apple_double_resource": "../x"})
    assert F.normalize("resources::ResourceLimits")["max_glyphs"] == 1_000_000
    assert F.normalize("resources::TransportLimits")["max_output_bytes"] == 64 * 1024 * 1024


def _setters():
    return sorted(n for n in dir(fontpixel.Registry) if re.fullmatch(r"set_\w+_(read|write)_options", n))


def test_every_typed_setter_is_a_method():
    names = _setters()
    assert len(names) == 170
    assert "set_vector_read_options" in names and "set_color_write_options" in names


@pytest.mark.parametrize("name", _setters())
def test_typed_setter_accepts_defaults_and_rejects_unknown(name):
    reg = fontpixel.Registry()
    getattr(reg, name)({})
    with pytest.raises(ValueError, match="unknown field"):
        getattr(reg, name)({"not_a_field": True})
    assert getattr(reg, name).__doc__


@pytest.mark.parametrize("fid", sorted(F.configurable_formats()["read"]))
def test_read_options_by_format_id(fid):
    reg = fontpixel.Registry()
    reg.set_read_options(fid, {})
    with pytest.raises(ValueError):
        reg.set_read_options(fid, {"not_a_field": 1})


@pytest.mark.parametrize("fid", sorted(F.configurable_formats()["write"]))
def test_write_options_by_format_id(fid):
    reg = fontpixel.Registry()
    reg.set_write_options(fid, {})
    with pytest.raises(ValueError):
        reg.set_write_options(fid, {"not_a_field": 1})


def test_setter_dispatch_errors_and_effects():
    configurable = F.configurable_formats()
    assert len(configurable["write"]) == 98 and "bdf" not in configurable["write"]
    assert len(configurable["read"]) == 85
    reg = fontpixel.Registry()
    with pytest.raises(fontpixel.UnknownFormatError):
        reg.set_read_options("nope", {})
    with pytest.raises(fontpixel.InvalidInputError, match="bdf.write"):
        reg.set_write_options("bdf", {})
    with pytest.raises(fontpixel.InvalidInputError, match="embedded.read"):
        reg.set_read_options("otb", {})
    with pytest.raises(fontpixel.InvalidInputError):
        reg.set_read_options("pcf", {})
    # The fallible setters report a missing writer.
    planned = fontpixel.Registry(builtin=False)
    with pytest.raises(fontpixel.NotImplementedFormatError) as e:
        planned.set_pcf_write_options({})
    assert e.value.operation == "Write"
    with pytest.raises(fontpixel.NotImplementedFormatError):
        planned.set_write_options("yaff", {})
    # Planned entries ignore infallible setters (nothing to configure).
    planned.set_fzx_write_options({})
    # Options change the output exactly as the CLI flags do.
    leg3 = fontpixel.read(open(SOURCES["leg3"], "rb").read())
    with pytest.raises(fontpixel.ReportedErrorsError):
        reg.write(leg3.fonts, "fzx")
    reg.set_options("fzx", write={"tracking": 1})
    configured = reg.write(leg3.fonts, "fzx")
    assert assets_of(configured) == assets_of(fontpixel.write(leg3.fonts, "fzx", case("fzx").write))
    reg2 = fontpixel.Registry()
    reg2.set_codec("fzx", {"write_options": {"tracking": 1}, "read_options": {}})
    assert assets_of(reg2.write(leg3.fonts, "fzx")) == assets_of(configured)
    with pytest.raises(ValueError, match="unknown codec field"):
        reg2.set_codec("fzx", {"options": {}})
    # Read options configured on the registry.
    uni3 = fontpixel.read(open(SOURCES["uni3"], "rb").read())
    hexdata = single(fontpixel.write(uni3.fonts, "hex", {"geometry": {"height": 6, "descent": 1}}))
    reg3 = fontpixel.Registry()
    with pytest.raises(fontpixel.FontpixelError):
        reg3.read(hexdata, "hex")
    reg3.set_hex_read_options({"height": 6, "descent": 1})
    assert reg3.read(hexdata, "hex").format == "hex"
    # fon shares the FNT options.
    reg3.set_read_options("fon", {})
    reg3.set_write_options("fon", {})
