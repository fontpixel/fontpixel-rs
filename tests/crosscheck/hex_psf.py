#!/usr/bin/env python3
"""W3.1 independent wire, bidirectional monobit and catalogue BDF-boundary checks.

No generated file or Python bytecode is written under reference/ or samples/.
"""
import argparse
import hashlib
import importlib.metadata
import json
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import warnings

sys.dont_write_bytecode = True
import monobit

ROOT = Path(__file__).resolve().parents[2]
REFERENCE = ROOT.parent / "reference/fontpixel/pipeline"
sys.path.insert(0, str(REFERENCE))
from opf.parsers.bdf import parse_bdf


def convert(binary, source, target, *options):
    p = subprocess.run([binary, "convert", str(source), "-o", str(target), "--report", "json", *options], capture_output=True)
    if p.returncode:
        raise RuntimeError(f"{source.name} -> {target.name}: {p.stderr.decode()}")
    return json.loads(p.stdout)


def signature(font):
    return [(g.as_matrix(), g.advance_width, g.left_bearing, g.shift_up, tuple(c.value for c in g.get_labels() if isinstance(c, monobit.Char))) for g in font.glyphs]


def wire_psf(data):
    """Independent kbd wire decoder; do not reuse Rust's implementation."""
    if data[:2] == b"\x36\x04":
        mode, height = data[2:4]
        count, width, offset = (512 if mode & 1 else 256), 8, 4
        size, unit, table = height, 2, bool(mode & 6)
    else:
        magic, version, offset, flags, count, size, height, width = struct.unpack_from("<8I", data)
        assert magic == 0x864AB572 and version == 0
        unit, table = 1, bool(flags & 1)
    assert size == height * ((width + 7) // 8)
    rows = [data[offset + i * size: offset + (i + 1) * size] for i in range(count)]
    position = offset + count * size
    labels = []
    if table:
        for _ in range(count):
            aliases, sequences = [], []
            current = None
            while True:
                if unit == 2:
                    cp = struct.unpack_from("<H", data, position)[0]
                    position += 2
                    marker = cp if cp >= 0xFFFE else None
                else:
                    byte = data[position]
                    if byte in (0xFE, 0xFF):
                        position += 1
                        marker, cp = 0xFF00 + byte, None
                    else:
                        marker = None
                        n = 1 if byte < 0x80 else 2 if byte < 0xE0 else 3 if byte < 0xF0 else 4
                        cp = ord(data[position:position + n].decode("utf-8"))
                        position += n
                if marker is not None:
                    if current is not None:
                        assert current
                        sequences.append(tuple(current))
                    if marker == 0xFFFF:
                        break
                    current = []
                elif current is None:
                    aliases.append(cp)
                else:
                    current.append(cp)
            labels.append((tuple(aliases), tuple(sequences)))
    else:
        labels = [((i,), ()) for i in range(count)]
    assert position == len(data)
    return width, height, rows, labels, table


def make_psf2(width, height, count, table):
    size = ((width + 7) // 8) * height
    rows = bytes(((i * 73 + 16) & 0xFF) for i in range(size * count))
    if width % 8:
        rows = bytearray(rows)
        for i in range((width + 7) // 8 - 1, len(rows), (width + 7) // 8):
            rows[i] &= 0xFF << (8 - width % 8)
        rows = bytes(rows)
    return struct.pack("<8I", 0x864AB572, 0, 32, 1, count, size, height, width) + rows + table


def aligned_rows(g, width, height, bottom):
    stride = (width + 7) // 8
    target = bytearray(stride * height)
    srcstride = (g.bbw + 7) // 8
    for sy in range(g.bbh):
        y = g.bby + g.bbh - sy - 1
        ty = bottom + height - y - 1
        for sx in range(g.bbw):
            if g.rows[sy * srcstride + sx // 8] & (0x80 >> (sx % 8)):
                tx = g.bbx + sx
                assert 0 <= ty < height and 0 <= tx < width
                target[ty * stride + tx // 8] |= 0x80 >> (tx % 8)
    return bytes(target)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--binary", default=str(ROOT / "target/release/fontpixel"))
    p.add_argument("--output", type=Path)
    args = p.parse_args()
    warnings.simplefilter("ignore")
    checks, limitations = [], []
    with tempfile.TemporaryDirectory(prefix="fontpixel-w31-") as directory:
        tmp = Path(directory)
        # HEX: both directions through independently installed monobit.
        source = tmp / "source.hex"
        source.write_text("0041:" + "A55A" * 8 + "\n6C38:" + "AA5555AA" * 8 + "\n1F600:" + "81" * 16 + "\n")
        expected = monobit.load(source, format="unifont")[0]
        rust = tmp / "rust.hex"
        convert(args.binary, source, rust, "--strict")
        assert signature(monobit.load(rust, format="unifont")[0]) == signature(expected)
        external = tmp / "external.hex"
        monobit.save(expected, external, format="unifont", overwrite=True)
        convert(args.binary, external, rust, "--strict")
        assert signature(monobit.load(rust, format="unifont")[0]) == signature(expected)
        checks.append({"case": "HEX standard 8/16 widths plus supplementary Unicode", "result": "bidirectional pixels/metrics/labels pass"})
        # All four widths use an independent specification fixture, not monobit's heuristic.
        wide = tmp / "wide.hex"
        wire = "".join(f"{65+i:04X}:" + "A5" * (w // 8 * 16) + "\n" for i, w in enumerate((8, 16, 24, 32)))
        wide.write_text(wire)
        convert(args.binary, wide, rust, "--strict")
        assert rust.read_text() == wire
        wide_mono = monobit.load(wide, format="unifont")[0]
        observed = [(g.width, g.height) for g in wide_mono.glyphs]
        assert observed != [(8, 16), (16, 16), (24, 16), (32, 16)]
        limitations.append({"oracle": "monobit", "case": "24/32-wide HEX", "observed_dimensions": observed, "expected": [[8,16],[16,16],[24,16],[32,16]], "coverage": "independent byte equality and Rust tests"})
        checks.append({"case": "HEX 8/16/24/32 extensions", "result": "exact canonical bytes pass"})
        for version, count, width in [(1, 256, 8), (1, 512, 8), (2, 7, 9), (2, 4, 24)]:
            glyphs = tuple(monobit.Glyph.from_bytes(bytes([(i * 37) & 255, 0, 0x80, 0] * ((width + 7) // 8)), width=width, height=4, char=chr(0x20+i), codepoint=i) for i in range(count))
            font = monobit.Font(glyphs, encoding="unicode")
            ext = tmp / f"mono-v{version}-{count}.psf"
            monobit.save(font, ext, format="psf", version=version, count=count, raw=True, overwrite=True)
            target = tmp / f"rust-v{version}-{count}.psf"
            options = ["--psf-version",str(version),"--strict"]
            convert(args.binary, ext, target, *options)
            assert wire_psf(target.read_bytes()) == wire_psf(ext.read_bytes())
            assert signature(monobit.load(target, format="psf")[0]) == signature(monobit.load(ext, format="psf")[0])
            # Read Rust-produced bytes and independently write them back with monobit.
            back = tmp / f"back-v{version}-{count}.psf"
            monobit.save(monobit.load(target, format="psf")[0], back, format="psf", version=version, count=count, raw=True, overwrite=True)
            convert(args.binary, back, target, *options)
            assert wire_psf(back.read_bytes()) == wire_psf(target.read_bytes())
            checks.append({"case": f"PSF{version} {width}x4 {count} positions", "result": "bidirectional wire/pixels/metrics/labels pass"})
        # Standards fixtures deliberately exercise aliases, sequences, empty entries.
        table = b"A\xc3\x85\xfeA\xcc\x8a\xfef\xef\xb8\x8f\xff\xfe\xf0\x9f\x98\x80\xe2\x80\x8d\xf0\x9f\x98\x81\xff\xff"
        sequences = tmp / "sequences.psf"
        sequences.write_bytes(make_psf2(9, 3, 3, table))
        target = tmp / "sequence-back.psf"
        convert(args.binary, sequences, target, "--strict")
        assert target.read_bytes() == sequences.read_bytes()
        assert wire_psf(target.read_bytes())[3] == [((65,197),((65,0x30A),(102,0xFE0F))), ((),((0x1F600,0x200D,0x1F601),)), ((),())]
        expected = wire_psf(sequences.read_bytes())[3]
        try:
            monobit.load(sequences, format="psf")
        except UnicodeDecodeError as exc:
            limitations.append({"oracle":"monobit", "case":"PSF2 sequence input", "error":str(exc), "coverage":"independent kbd grammar roundtrip"})
        else:
            raise AssertionError("monobit sequence limitation changed; reassess oracle coverage")
        mono = monobit.Font((monobit.Glyph.from_bytes(bytes([0x80,0]),width=8,height=2,char="A\u030a",codepoint=0),),encoding="unicode")
        monobit.save(mono, tmp / "mono-sequences.psf", format="psf", count=0, raw=True, overwrite=True)
        observed = wire_psf((tmp / "mono-sequences.psf").read_bytes())[3]
        expected = [((), ((65,0x30A),))]
        assert observed != expected
        limitations.append({"oracle":"monobit", "case":"PSF2 sequence output", "expected_mappings":expected, "rewritten_mappings":observed, "coverage":"independent kbd grammar roundtrip"})
        first = [0xC5, 0x212B, 0xFFFE, 0x41, 0x30A, 0xFFFF]
        raw = bytes([0x36,4,2,2]) + bytes(512) + struct.pack("<6H",*first) + b"\xff\xff"*255
        source = tmp / "psf1-sequence.psf"; source.write_bytes(raw)
        convert(args.binary, source, target, "--psf-version","1","--strict")
        assert wire_psf(target.read_bytes()) == wire_psf(raw)
        mono = monobit.load(source, format="psf")[0]
        chars = tuple(c.value for c in mono.glyphs[0].get_labels() if isinstance(c, monobit.Char))
        assert "A\u030a" not in chars
        limitations.append({"oracle":"monobit", "case":"PSF1 LE FFFE marker", "observed_labels":chars, "expected_sequence":"A\u030a", "coverage":"independent wire grammar"})
        checks.append({"case":"PSF1/PSF2 aliases, sequence-only, multiple sequences and empty entries", "result":"independent grammar and exact mapping roundtrip pass"})
        # PLAN 9.5: no direct importer exists. Exercise actual test_bdf.py's mini
        # contract through the new formats, comparing full-cell pixels explicitly.
        mini = REFERENCE / "tests/fixtures/mini.bdf"
        before = parse_bdf(mini,"mini")
        assert [g.cp for g in before.glyphs] == [65,27704]
        assert before.ascent == 14 and before.descent == 2
        assert before.glyphs[0].rows[0] == 0x30 and before.glyphs[0].rows[5] == 0xFC
        for fmt in ("hex","psf"):
            intermediate = tmp / f"mini.{fmt}"
            output = tmp / f"mini-{fmt}.bdf"
            opts = ["--hex-descent","2"] if fmt == "hex" else []
            convert(args.binary, mini, intermediate,*opts)
            convert(args.binary, intermediate, output,*opts)
            after = parse_bdf(output,"mini")
            assert [g.cp for g in after.glyphs] == [65,27704]
            for a,b in zip(before.glyphs,after.glyphs):
                width = a.dwidth if fmt == "hex" else 16
                assert (b.dwidth,b.bbw,b.bbh,b.bbx,b.bby)==(width,width,16,0,-2 if fmt=="hex" else 0)
                assert b.rows == aligned_rows(a,width,16,-2)
            checks.append({"case":f"catalogue test_bdf mini.bdf via {fmt}","result":"actual importer, codepoints, full-cell rows and normalized metrics pass"})
        manifest = (ROOT.parent / "reference/fontpixel/ingest/manifest.toml").read_text()
        assert "importer also has no .hex conversion" in manifest and "同名 .psf.gz" in manifest
    result = {"monobit_version":importlib.metadata.version("monobit"),"checks":checks,"oracle_limitations":limitations,"catalogue_direct_importers":"absent in supplied reference; explicit unsupported manifest entries verified","result":"pass"}
    if args.output: args.output.write_text(json.dumps(result,ensure_ascii=False,indent=2)+"\n")
    print(json.dumps(result,ensure_ascii=False,indent=2))

if __name__ == "__main__": main()
