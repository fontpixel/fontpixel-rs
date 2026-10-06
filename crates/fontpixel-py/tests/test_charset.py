"""fontpixel.charset through Python, compared with the core's fixtures and tests."""
import json

import pytest

import fontpixel as fp
from conftest import CORE_TEST_DATA


def text(value):
    return None if value is None else str(value)


def test_canonicalization_matches_reference_fixture():
    fixture = json.loads((CORE_TEST_DATA / "charset_canonical.json").read_text())
    for name, sanitized, canonical in fixture["names"]:
        assert fp.sanitize_charset(name, None) == sanitized, name
        assert fp.canonical_charset(name) == canonical, name
    for registry, encoding, expected in fixture["pairs"]:
        assert fp.sanitize_charset(text(registry), text(encoding)) == expected
    for key, (reg, enc) in fixture["props"]:
        assert fp.charset_props(key) == (reg, enc)
    assert fp.canonical_charset("ISO8859-000000000000000000000000000000000001") == "iso88591"
    huge = "9" * 1000
    assert fp.sanitize_charset("ISO8859", huge) == "iso8859" + huge
    assert fp.canonical_charset("ISO8859-١") == "iso88591"


def test_charset_table_and_codecs():
    table = fp.charset_table()
    assert len(table) == 62
    for entry in table:
        assert entry["available"]  # the Python build enables cjk-charsets
        codec = fp.CharsetCodec.from_key(entry["key"])
        assert codec.known and codec.name == entry["key"]
        assert codec.kind == entry["kind"]
        assert codec.gl == entry["force_gl"]
        assert codec.cp2char(65) == "A"
    keys = {e["key"] for e in table}
    assert {"iso88591", "cp1252", "cp932", "gb18030"} <= keys
    cp932 = next(e for e in table if e["key"] == "cp932")
    assert cp932["kind"] == "double_byte" and cp932["python_codec"] == "cp932"
    for name in ["HZ", "iso2022jp", "FontSpecific", "KOI8-RU"]:
        codec = fp.CharsetCodec(name)
        assert not codec.known
        assert codec.cp2unicode(0x1F600) == 0x1F600
    assert "CharsetCodec(name=" in repr(codec)


def fnv(pairs):
    h, n = 0xCBF29CE484222325, 0
    for key, value in pairs:
        for byte in key.to_bytes(4, "little") + value.to_bytes(4, "little"):
            h = ((h ^ byte) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
        n += 1
    return n, f"{h:016x}"


@pytest.mark.parametrize("key", ["cp1252", "iso88591", "koi8r", "cp437"])
def test_decode_tables_match_python_generated_digests(key):
    expected = json.loads((CORE_TEST_DATA / "charset_maps.json").read_text())[key]["decode"]
    codec = fp.CharsetCodec.from_key(key, False)
    pairs = ((c, u) for c in range(0x10000) if (u := codec.cp2unicode(c)) is not None)
    assert fnv(pairs) == (expected["count"], expected["fnv1a64"])


def test_single_byte_strictness_and_exact_variants():
    latin = fp.CharsetCodec("ISO-8859-1")
    windows = fp.CharsetCodec("cp1252")
    assert latin.cp2char(0x80) == "\x80"
    assert windows.cp2char(0x80) == "€"
    assert windows.cp2char(0x81) is None
    assert windows.char2cp("\x81") is None
    assert windows.char2cp("€") == 0x80 and windows.unicode2cp(0x20AC) == 0x80
    assert latin.cp2char(256) is None
    jis = fp.CharsetCodec("jisx0201", True)
    assert not jis.gl
    assert jis.char2cp("¥") is None
    assert jis.cp2char(0x5C) == "\\"
    assert jis.cp2char(0xA1) == "｡"


def test_cjk_tables_are_compiled_in():
    cp932 = fp.CharsetCodec("cp932")
    assert cp932.cp2char(0x8160) == "～"
    assert cp932.char2cp("ⅰ") == 0xEEEF
    sjis = fp.CharsetCodec("shiftjis")
    assert sjis.cp2char(0x8160) == "〜"
    gl = fp.CharsetCodec("gb2312", True)
    assert gl.gl and gl.cp2char(0x3021) == "啊" and gl.char2cp("啊") == 0x3021
    gb = fp.CharsetCodec("gb18030")
    assert gb.kind == "gb18030"
    assert gb.cp2char(0x81308130) == "\x80"
    assert gb.char2cp("\U0010ffff") == 0xE3329A35
    for key in ["unicode", "utf16be", "unknown"]:
        identity = fp.CharsetCodec.from_key(key)
        assert identity.cp2unicode(0xD800) == 0xD800
        assert identity.cp2char(0xD800) is None
    assert identity.unicode2cp(0x110000) is None


def test_font_charset_codec():
    font = fp.Font()
    font.charset = {"registry": "JISX0208.1983", "encoding": "0", "code_ranges": []}
    assert font.charset_canonical_name() == fp.charset_canonical_name(font)
    codec = fp.CharsetCodec.for_font(font)
    assert codec.name == font.charset_canonical_name() and codec.known
    assert fp.charset_canonical_name({"name": "ISO-8859-1", "code_ranges": []}) == "iso88591"
    codec = fp.CharsetCodec.for_font({"name": "cp1252", "code_ranges": []})
    assert codec.cp2char(0x80) == "€"
    with pytest.raises(ValueError):
        fp.charset_canonical_name({"bogus": 1})


def test_gl_windows_charsets_and_kuten():
    assert fp.detect_gl("gb2312", None, [0xC4E3, 65, 0x3021])
    assert not fp.detect_gl("gb2312", None, [0x0100, 0x3021])
    assert fp.detect_gl("gb2312", "0", [])
    assert not fp.detect_gl("gbk", "0", [0x3021])
    assert fp.detect_gl("iso2022cn", None, [])
    assert fp.is_unicode_charset("utf32le") and not fp.is_unicode_charset("utf9")
    assert fp.windows_charset(0) == {"code_page": "cp1252"}
    assert fp.windows_charset(1) == "system_ansi"
    assert fp.windows_charset(2) == "symbol"
    assert fp.windows_charset(77) == "system_mac"
    assert fp.windows_charset(255) == "system_oem"
    assert fp.windows_charset(128) == {"code_page": "cp932"}
    assert fp.windows_charset(42) == {"unknown": 42}
    for row in range(1, 95):
        for cell in range(1, 95):
            assert fp.euc_to_kuten(fp.kuten_to_euc(row, cell)) == (row, cell)
    assert fp.kuten_to_euc(16, 1) == 0xB0A1
    assert fp.kuten_to_euc(0, 1) is None
    assert fp.kuten_to_euc(1, 95) is None
    assert fp.euc_to_kuten(0x3021) is None


def test_merge_categories():
    assert fp.UNICODE_VERSION == "17.0.0"
    for ranges in [fp.CJK_RANGES, fp.GEOMETRIC_RANGES, fp.UNICODE_BLOCKS]:
        assert isinstance(ranges, tuple) and ranges
        for a, b in zip(ranges, ranges[1:]):
            assert a["end"] < b["start"]
    for r in fp.CJK_RANGES:
        assert fp.is_cjk(r["start"]) and fp.is_cjk(r["end"])
    for cp in list(range(0, 0x3000, 7)) + [0xD800, 0xDFFF, 0x110000, 0x4E00, 0x1F600]:
        assert (fp.is_cjk(cp) or fp.is_western(cp)) == fp.is_scalar(cp)
        assert not (fp.is_cjk(cp) and fp.is_western(cp))
    for start, end in [(0x2500, 0x259F), (0x2800, 0x28FF), (0xE0A0, 0xE0D4)]:
        assert fp.is_geometric(start) and fp.is_geometric(end)
        assert not fp.is_geometric(start - 1) and not fp.is_geometric(end + 1)
        assert fp.is_western(start)
    assert fp.is_cjk(0xFF00) and fp.is_cjk(0x1F2FF)
    assert fp.is_han(0x323B0) and fp.is_han(0x3347F) and not fp.is_han(0x33480)
    assert fp.east_asian_width(0xFF21) == "fullwidth"
    assert fp.east_asian_width(0xFF66) == "halfwidth"
    assert fp.east_asian_width(0x4E00) == "wide"
    assert fp.east_asian_width(65) == "narrow"
    assert fp.east_asian_width(0xE0A0) == "ambiguous"
    assert fp.east_asian_width(0x0300) == "ambiguous"
    assert fp.east_asian_width(0xD800) is None
    assert fp.unicode_block(65) == {"start": 0, "end": 0x7F, "name": "Basic Latin"}
    assert fp.unicode_block(0x2FE0) is None
