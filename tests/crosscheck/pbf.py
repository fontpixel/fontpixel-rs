#!/usr/bin/env python3
"""PBF native import/export against independent wire fixtures and the MIT catalogue.

Needs Pillow and the supplied reference/fontpixel catalogue. All generated files
remain in --output-dir. Optional --source-dir reads licensed upstream fonts in
place; provenance is copied from its sources.json into the result report.
"""
import argparse
import hashlib
import itertools
import json
import struct
import subprocess
import sys
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT.parent / "reference/fontpixel/pipeline"))
from opf.ingest.pebble import convert_pbf, read_pbf


def fixture(version, rle=False, cp_bytes=2, off_bytes=2):
    # Two independent glyph records plus an alias of the first. Width nine
    # deliberately crosses byte boundaries and the LSB stream crosses rows.
    entries = [(65, 0), (66, 0), (0x1F600 if cp_bytes == 4 else 67, 1)]
    images = [(-2, 3, 10, [[int(v) for v in row] for row in ["101001011", "001110010", "111000100"]]),
              (1, -2, -3, [[0] * 7 for _ in range(2)])]
    data = bytearray(4)
    offsets = []
    for left, top, advance, rows in images:
        offsets.append(len(data))
        width, height = len(rows[0]), len(rows)
        pixels = list(itertools.chain.from_iterable(rows))
        packed = bytearray()
        if rle:
            units = []
            for colour, group in itertools.groupby(pixels):
                remaining = len(list(group))
                while remaining:
                    run = min(8, remaining)
                    units.append((colour << 3) | (run - 1))
                    remaining -= run
            height = len(units)
            for i in range(0, len(units), 2):
                packed.append(units[i] | ((units[i + 1] if i + 1 < len(units) else 0) << 4))
        else:
            for i in range(0, len(pixels), 8):
                packed.append(sum(v << j for j, v in enumerate(pixels[i:i + 8])))
        if version == 1:
            data.extend(struct.pack("<BBbb3xb", width, height, left, top, advance))
        else:
            data.extend(struct.pack("<BBbbb", width, height, left, top, advance))
        data.extend(packed)
        while version == 1 and len(data) % 4:
            data.append(0)
    header = struct.pack("<BBHH", version, 8, len(entries), 65)
    if version > 1:
        header += bytes([1, cp_bytes])
    if version == 3:
        header += bytes([10, int(off_bytes == 2) | (int(rle) << 1)])
    if version > 1:
        header += struct.pack("<BBH", 0, len(entries), 0)
    table = b"".join(cp.to_bytes(cp_bytes, "little") + (offsets[index] // (4 if version == 1 else 1)).to_bytes(off_bytes, "little") for cp, index in entries)
    return header + table + data, {cp: images[index] for cp, index in entries}


def wire(path):
    """Small independent runtime-layout decoder, including v1 word offsets."""
    b = path.read_bytes()
    version, baseline, count, wildcard = struct.unpack_from("<BBHH", b)
    if version == 1:
        size, buckets, cp_bytes, off_bytes, compressed = 6, 0, 2, 2, False
    else:
        buckets, cp_bytes = b[6:8]
        size, off_bytes, compressed = (8, 4, False) if version == 2 else (b[8], 2 if b[9] & 1 else 4, bool(b[9] & 2))
    entry = cp_bytes + off_bytes
    start = size + buckets * 4
    base = start + count * entry
    glyphs = {}
    ranges = [(0, count)] if not buckets else [(struct.unpack_from("<H", b, size + i * 4 + 2)[0], b[size + i * 4 + 1]) for i in range(buckets)]
    for offset, length in ranges:
        previous = -1
        for i in range(length):
            p = start + offset + i * entry
            cp = int.from_bytes(b[p:p + cp_bytes], "little")
            assert cp > previous
            previous = cp
            relative = int.from_bytes(b[p + cp_bytes:p + entry], "little")
            assert relative > 0, "writer must reserve runtime null glyph offset"
            p = base + relative * (4 if version == 1 else 1)
            if version == 1:
                w, h, left, top, advance = struct.unpack_from("<BBbb3xb", b, p)
                p += 8
            else:
                w, h, left, top, advance = struct.unpack_from("<BBbbb", b, p)
                p += 5
            if compressed:
                pixels = []
                for j in range(h):
                    unit = (b[p + j // 2] >> ((j % 2) * 4)) & 15
                    pixels += [unit >> 3] * ((unit & 7) + 1)
                assert not pixels or (w and len(pixels) % w == 0)
                rows = [pixels[j:j + w] for j in range(0, len(pixels), w)] if w else []
            else:
                rows = [[(b[p + (y * w + x) // 8] >> ((y * w + x) % 8)) & 1 for x in range(w)] for y in range(h)]
            assert cp not in glyphs
            glyphs[cp] = (left, top, advance, rows)
    assert len(glyphs) == count
    return baseline, wildcard, glyphs


def atlas(path):
    mapping = json.loads(path.with_suffix(".json").read_text())
    pixels = Image.open(path).convert("L")
    result = {}
    for cell in mapping["cells"]:
        x, y, w, h = cell["rect"]
        glyph = cell["native"]
        assert not glyph["gray"]
        metrics = glyph["metrics"]
        rows = [[int(pixels.getpixel((x + px, y + py)) == 255) for px in range(w)] for py in range(h)]
        bounds = metrics["bounds"]
        assert (bounds["width"], bounds["height"]) == (w, h)
        for code in glyph["codepoints"]:
            cp = code["Unicode"]
            assert cp not in result
            result[cp] = (bounds["x"], bounds["y"], metrics["advance"]["x"], rows)
    return mapping["native"]["metrics"], result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--source-dir", type=Path)
    args = parser.parse_args()
    dest = args.output_dir.resolve()
    dest.mkdir(parents=True, exist_ok=False)
    binary = str(args.binary.resolve())
    report = {"checks": [], "artifacts": {}}

    def command(*argv):
        r = subprocess.run([binary, *map(str, argv)], capture_output=True)
        assert r.returncode == 0, (argv, r.stderr.decode())
        return r

    for version, rle, cp_bytes, off_bytes in [(1, False, 2, 2), (2, False, 2, 4), (2, False, 4, 4), (3, False, 2, 2), (3, False, 4, 4), (3, True, 2, 2), (3, True, 4, 4)]:
        stem = f"v{version}-{'rle' if rle else 'raw'}-{cp_bytes}-{off_bytes}"
        source = dest / f"{stem}.pbf"
        data, expected = fixture(version, rle, cp_bytes, off_bytes)
        source.write_bytes(data)
        assert wire(source) == (8, 65, expected)
        png = dest / f"{stem}.png"
        command("convert", source, "-o", png, "--to", "image", "--strict")
        metrics, actual = atlas(png)
        wanted = {cp: (left, 8 - top - len(rows), advance, rows) for cp, (left, top, advance, rows) in expected.items()}
        assert actual == wanted
        assert metrics["ascent"] == 8 and metrics["default_character"] == {"Unicode": 65}
        for out_version, out_rle in [(1, False), (2, False), (3, False), (3, True)]:
            if cp_bytes == 4 and out_version == 1:
                continue
            output = dest / f"{stem}-out{out_version}-{'rle' if out_rle else 'raw'}.pbf"
            command("convert", png, "-o", output, "--strict", "--pbf-version", str(out_version), "--pbf-compression", "rle4" if out_rle else "raw")
            assert wire(output) == (8, 65, expected)
            if out_version == 3:
                assert read_pbf(output) == (8, expected)
        report["checks"].append({"fixture": stem, "mappings": len(expected), "glyphs": 2})

    if args.source_dir:
        for source in sorted(args.source_dir.glob("*.pbf")):
            expected = convert_pbf(source, "tumbled", "TUMBLED")
            png = dest / f"{source.stem}-trim.png"
            converted = command("convert", source, "-o", png, "--to", "image", "--pbf-trim", "--report", "json")
            diagnostics = json.loads(converted.stdout)["report"]["entries"]
            tail_losses = sum(e["code"] == "pbf.read.rle_tail" and e["impact"] == "loss" for e in diagnostics)
            metrics, actual = atlas(png)
            wanted = {g.cp: (g.bbx, g.bby, g.dwidth, [[int(bool(g.rows[y * ((g.bbw + 7) // 8) + x // 8] & (128 >> (x % 8)))) for x in range(g.bbw)] for y in range(g.bbh)]) for g in expected.glyphs}
            assert actual == wanted, source.name
            assert (metrics["ascent"], metrics["descent"], metrics["pixel_size"]) == (expected.ascent, expected.descent, expected.pixel_size)
            report["checks"].append({"oracle": "MIT catalogue convert_pbf", "file": source.name, "mappings": len(wanted), "sha256": hashlib.sha256(source.read_bytes()).hexdigest(), "rle_tail_losses": tail_losses})
        manifest = args.source_dir / "sources.json"
        if manifest.exists():
            report["upstream"] = json.loads(manifest.read_text())
    for path in sorted(dest.iterdir()):
        if path.is_file():
            report["artifacts"][path.name] = {"bytes": path.stat().st_size, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
    (dest / "results.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"checks": len(report["checks"]), "results": str(dest / "results.json")}, indent=2))

if __name__ == "__main__":
    main()
