"""fontpixel.bitmap through Python, ported from crates/fontpixel/tests/bitmap.rs."""
import copy

import pytest

import fontpixel as fp

I64_MIN, I64_MAX, U32_MAX = -(2**63), 2**63 - 1, 2**32 - 1
SMALL = {"max_dimension": 20, "max_pixels": 20, "max_bytes": 20}


def mono(text):
    return fp.Bitmap.from_text(text)


def gray(rows):
    return fp.Bitmap.from_raw(len(rows[0]) if rows else 0, len(rows), bytes(sum(rows, [])), depth="gray8")


def test_canonical_storage_limits_and_invalid_input():
    b = fp.Bitmap.from_raw(9, 2, bytes([0xFF, 0xFF, 0x80, 0x7F]))
    assert b.as_bytes() == bytes([0xFF, 0x80, 0x80, 0])
    assert b.stride == 2 and b.depth == "mono" and b.packed and b.max_value == 1
    with pytest.raises(fp.InvalidInputError):
        b.get_pixel(9, 0)
    with pytest.raises(fp.InvalidInputError):
        b[0, 2]
    c = b.copy()
    with pytest.raises(fp.InvalidInputError) as e:
        c.set_pixel(0, 0, 2)
    assert str(e.value) == "pixel value is not representable in this bitmap format"
    assert b == c
    with pytest.raises(fp.ResourceLimitError) as e:
        fp.Bitmap(U32_MAX, 0)
    assert e.value.resource == "bitmap dimension" and e.value.limit == 1048576
    with pytest.raises(fp.FontpixelError):
        fp.Bitmap(U32_MAX, U32_MAX)
    with pytest.raises(fp.InvalidInputError):
        fp.Bitmap.from_raw(9, 2, b"\x00")
    with pytest.raises(fp.ResourceLimitError):
        fp.Bitmap(5, 5, limits=SMALL)
    with pytest.raises(fp.ResourceLimitError):
        fp.Bitmap.with_limits(4, 6, SMALL, depth="gray8")
    one = mono("#")
    for call in [
        lambda: one.crop(U32_MAX, 1, 0, 0),
        lambda: one.enlarge(0, 1),
        lambda: one.enlarge(U32_MAX, 2),
        lambda: one.rotate(45),
        lambda: one.italic(0, 0),
        lambda: one.byte_pad(0),
        lambda: one.shadow(I64_MIN, 0, 1),
        lambda: one.shadow(0, I64_MAX, 1),
        lambda: one.bold(U32_MAX),
        lambda: one.extend(U32_MAX, 1, 0, 0),
        lambda: one.concat(mono("#"), "right", "start", I64_MAX),
        lambda: fp.Bitmap.concat_all([one], "right", "start", [0], SMALL),
    ]:
        with pytest.raises(fp.FontpixelError):
            call()
    with pytest.raises(fp.InvalidInputError):
        fp.Bitmap(1, 1, depth="rgb")


def test_bitmap_buffers_and_rows():
    data = bytearray([0xA0, 0xE0])
    a = fp.Bitmap.from_raw(3, 2, data)
    b = fp.Bitmap.from_raw(3, 2, memoryview(bytes(data)))
    c = fp.Bitmap.from_raw(3, 2, [0xA0, 0xE0])
    assert a == b == c == mono("#.#\n###")
    data[0] = 0  # the bitmap owns a copy
    assert a.to_rows() == [[1, 0, 1], [1, 1, 1]]
    assert a.to_list() == [1, 0, 1, 1, 1, 1]
    assert fp.Bitmap.from_rows(a.to_rows()) == a
    g = fp.Bitmap.from_rows([[0, 200], [255, 1]], depth="gray8")
    assert g.depth == "gray8" and not g.packed and g.max_value == 255 and g.stride == 2
    assert g.as_bytes() == bytes([0, 200, 255, 1])
    with pytest.raises(fp.InvalidInputError):
        fp.Bitmap.from_rows([[1, 0], [1]])
    with pytest.raises(fp.InvalidInputError):
        fp.Bitmap.from_rows([[2]])
    with pytest.raises(TypeError):
        fp.Bitmap.from_raw(1, 1, "x")
    g[1, 0] = 7
    assert g[1, 0] == 7 and g.get_pixel(1, 0) == 7
    assert g.to_dict() == {"gray8": {"width": 2, "height": 2, "data": [0, 7, 255, 1]}}
    assert fp.Bitmap.from_dict(g.to_dict()) == g
    assert a.to_dict() == {"mono": {"width": 3, "height": 2, "data": [160, 224]}}
    assert fp.Bitmap.from_dict({"mono": {"width": 9, "height": 1, "data": [255, 255]}}).as_bytes() == b"\xff\x80"
    with pytest.raises(ValueError):
        fp.Bitmap.from_dict({"mono": {"width": 9, "height": 1, "data": [255]}})


def test_bitmap_limits():
    b = fp.Bitmap(4, 4)
    assert b.limits == fp.default_bitmap_limits()
    assert b.limits == {"max_dimension": 1048576, "max_pixels": 16777216, "max_bytes": 67108864}
    b.set_limits(SMALL)
    assert b.limits == SMALL
    with pytest.raises(fp.ResourceLimitError) as e:
        b.enlarge(3, 3)
    assert e.value.resource == "bitmap pixels"
    with pytest.raises(fp.ResourceLimitError):
        b.set_limits({"max_dimension": 2})
    with pytest.raises(ValueError):
        b.set_limits({"max_size": 2})
    # Limits are inherited and not compared by ==.
    assert b.crop(2, 2).limits == SMALL
    assert b == fp.Bitmap(4, 4)


def test_zero_dimensions_are_explicit():
    for w, h in [(0, 0), (0, 4), (5, 0)]:
        b = fp.Bitmap(w, h)
        assert (b.width, b.height) == (w, h)
        assert b.as_bytes() == b""
        assert b.bbox() == {"width": 0, "height": 0, "x": 0, "y": 0}
        assert b.trim() == fp.Bitmap()
        assert b.rotate(90).rotate(-90) == b
        assert b.flip_horizontal().flip_horizontal() == b
        assert b.flip_vertical().flip_vertical() == b
        assert b.to_gray8().to_mono(128) == b
        assert b.to_bytes() == b""
    assert fp.Bitmap(1, 0) != fp.Bitmap(2, 0)
    z = fp.Bitmap(0, 3)
    assert z.to_numeric_hex_rows() == ["", "", ""]
    assert z.to_row_values() == [0, 0, 0]
    a = mono("#")
    b = fp.Bitmap.concat_all([z, a, z, a], "down", "start", [999, 999, 0])
    assert str(b) == "#\n#"


def test_strings_palettes_hex_and_thresholds():
    b = fp.Bitmap.from_text(" \r\n.#\r\n10\n\n")
    assert str(b) == ".#\n#."
    for text in ["#\n##", "&", "# #", "#\n\n#"]:
        with pytest.raises(fp.InvalidInputError):
            fp.Bitmap.from_text(text)
    with pytest.raises(fp.InvalidInputError):
        fp.Bitmap.from_text("x", [("x", 1), ("x", 2)], depth="gray8")
    with pytest.raises(fp.InvalidInputError):
        fp.Bitmap.from_text("x", depth="gray8")
    assert b.to_text({0: "白", 1: "黑"}) == "白黑\n黑白"
    assert b.to_text() == ".#\n#."
    with pytest.raises(fp.InvalidInputError):
        b.to_text({0: "."})
    with pytest.raises(fp.InvalidInputError):
        b.to_text([(0, "."), (0, "#")])
    assert b.to_palette_bytes({0: b"\xff\xff\xff", 1: b"\x00\x00\x00"}) == bytes(
        [255, 255, 255, 0, 0, 0, 0, 0, 0, 255, 255, 255]
    )
    for palette in [{0: b"\x00", 1: b"\x00\x00"}, {0: b"\x00"}, {}]:
        with pytest.raises(fp.InvalidInputError):
            b.to_palette_bytes(palette)
    b = mono("#...#....")
    assert b.to_hex_rows() == ["8800"]
    assert b.to_numeric_hex_rows() == ["110"]
    assert b.to_row_values() == [272]
    assert fp.Bitmap.from_hex_rows(9, ["887f"]) == b
    for rows in [["zz"], ["0"], ["0000"]]:
        with pytest.raises(fp.InvalidInputError):
            fp.Bitmap.from_hex_rows(8, rows)
    with pytest.raises(fp.InvalidInputError):
        fp.Bitmap(65, 1).to_row_values()
    g = gray([[0, 1, 127, 128, 254, 255]])
    assert str(g.to_mono(128)) == "...###"
    assert str(g.to_mono(0)) == "######"
    assert str(g.to_mono(255)) == ".....#"
    inverse = g.copy()
    inverse.invert()
    assert inverse.as_bytes() == bytes([255, 0, 0, 0, 0, 0])
    inverse = g.copy()
    inverse.negate()
    assert inverse.as_bytes() == bytes([255, 254, 128, 127, 1, 0])
    b = mono("#.#")
    b.invert()
    assert b.as_bytes() == b"\x40"
    lab = gray([[0, 3, 1]])
    lab.invert_with(5)
    assert lab.as_bytes() == bytes([5, 0, 0])
    with pytest.raises(fp.InvalidInputError):
        mono("#").invert_with(2)


def test_depth_specific_methods_and_conversions():
    m, g = mono("#."), gray([[0, 9]])
    assert m.to_gray8().as_bytes() == bytes([255, 0])
    assert m.to_gray8_palette(10, 20).as_bytes() == bytes([20, 10])
    for call in [g.to_gray8, lambda: g.to_gray8_palette(0, 1), g.to_hex_rows, g.to_numeric_hex_rows, g.to_row_values]:
        with pytest.raises(fp.InvalidInputError):
            call()
    for call in [m.negate, lambda: m.to_mono(1)]:
        with pytest.raises(fp.InvalidInputError):
            call()
    with pytest.raises(fp.InvalidInputError):
        m.paste(g)
    with pytest.raises(fp.InvalidInputError):
        m.concat(g)
    with pytest.raises(fp.InvalidInputError):
        fp.Bitmap.concat_all([m, g])
    assert "gray8" in str(g)


def test_checked_geometry_and_label_effects():
    b = mono(".....\n..##.\n...#.\n.....")
    assert b.bbox() == {"width": 2, "height": 2, "x": 2, "y": 1}
    assert str(b.trim()) == "##\n.#"
    assert str(mono("#").crop(3, 3, -1, -1)) == "...\n.#.\n..."
    assert str(mono("#").crop_bounds({"width": 3, "height": 3, "x": -1, "y": -1})) == "...\n.#.\n..."
    assert str(mono("##\n.#").shift(1, 1)) == "..\n.."
    assert not b.crop(2, 2, I64_MIN, I64_MAX).has_ink()
    assert not b.shift(I64_MIN, I64_MIN).has_ink()
    assert b.has_ink()
    c = b.copy()
    c.paste(b, I64_MAX, I64_MIN)
    assert b == c
    assert str(mono("#..\n.##").rotate(90)) == ".#\n#.\n#."
    assert str(mono("#.").enlarge(3, 2)) == "###...\n###..."
    assert str(mono("#.").enlarge(2)) == "##..\n##.."
    assert str(mono("#.#").byte_pad(8)) == "#.#....."
    assert str(mono("#.").flip_horizontal()) == ".#"
    assert str(mono("#\n.").flip_vertical()) == ".\n#"
    assert str(mono("#").extend(1, 2, 1, 0)) == "....\n.#.."
    labels = fp.Bitmap.from_text(".#.\n.&.", {".": 0, "#": 1, "&": 2}, depth="gray8")
    assert labels.shadow(1, -1, 2).as_bytes() == bytes([0, 1, 0, 0, 0, 2, 2, 0, 0, 0, 2, 0])
    glow = labels.glow(False, 1, 2)
    assert glow.as_bytes() == bytes([0, 0, 2, 0, 0, 0, 2, 1, 2, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0])
    assert labels.bold(1).as_bytes() == bytes([0, 1, 1, 0, 0, 2, 2, 0])
    assert str(mono("#\n#\n#").italic(2, 1)) == ".#\n.#\n#."
    assert str(mono("#\n#").italic(I64_MAX, I64_MIN)) == "#\n#"
    p = gray([[1, 2, 1, 2], [2, 1, 2, 1]])
    assert p.replace_pattern([1, 2], b"\x02\x01").as_bytes() == bytes([2, 1, 2, 1, 2, 2, 1, 1])
    with pytest.raises(fp.InvalidInputError):
        p.replace_pattern([1, 2], [1])
    assert str(mono("#.").replace_pattern(b"", [1])) == "###.#"
    p = mono("#.")
    p.replace(0, 1)
    assert str(p) == "##"
    p.replace(1, 0)
    assert str(p) == ".."


def scalar(a, b, mode, mx):
    return {
        "over": a if b == 0 else b,
        "replace": b,
        "or": a | b,
        "and": a & b,
        "xor": a ^ b,
        "clear": a & ~b & 0xFF,
        "add": min(a + b, 255, mx),
        "subtract": max(a - b, 0),
        "max": max(a, b),
        "min": min(a, b),
        "multiply": (a * b + mx // 2) // mx,
        "alpha_over": a + b - (a * b + mx // 2) // mx,
    }[mode]


@pytest.mark.parametrize("depth", ["mono", "gray8"])
def test_composite_matches_scalar_reference(depth):
    import random

    rnd = random.Random(0xCAFE)
    mx = 1 if depth == "mono" else 255
    modes = ["over", "replace", "or", "and", "xor", "clear", "add", "subtract", "max", "min", "multiply", "alpha_over"]
    for _ in range(150):
        w, h, sw, sh = rnd.randrange(20), rnd.randrange(6), rnd.randrange(20), rnd.randrange(6)
        a = [[rnd.randrange(mx + 1) for _ in range(w)] for _ in range(h)]
        s = [[rnd.randrange(mx + 1) for _ in range(sw)] for _ in range(sh)]
        x, y = rnd.randrange(-10, 20), rnd.randrange(-4, 8)
        mode = rnd.choice(modes)
        target = fp.Bitmap.from_rows(a, depth=depth) if h else fp.Bitmap(w, 0, depth)
        source = fp.Bitmap.from_rows(s, depth=depth) if sh else fp.Bitmap(sw, 0, depth)
        target.composite(source, x, y, mode)
        expected = [row[:] for row in a]
        for sy in range(sh):
            for sx in range(sw):
                tx, ty = sx + x, sy + y
                if 0 <= tx < w and 0 <= ty < h:
                    expected[ty][tx] = scalar(a[ty][tx], s[sy][sx], mode, mx)
        assert target.to_rows() == expected, (w, h, sw, sh, x, y, mode)


def test_composite_region_paste_overlay():
    source = mono("#..##.#..\n.#####.#.\n####..##.")
    for sx in range(-3, 12, 2):
        for sy in range(-2, 6):
            for tx in range(-3, 10, 3):
                for ty in range(-2, 5, 2):
                    actual = fp.Bitmap(12, 7)
                    expected = actual.copy()
                    actual.composite_region(source, {"width": 8, "height": 4, "x": sx, "y": sy}, tx, ty, "over")
                    for y in range(4):
                        for x in range(8):
                            a, b, c, d = sx + x, sy + y, tx + x, ty + y
                            if 0 <= a < 9 and 0 <= b < 3 and 0 <= c < 12 and 0 <= d < 7:
                                expected[c, d] = source[a, b]
                    assert actual == expected
    t = fp.Bitmap(3, 1)
    t.overlay(mono("#.#"))
    assert str(t) == "#.#"
    t.paste(mono("#"), 1, 0)
    assert str(t) == "###"
    t.composite(mono("#"), 1, 0, "clear")
    assert str(t) == "#.#"
    with pytest.raises(ValueError):
        t.composite(mono("#"), 0, 0, "blend")


def test_concat_directions_and_alignment():
    a, b = mono("#\n#"), mono("##")
    assert a.concat(b) == a.concat(b, "right", "start", 0)
    assert str(a.concat(b, "right", "start", 0)) == "###\n#.."
    assert str(a.concat(b, "right", "end", 0)) == "#..\n###"
    assert str(a.concat(b, "right", "start", 1)) == "#.##\n#..."
    assert str(a.concat(b, "left", "start", 0)) == "###\n..#"
    assert str(a.concat(b, "down", "start", 0)) == "#.\n#.\n##"
    assert str(a.concat(b, "up", "end", 0)) == "##\n.#\n.#"
    assert str(fp.Bitmap.concat_all([a, b, a], "right")) == "####\n#..#"
    with pytest.raises(fp.InvalidInputError):
        fp.Bitmap.concat_all([a, b], "right", "start", [0, 0])
    with pytest.raises(ValueError):
        a.concat(b, "sideways")


def test_all_wire_layouts_and_known_vectors():
    import random

    rnd = random.Random(99)
    for w in [0, 1, 7, 8, 9, 15, 16, 17, 33, 65]:
        b = fp.Bitmap(w, 5)
        for y in range(5):
            for x in range(w):
                b[x, y] = rnd.randrange(2)
        for bit_order in ["msb", "lsb"]:
            for byte_order in ["big", "little"]:
                for row_order in ["top-down", "bottom-up"]:
                    for row_alignment in [1, 2, 4, 8]:
                        for scan_unit in [1, 2, 4, 8]:
                            layout = {
                                "bit_order": bit_order,
                                "byte_order": byte_order,
                                "row_order": row_order,
                                "row_alignment": row_alignment,
                                "scan_unit": scan_unit,
                            }
                            invalid = (bit_order == "msb") != (byte_order == "big") and scan_unit > row_alignment
                            if invalid:
                                with pytest.raises(fp.InvalidInputError):
                                    b.to_bytes(layout)
                                continue
                            data = b.to_bytes(layout)
                            assert fp.Bitmap.from_bytes(w, 5, data, layout) == b
                            if data:
                                with pytest.raises(fp.InvalidInputError):
                                    fp.Bitmap.from_bytes(w, 5, data[:-1], layout)
    b = mono("#........#......")
    layout = {"byte_order": "little", "scan_unit": 2, "row_alignment": 2}
    assert b.to_bytes(layout) == bytes([0x40, 0x80])
    assert b.to_bytes({**layout, "bit_order": "lsb_first"}) == bytes([0x01, 0x02])
    g = gray([[1, 2, 3], [4, 5, 6]])
    layout = {"row_alignment": 4, "row_order": "bottom-up"}
    assert g.to_bytes(layout) == bytes([4, 5, 6, 0, 1, 2, 3, 0])
    assert fp.Bitmap.from_bytes(3, 2, g.to_bytes(layout), layout, depth="gray8") == g
    with pytest.raises(fp.InvalidInputError):
        g.to_bytes({"bit_order": "lsb"})
    assert fp.default_byte_layout() == {
        "bit_order": "msb",
        "byte_order": "big",
        "row_alignment": 1,
        "scan_unit": 1,
        "row_order": "top-down",
    }


def test_bitmap_protocols():
    b = mono("#.\n.#")
    c = copy.copy(b)
    d = copy.deepcopy(b)
    c[0, 0] = 0
    assert b[0, 0] == 1 and d == b and c != b
    assert b != "bitmap"
    assert repr(b) == "Bitmap(width=2, height=2, depth='mono')"
    assert b.__hash__ is None
    assert mono("#") != mono("#").to_gray8()
