#!/usr/bin/env python3
"""HBF specification fixtures, monobit reads/writes, and catalogue BDF boundary."""
import argparse
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path
import monobit

ROOT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT.parent / "reference/fontpixel/pipeline"))
from opf.parsers.bdf import parse_bdf


def header(scheme, byte2, byte3, ranges, count, default):
    lines = ["HBF_START_FONT 1.1", f"HBF_CODE_SCHEME {scheme}", "FONT SpecFixture",
             "SIZE 12 72 72", "HBF_BITMAP_BOUNDING_BOX 9 2 0 0", "FONTBOUNDINGBOX 9 2 0 0",
             "STARTPROPERTIES 4", f"DEFAULT_CHAR {default}", 'FAMILY_NAME "HBF fixture"',
             "FONT_ASCENT 2", "FONT_DESCENT 0", "ENDPROPERTIES", f"CHARS {count}"]
    for index, sequence in [(2, byte2), (3, byte3)]:
        if sequence:
            lines.append(f"HBF_START_BYTE_{index}_RANGES {len(sequence)}")
            lines.extend(f"HBF_BYTE_{index}_RANGE 0x{a:X}-0x{b:X}" for a, b in sequence)
            lines.append(f"HBF_END_BYTE_{index}_RANGES")
    lines.append(f"HBF_START_CODE_RANGES {len(ranges)}")
    lines.extend(f"HBF_CODE_RANGE 0x{a:X}-0x{b:X} {name} {offset}" for a, b, name, offset in ranges)
    return "\n".join(lines + ["HBF_END_CODE_RANGES", "HBF_END_FONT", ""])


def monobit_signature(font):
    return [(int.from_bytes(bytes(g.codepoint), "big"), g.width, g.height,
             g.left_bearing, g.shift_up, g.advance_width, g.as_bytes()) for g in font.glyphs]


def bdf_signature(path):
    return [(g.cp, g.bbw, g.bbh, g.bbx, g.bby, g.dwidth, g.rows)
            for g in parse_bdf(path, "hbf-crosscheck").glyphs]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--binary", required=True, type=Path)
    p.add_argument("--output-dir", required=True, type=Path)
    args = p.parse_args()
    dest = args.output_dir.resolve()
    dest.mkdir(parents=True, exist_ok=False)
    binary = str(args.binary.resolve())
    checks = []

    def command(*args):
        result = subprocess.run([binary, *map(str, args)], capture_output=True)
        assert result.returncode == 0, (args, result.stderr.decode())
        return result

    cases = [
        ("two-byte", "Vendor 2 bytes", [(0x40, 0x41), (0xFE, 0xFF)], [],
         [(0xA141, 0xA240, "a.bin", 3), (0xA241, 0xA241, "b.bin", 5)],
         [0xA141, 0xA1FE, 0xA1FF, 0xA240, 0xA241]),
        ("three-byte", "Vendor 3 bytes", [(2, 3)], [(4, 4), (7, 7)],
         [(0x10207, 0x10307, "a.bin", 3)], [0x10207, 0x10304, 0x10307]),
        ("unicode", "Unicode 1.1", [(0, 255)], [],
         [(0x4E00, 0x4E02, "a.bin", 3)], [0x4E00, 0x4E01, 0x4E02]),
    ]
    for name, scheme, b2, b3, ranges, codes in cases:
        folder = dest / name
        folder.mkdir()
        data = [bytes([(i * 37 + 128) & 255, 128, (i * 19 + 64) & 255, 0]) for i in range(len(codes))]
        cursor = 0
        for first, last, filename, offset in ranges:
            count = sum(first <= cp <= last for cp in codes)
            (folder / filename).write_bytes(b"\xff" * offset + b"".join(data[cursor:cursor + count]))
            cursor += count
        source = folder / "spec.hbf"
        source.write_text(header(scheme, b2, b3, ranges, len(codes), codes[0]))
        oracle = monobit.load(source, format="hbf")[0]
        expected = [(cp, 9, 2, 0, 0, 9, raw) for cp, raw in zip(codes, data)]
        assert monobit_signature(oracle) == expected
        bdf = folder / "rust.bdf"
        command("convert", source, "-o", bdf)
        assert monobit_signature(monobit.load(bdf, format="bdf")[0]) == expected
        if name == "unicode":
            assert bdf_signature(bdf) == expected
        else:
            # The catalogue only imports Unicode-mappable glyphs. Unknown vendor
            # encodings deliberately remain Legacy in Rust and in the BDF wire.
            assert bdf_signature(bdf) == []
        native = folder / "native.hbf"
        command("convert", source, "-o", native, "--strict")
        assert monobit_signature(monobit.load(native, format="hbf")[0]) == expected
        restored = folder / "restored.bdf"
        command("convert", native, "-o", restored)
        assert restored.read_bytes() == bdf.read_bytes()
        checks.append({"case": name, "glyphs": len(codes), "checks": ["spec bytes", "monobit read", "monobit BDF boundary", "native strict bundle"]})

        if name == "unicode":
            external = folder / "monobit.hbf"
            previous_cwd = Path.cwd()
            try:
                os.chdir(folder)
                monobit.save(oracle, Path("monobit.hbf"), format="hbf")
            finally:
                os.chdir(previous_cwd)
            # monobit 0.54 writes the end of code ranges but omits the mandatory
            # HBF_END_FONT. Preserve the raw artifact and add only that terminator.
            original = external.read_text()
            repairs = []
            (folder / "monobit-unrepaired.hbf.txt").write_text(original)
            normalized = []
            for line in original.splitlines():
                if line.startswith("HBF_CODE_RANGE "):
                    tokens = line.split()
                    bitmap_path = Path(tokens[-2])
                    if bitmap_path.is_absolute():
                        assert bitmap_path.parent == folder
                        assert bitmap_path.is_file()
                        tokens[-2] = bitmap_path.name
                        line = " ".join(tokens)
                        repairs.append("replace monobit absolute bitmap path with same-directory basename")
                normalized.append(line)
            if "HBF_END_FONT" not in original:
                normalized.append("HBF_END_FONT")
                repairs.append("append mandatory HBF_END_FONT omitted by monobit 0.54")
            external.write_text("\n".join(normalized) + "\n")
            external_expected = monobit_signature(monobit.load(external, format="hbf")[0])
            imported = folder / "external.bdf"
            command("convert", external, "-o", imported)
            assert bdf_signature(imported) == external_expected
            checks.append({"case": "monobit writer to Rust", "glyphs": len(external_expected), "repairs": repairs})
    hashes = {str(p.relative_to(dest)): {"bytes": p.stat().st_size, "sha256": hashlib.sha256(p.read_bytes()).hexdigest()}
              for p in sorted(dest.rglob("*")) if p.is_file()}
    report = {"oracle": "monobit 0.54", "checks": checks, "artifacts": hashes}
    (dest / "results.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(checks, indent=2))


if __name__ == "__main__":
    main()
