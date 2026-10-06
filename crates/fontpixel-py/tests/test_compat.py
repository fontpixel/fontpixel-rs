"""fontpixel.compat.bdfparser: the bdfparser-compatible API over `fontpixel::compat`.

Expected values come from the Rust compat tests and goldens
(crates/fontpixel/tests/compat_bdfparser.rs, tests/data/compat/). The full
bdfparser reference suite runs through tests/compat_bdfparser/run_reference.py.
"""

import gzip
import io
import json
import warnings

import pytest

import fontpixel as fp
from fontpixel._native import _bdfparser as native
from fontpixel.compat import bdfparser as bp
from fontpixel.compat.bdfparser import BDFParseError, BDFParserWarning, Bitmap, Font, Glyph
from fontpixel.compat.bdfparser import encoding, outline, pcf, ttf
from fontpixel.compat.bdfparser.font import fmtver
from fontpixel.compat.bdfparser.glyph import META_TITLES

from conftest import REPO

DATA = REPO / 'tests' / 'data' / 'compat'
MINI = (DATA / 'mini.bdf').read_text()


def mini() -> Font:
    return Font().load_string(MINI)


# ---------------------------------------------------------------------------
# module surface


def test_module_surface():
    assert bp.__version__ == '3.0.0'
    assert set(bp.__all__) == {
        'Font', 'Glyph', 'Bitmap', 'BDFParserWarning', 'BDFParseError', 'defaultglyphname'
    }
    assert issubclass(BDFParseError, ValueError) and issubclass(pcf.PCFError, BDFParseError)
    assert issubclass(BDFParserWarning, UserWarning)
    assert META_TITLES == tuple(native.META_TITLES) and len(META_TITLES) == 17
    assert META_TITLES[0] == 'glyphname' and META_TITLES[16] == 'hexdata'


def test_defaultglyphname_and_fmtnum():
    assert bp.defaultglyphname(65) == 'uni0041'
    assert bp.defaultglyphname(0x1F600) == 'u1F600'
    assert bp.defaultglyphname(65, False) == 'char65'
    assert bp.defaultglyphname(-1) == 'char-1'
    assert bp.bitmap.fmtnum(3.0) == '3' and bp.bitmap.fmtnum(-0.0) == '0'
    assert bp.bitmap.fmtnum(12.8) == '12.8' and bp.bitmap.fmtnum(0.1 + 0.2) == '0.30000000000000004'
    assert native.fmtnum(2.5) == '2.5'
    assert fmtver(2) == '2.0' and fmtver(2.1) == '2.1'


# ---------------------------------------------------------------------------
# Bitmap


def test_bitmap_construction_and_access():
    b = Bitmap(['012', '100'])
    assert b.bindata == ['012', '100'] and b.width() == 3 and b.height() == 2
    assert str(b) == '.#&\n#..'
    assert repr(b) == "Bitmap(['012',\n        '100'])"
    assert b == Bitmap(['012', '100']) and b != Bitmap(['000', '000']) and b != ['012', '100']
    assert b.equals(Bitmap(['012', '100']))
    with pytest.raises(TypeError):
        hash(b)
    assert Bitmap.fromstring('\n.#&\n#..\n').bindata == ['012', '100']
    assert Bitmap.blank(3, 2).bindata == ['000', '000']
    assert Bitmap.blank(7, 0).width() == 0  # Python loses W for W x 0
    b.bindata = ['1']
    assert b.bindata == ['1']
    c = b.clone().setpixel(0, 0, 0)
    assert b.bindata == ['1'] and c.bindata == ['0']


def test_bitmap_errors():
    with pytest.raises(ValueError, match='different lengths'):
        Bitmap(['110', '1'])
    with pytest.raises(ValueError, match="Invalid character 'x'"):
        Bitmap.fromstring('.#\n.x')
    with pytest.raises(ValueError, match='only'):
        Bitmap(['013'])  # native rasters hold labels 0, 1, 2
    with pytest.raises(ValueError):
        Bitmap.blank(-1, 2)
    with pytest.raises(IndexError):
        Bitmap(['01']).getpixel(2, 0)
    with pytest.raises(ValueError):
        Bitmap(['01']).setpixel(0, 0, 3)
    b = Bitmap(['11', '10'])
    with pytest.raises(ValueError):
        b.replace('11', '')  # ragged result rejected, bitmap unchanged
    assert b.bindata == ['11', '10']
    with pytest.raises(ValueError):
        b.bold(-1)
    with pytest.raises(ValueError):
        b.todata(9)
    with pytest.raises(ValueError):
        b.rotate(45)
    try:
        Bitmap(['2']).todata(4)
    except ValueError as e:
        assert isinstance(e, fp.InvalidInputError) and e.kind == 'InvalidInput'
    else:
        raise AssertionError('todata(4) with label 2 must fail')


def test_bitmap_geometry():
    T = Bitmap.fromstring
    assert T('##.\n#..').crop(2, 1).bindata == ['10']
    assert T('##.\n#..').crop(4, 3, -1, 0).bindata == ['0000', '0110', '0100']
    assert T('#.\n..').overlay(T('.#\n#.')).bindata == ['11', '10']
    assert Bitmap.concatall([T('#'), T('.\n#')]).bindata == ['00', '11']
    assert Bitmap.concatall([T('#'), T('#')], 0).bindata == ['1', '1']
    assert Bitmap.concatall([T('#'), T('#')], 1, 1, [1]).bindata == ['101']
    assert T('#').concat(T('#'), 2).bindata == ['11']
    assert (T('#.') + T('#')).bindata == ['101']
    assert (T('#.') * 2).bindata == ['1100', '1100']
    assert (T('#.') * (1, 2)).bindata == ['10', '10']
    assert T('#.').enlarge(2, 1).bindata == ['1100']
    assert T('#.').replace(1, 2).bindata == ['20']
    assert T('#').shadow().bindata == ['10', '02']
    # expected values: bdfparser 3.0 (reference implementation)
    assert T('...\n.#.\n...').glow().bindata == ['00000', '00200', '02120', '00200', '00000']
    assert T('...\n.#.\n...').glow(1).bindata == ['00000', '02220', '02120', '02220', '00000']
    assert T('#').bytepad(4).bindata == ['1000']
    assert T('.#&\n#..').getpixel(2, 0) == 2
    assert T('##.\n#..').fliph().bindata == ['011', '001']
    assert T('##.\n#..').flipv().bindata == ['100', '110']
    assert T('##.\n#..').rotate().bindata == ['11', '01', '00']
    assert T('.#&').invert().bindata == ['100']
    assert T('#..\n...').shift(1, -1).bindata == ['000', '010']
    assert T('.....\n..#..\n.#&..\n.....').bbox() == (2, 2, 1, 1)
    assert T('.....\n..#..\n.#&..\n.....').trim().bindata == ['01', '12']
    assert T('#.#..').bold().bindata == ['111100']
    assert T('#\n#\n#\n#\n#').italic().bindata == ['001', '010', '010', '100', '100']
    assert Bitmap.blank(4, 3).paste(T('#&\n.#'), 1, 1).bindata == ['0000', '0120', '0010']


def test_bitmap_chaining_returns_same_object():
    b = Bitmap(['01'])
    assert b.crop(2, 1) is b and b.invert() is b and b.setpixel(0, 0) is b and b.trim() is b


def test_bitmap_data_exports():
    b = Bitmap(['01', '12'])
    assert b.todata(0) == '01\n12' and b.todata(1) == ['01', '12']
    assert b.todata(2) == [[0, 1], [1, 2]] and b.todata(3) == [0, 1, 1, 2]
    assert Bitmap(['0101']).todata(4) == ['5']
    assert Bitmap(['1' * 70]).todata(5) == [2**70 - 1]  # exact beyond 64 bits
    one = Bitmap(['012'])
    assert one.tobytes('L') == bytes([255, 0, 127])
    assert one.tobytes('1') == bytes([0x9F])
    assert one.tobytes() == bytes([255, 255, 255, 0, 0, 0, 255, 0, 0])
    assert one.tobytes('L', {0: b'a', 1: b'b', 2: b'c'}) == b'abc'
    with pytest.warns(BDFParserWarning, match='Unknown mode'):
        assert one.tobytes('CMYK') == one.tobytes('RGB')
    with pytest.warns(BDFParserWarning, match='different width'):
        Bitmap(['1']).overlay(Bitmap(['11']))


def test_bitmap_svg():
    # expected values: bdfparser 3.0 (reference implementation)
    b = Bitmap(['1'])
    assert b.tosvg(format='pathOnly') == 'M0 1V0H1V1Z'
    assert b.tosvg() == (
        '<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1">'
        '<path d="M0 1V0H1V1Z" fill="#000000" /></svg>'
    )
    assert b.tosvg(format='rect') == (
        '<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1">'
        '<rect x="0" y="0" width="1" height="1" fill="#000000" /></svg>'
    )
    assert b.tosvg(shape='circle', format='pathOnly') == 'M0 0.5a0.5 0.5 0 1 0 1 0a0.5 0.5 0 1 0 -1 0Z'
    assert b.tosvg(2, 'pathOnly', True, 1) == 'M1 0V2H3V0Z'
    assert Bitmap([]).tosvg(format='pathOnly') == ''


def test_bitmap_subclass_construction():
    class Mine(Bitmap):
        pass

    m = Mine.fromstring('#.')
    assert type(m) is Mine and type(m.clone()) is Mine and type(Mine.blank(1, 1)) is Mine


# ---------------------------------------------------------------------------
# outline / ttf / svg goldens


def test_outline_functions():
    assert outline.bitmap_contours([]) == []
    assert outline.bitmap_contours(['1']) == [[(0, 0, True), (0, 1, True), (1, 1, True), (1, 0, True)]]
    assert outline.bitmap_contours(['1'], ppu=64, xoff=2, yoff=-3)[0][0] == (128, -192, True)
    circle = outline.bitmap_contours(['1'], 'circle', 1.0, 64)
    assert len(circle) == 1 and any(not on for _x, _y, on in circle[0])
    with pytest.raises(ValueError):
        outline.bitmap_contours(['1'], 'triangle')
    with pytest.raises(ValueError, match='ppu'):
        outline.bitmap_contours(['1'], ppu=1.5)
    cs = outline.bitmap_contours(['101', '111'], ppu=4)
    assert outline.rasterize(cs, 4, 0, 0, 3, 2) == ['101', '111']
    assert native.rasterize(cs, 4.0, 0, 0, 3, 2).bindata == ['101', '111']
    assert outline.K == pytest.approx(1 / 0.9238795325112867) and outline.D == pytest.approx(0.5**0.5)


def test_ttf_and_svg_goldens_match_rust():
    f = mini()
    assert f.tobdf() == MINI
    assert f.tosvgfont() == (DATA / 'mini.svg').read_text()
    for rec in json.loads((DATA / 'ttf.json').read_text()):
        o = rec['options']
        data = f.tottf(shape=o['shape'], dotsize=o['dotsize'], metadata=o['metadata'])
        assert data == (DATA / f"{rec['name']}.ttf").read_bytes(), rec['name']
        assert ttf.write_ttf(f, shape=o['shape'], dotsize=o['dotsize'], metadata=o['metadata']) == data
        back = ttf.read_ttf(data, pixelsize=8)
        assert back.tobdf() == (DATA / f"{rec['name']}.bdf").read_text(), rec['name']
    assert native.write_ttf(f._raw, {'ppu': 64}) == f.tottf()
    assert native.read_ttf(f.tottf(), None).tobdf() == MINI
    with pytest.raises(ValueError, match='ppu'):
        f.tottf(ppu=3)
    with pytest.raises(ValueError, match='pixelsize'):
        ttf.read_ttf(f.tottf(), pixelsize=True)
    with pytest.raises(ValueError):
        ttf.read_ttf(b'\x00\x01\x00\x00' + b'\x00' * 8)


# ---------------------------------------------------------------------------
# Font: loading


def test_load_paths(tmp_path):
    p = tmp_path / 'mini.bdf'
    p.write_text(MINI)
    assert Font(str(p)).tobdf() == MINI
    assert Font(p).tobdf() == MINI
    assert Font(MINI.encode()).tobdf() == MINI
    assert Font(gzip.compress(MINI.encode())).tobdf() == MINI
    assert Font(io.BytesIO(MINI.encode())).tobdf() == MINI  # Rust bounded reader
    assert Font(io.StringIO(MINI)).tobdf() == MINI
    assert Font().load_lines(MINI.splitlines(True)).tobdf() == MINI
    assert Font().load_bytes(mini().tottf(), pixelsize=8).length() == 3
    raw = native.RawFont()
    raw.load_ttf(mini().tottf(), None)
    assert raw.tobdf() == MINI
    raw.load_file_obj(io.BytesIO(MINI.encode()))
    raw.load_lines(MINI.splitlines())
    assert raw.tobdf() == MINI
    with pytest.raises(TypeError):
        Font(123)
    with pytest.raises(OSError) as exc:
        Font(str(tmp_path / 'missing.bdf'))
    assert isinstance(exc.value, fp.FontpixelIoError) and exc.value.kind == 'Io'


def test_parse_error_translation():
    with pytest.raises(BDFParseError) as exc:
        Font().load_string('STARTFONT 2.1\nFONT x\n')
    e = exc.value
    assert isinstance(e, ValueError) and isinstance(e, fp.BdfParseError)
    assert e.lineno == e.line and e.kind == 'BdfParse'
    assert str(e) == f'line {e.lineno}: {e.msg}'
    assert BDFParseError('boom').lineno is None


def test_parse_warnings_are_bdfparser_warnings():
    text = MINI.replace('ENDPROPERTIES\n', 'ENDPROPERTIES\nFOO 1\n')
    with pytest.warns(BDFParserWarning, match="unexpected line 'FOO 1' ignored"):
        f = Font().load_string(text)
    report = f._raw.report()
    assert any(e['code'] == 'bdf.parse.unexpected_line' for e in report['entries'])
    with warnings.catch_warnings():
        warnings.simplefilter('error')
        mini()  # info-level native notes are not warnings


def test_create_and_core_round_trip():
    f = Font.create(pixelsize=12, ascent=10, descent=3, fontname='-x-Foo', familyname='Foo', xres=96, yres=72)
    h = f.headers
    assert (h['fontname'], h['pointsize'], h['xres'], h['yres']) == ('-x-Foo', 12, 96, 72)
    assert (h['fbbx'], h['fbby'], h['fbbxoff'], h['fbbyoff']) == (12, 13, 0, -3)
    assert f.props['family_name'] == 'Foo'
    with pytest.raises(ValueError):
        native.RawFont.create({'pixelsize': 8, 'bogus': 1})
    core = mini()._raw.core()
    assert isinstance(core, fp.Font) and len(core) == 3
    back = native.RawFont.from_core(core)
    assert back.tobdf() == MINI


# ---------------------------------------------------------------------------
# Font: data views, properties, charsets


def test_headers_props_views_write_through():
    f = mini()
    h = f.headers
    assert h['fbbx'] == 6 and dict(h) == f._raw.headers()
    f.headers['fbbx'] = 9
    assert h['fbbx'] == 9 and 'FONTBOUNDINGBOX 9 8 0 -1' in f.tobdf()
    del f.headers['comment']
    assert 'COMMENT "mini' not in f.tobdf()
    assert f.getprop('Pixel_Size') == 8 and f.getprop('nope') is None
    assert f.setprop('MinSpace', 2) is f and f._propnames['minspace'] == 'MinSpace'
    f.props['lower_key'] = 1
    assert '\nLOWER_KEY 1\n' in f.tobdf()
    assert f.delprop('minspace') is f and 'minspace' not in f.props
    assert f.swidth(6) == 720


def test_glyphs_and_unencoded_views():
    f = mini()
    assert list(f.glyphs) == [32, 65, 66] and len(f) == f.length() == 3
    assert f.glyphs[65][0] == 'A' and f.glyphs[65] is f.glyphs[65]
    f.glyphs[65][4] = 1  # item assignment is written back
    assert f.glyphbycp(65).meta['bbxoff'] == 1
    f.glyphs[70] = ['F', 70, 1, 1, 0, 0, None, None, 6, 0] + [None] * 6 + [['80']]
    assert f.glyphs[70][16] == ['80']
    del f.glyphs[70]
    assert 70 not in f.glyphs
    f.unencoded.append({'altcode': 42, 'meta': ['u', -1, 0, 0, 0, 0] + [None] * 10 + [[]]})
    assert f.unencoded == [{'altcode': 42, 'meta': ['u', -1, 0, 0, 0, 0] + [None] * 10 + [[]]}]
    assert 'ENCODING -1 42' in f.tobdf()
    model = f._raw.model()
    assert [cp for cp, _ in model['glyphs']] == [32, 65, 66] and model['unencoded'][0][0] == 42
    f.unencoded.clear()
    assert f.unencoded == [] and 'ENCODING -1' not in f.tobdf()


def test_charset_encoding_and_converters():
    f = mini()
    assert f.charset == 'iso10646'
    assert f.char2cp('A') == 65 and f.cp2char(65) == 'A'
    assert f._raw.unicode2cp(0x41) == 65 and f._raw.cp2unicode(0xD800) == 0xD800
    assert f._raw.cp2char(0xD800) is None
    with pytest.raises(TypeError):
        f.char2cp('AB')
    f.encoding(char2cp=lambda c: 65 if c == 'a' else None, cp2char=lambda cp: 'a' if cp == 65 else None)
    assert f.glyph('a').cp() == 65 and f.cp2char(65) == 'a'
    assert f.clone().char2cp('a') == 65
    assert f.lacksglyphs('ab') == ['b']
    with pytest.warns(BDFParserWarning, match='could not be converted'):
        f.charsetconvert('iso10646')
    assert f.char2cp('a') == 97 and f._char2cp_override is None

    def boom(c):
        raise RuntimeError('converter failed')

    g = mini().encoding(char2cp=boom)
    with pytest.raises(RuntimeError, match='converter failed'):
        g.draw('A')
    gb = mini()
    gb.encoding(charset='GB2312')
    assert gb.charset == 'gb2312'
    gb.encoding(charset='')
    assert gb.charset == 'iso10646'
    with pytest.warns(BDFParserWarning, match="Unknown charset 'weird'"):
        mini().charsetconvert('weird')


def test_encoding_module_uses_native_tables():
    assert encoding.canonical_charset('Shift-JIS') == 'shiftjis' == fp.canonical_charset('Shift-JIS')
    assert encoding.sanitize_charset('GB2312.1980', '0') == 'gb2312'
    assert encoding.charset_props('koi8r') == ('KOI8', 'R')
    assert encoding.is_unicode_charset('utf16le')
    assert encoding.charset_codec('gb2312') == encoding.CharsetCodec('gb2312', 'gb2312', False)
    assert encoding.char2cp('啊', 'gb2312') == 0xB0A1 and encoding.cp2char(0xB0A1, 'GB2312') == '啊'
    assert encoding.charset_table()['gbk']['codec'] == 'gbk'
    assert encoding.codec_name('hz') is None and encoding.charset_codec('hz').known is False
    assert len(encoding.charset_map('cp1252')) == 251
    assert encoding.gb18030_ranges()[-1] == (encoding.gb18030_ranges()[-1][0], 0x10000, 0x100000)
    with pytest.raises(TypeError):
        encoding.char2cp('ab', 'utf8')


# ---------------------------------------------------------------------------
# Font: querying, editing, drawing, output


def test_query_and_glyph_handles():
    f = mini()
    assert list(f.itercps()) == [32, 65, 66] and list(f.itercps(2, (33, 70))) == [66, 65]
    assert list(f.itercps(-1)) == [66, 65, 32]
    assert [g.cp() for g in f.iterglyphs(1, [(65, 65)])] == [65]
    with pytest.warns(BDFParserWarning, match=r'^Glyph "€" \(codepoint 8364\) does not exist'):
        assert f.glyphbycp(0x20AC) is None
    g = f.glyphbycp(65)
    assert g._attached and g.chr() == 'A' and g.cp() == 65
    assert g.origin() == (0, 1) and g.origin(1, True) == (0, 0)
    assert str(g).splitlines()[0] == '..#...'
    assert g.draw(1).bindata[0] == '00100' and g.draw(2).bindata[0] == '00100000'
    assert g.draw(-1, (2, 2, 0, 0)).bindata == ['10', '10']
    g.shift(1, 2)
    assert f.glyphs[65][4:6] == [1, 2]
    g.meta['dwx0'] = 9  # direct meta edit, written with the next editing method
    g.setmeta(glyphname='A2')
    assert f.glyphs[65][0] == 'A2' and f.glyphs[65][8] == 9
    with pytest.raises(KeyError, match='bogus'):
        g.setmeta(bogus=1)
    c = g.clone()
    assert not c._attached and c.font is f
    c.shift(5)
    assert f.glyphs[65][4] == 1
    g.setbitmap(Bitmap(['11']), mode=1)
    assert f.glyphs[65][2:4] == [2, 1]
    g.transform(lambda b: b.invert())
    raw = native.RawGlyph({'codepoint': 1, 'hexdata': ['80'], 'bbw': 1, 'bbh': 1}, f._raw)
    assert not raw.is_attached() and raw.meta()[16] == ['80'] and raw.cp() == 1
    assert raw.font().length() == f.length() and raw.clone().meta() == raw.meta()


def test_add_remove_new_glyphs():
    f = mini()
    g = Glyph({'glyphname': 'F', 'codepoint': 70, 'bbw': 1, 'bbh': 1, 'hexdata': ['80']}, f)
    assert not g._attached
    f.addglyph(g)
    assert g._attached and f.glyphs[70][0] == 'F'
    f.removeglyph(g)
    assert 70 not in f.glyphs and not g._attached
    f.addglyph({'codepoint': 71, 'hexdata': ['80']})
    assert f.glyphs[71][0] == 'uni0047'
    with pytest.warns(BDFParserWarning, match='already exists'):
        f.addglyph({'codepoint': 71, 'glyphname': 'x'}, replace=False)
    f.removeglyph(71).removeglyph(9999)
    n = f.newglyph(72, Bitmap(['11', '01']), mode=1, name='H')
    assert n._attached and f.glyphs[72][:4] == ['H', 72, 2, 2] and f.glyphs[72][8] == 2
    h = f.headers
    assert f.updatebbox() is f
    assert (h['fbbx'], h['fbby'], h['fbbxoff'], h['fbbyoff']) == (5, 7, 0, 0)
    with pytest.raises(KeyError):
        f.addglyph({'codepoint': 1, 'bogus': 2})


def test_clone_subset_merge_transform_charsetconvert():
    f = mini()
    c = f.clone()
    c.glyphbycp(65).shift(1)
    assert f.glyphs[65][4] == 0 and c.glyphs[65][4] == 1
    s = f.subset((65, 66))
    assert list(s.glyphs) == [65, 66] and len(f) == 3
    m = Font.create(8)
    m.merge(f)
    assert list(m.glyphs) == [32, 65, 66]
    calls = []
    f.transform(lambda b: calls.append(b.width()), r=(65, 66))
    assert calls == [6, 6]

    def fail(b):
        raise RuntimeError('stop')

    with pytest.raises(RuntimeError, match='stop'):
        f.transform(fail)
    gb = mini()
    gb.charsetconvert('ISO8859-1')
    assert gb.props['charset_registry'] == 'ISO8859' and list(gb.glyphs) == [32, 65, 66]


def test_drawing():
    f = mini()
    assert f.draw('AB').bindata[3] == '111110011100'
    assert f.drawcps([65, None]).width() == 12  # a missing code draws a blank cell
    assert f.draw('A', direction='tb').height() == 8
    assert f.drawall().width() == 18
    assert f.drawall(r=(65, 65)).width() == 6
    assert f.draw('Z', missing={'codepoint': -1, 'bbw': 1, 'bbh': 1, 'hexdata': ['80'], 'dwx0': 2}).width() == 6
    assert f._raw.drawcps([65], {'mode': 0}).width() == 6
    assert f._raw.draw('A', None).bindata == f.draw('A').bindata
    assert f._raw.drawall({'mode': 0}).bindata == f.drawall().bindata
    with pytest.raises(ValueError):
        f.draw('AAAA', linelimit=3)


def test_output_and_save(tmp_path):
    f = mini()
    data = f.topcf()
    assert data[:4] == b'\x01fcp' and pcf.write_pcf(f) == data
    assert pcf.read_pcf(data).tobdf().startswith('STARTFONT')
    assert pcf.read_pcf(gzip.compress(data)).length() == 3
    assert f.topcf(byteorder='lsb', bitorder='lsb', glyphpad=1, scanunit=1) != data
    with pytest.raises(ValueError, match='glyphpad'):
        f.topcf(glyphpad=3)
    with pytest.raises(pcf.PCFError, match='magic'):
        pcf.read_pcf(b'not a pcf')
    with pytest.raises(pcf.PCFError):
        pcf.read_pcf(b'\x01fcp\xff\xff\xff\xff')
    for ext in ('bdf', 'pcf', 'ttf', 'svg'):
        p = tmp_path / f'out.{ext}'
        assert f.save(p) is f
        assert p.read_bytes()[:4] in (b'STAR', b'\x01fcp', b'\x00\x01\x00\x00', b'<?xm', b'<svg')
    assert (tmp_path / 'out.bdf').read_text() == MINI
    f.save(tmp_path / 'opts.svg', unicodeformat='decncr')
    assert 'unicode="&#65;"' in (tmp_path / 'opts.svg').read_text()
    with pytest.raises(ValueError, match='Unknown output format'):
        f.save(tmp_path / 'x.png')
    out = io.BytesIO()
    f._raw.save_to(out, 'bdf', None)
    assert out.getvalue() == MINI.encode()
    f._raw.save(str(tmp_path / 'raw.pcf'), None, None)
    assert (tmp_path / 'raw.pcf').read_bytes() == data


def test_cli(tmp_path, capsys):
    from fontpixel.compat.bdfparser.cli import main

    p = tmp_path / 'mini.bdf'
    p.write_text(MINI)
    assert main(['info', str(p)]) == 0
    assert 'glyphs: 3 (+ 0 unencoded)' in capsys.readouterr().out
    assert main(['convert', str(p), str(tmp_path / 'o.ttf'), '--shape', 'circle']) == 0
    assert main(['preview', str(p), 'A']) == 0
    assert capsys.readouterr().out == str(mini().draw('A')) + '\n'
    assert main(['info', str(tmp_path / 'missing.bdf')]) == 1
