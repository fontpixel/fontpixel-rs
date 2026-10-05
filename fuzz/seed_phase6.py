#!/usr/bin/env python3
"""Phase-6 valid seeds for targets that had no or wrong-container seed corpora.

Every seed is either a verbatim copy of a repo-owned MIT fixture, a project-
authored synthetic font generated below (MIT), or the output of the frozen
fontpixel CLI converting one of those (MIT), or the OFL-1.1 3x5-block fixture
converted under a new family name (OFL modified version; no Reserved Font
Name). The 3x5-block source is renamed because OFL counts conversion as a
modification. No seed exceeds MAX_SEED bytes.

  python3 fuzz/seed_phase6.py --binary <fontpixel> [--root fuzz/corpus]
  python3 fuzz/seed_phase6.py --check   # verify corpus files against manifest

Writes fuzz/corpus/<target>/p6-* and fuzz/seed_phase6-manifest.json (source
path + SHA-256 + license + exact generating command for every seed). Existing
non-p6 corpus files are never touched.
"""
import argparse
import gzip
import hashlib
import json
import shutil
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

FUZZ = Path(__file__).resolve().parent
REPO = FUZZ.parent
DEFAULT_BINARY = REPO.parent / "local-work/phase6-local-20261004/frozen-binaries-v2/release/fontpixel"
MANIFEST = FUZZ / "seed_phase6-manifest.json"
MAX_SEED = 64 * 1024
MIT = "MIT (project-owned; LICENSE)"
OFL = "OFL-1.1 modified version, renamed (LICENSES/bdf-fonts/3x5-block.txt, LICENSES/OFL-1.1.txt)"


def sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


# ------------------------------------------------------------ synthetic fonts
def _bdf(name: str, size: int, ascent: int, descent: int, glyphs) -> bytes:
    """glyphs: list of (codepoint, dwidth, (w, h, x, y), rows[int])."""
    w = max(g[2][0] for g in glyphs)
    out = [
        "STARTFONT 2.1", f"FONT -P6-{name}-Medium-R-Normal--{size}-{size * 10}-75-75-C-80-ISO10646-1",
        f"SIZE {size} 75 75", f"FONTBOUNDINGBOX {w} {ascent + descent} 0 {-descent}",
        "STARTPROPERTIES 6", f'FAMILY_NAME "{name}"', f"PIXEL_SIZE {size}", f"FONT_ASCENT {ascent}",
        f"FONT_DESCENT {descent}", 'CHARSET_REGISTRY "ISO10646"', 'CHARSET_ENCODING "1"', "ENDPROPERTIES",
        f"CHARS {len(glyphs)}",
    ]
    for cp, dw, (bw, bh, bx, by), rows in glyphs:
        nbytes = (bw + 7) // 8
        out += [f"STARTCHAR u{cp:04X}", f"ENCODING {cp}", f"SWIDTH {dw * 1000 // size} 0", f"DWIDTH {dw} 0",
                f"BBX {bw} {bh} {bx} {by}", "BITMAP"]
        out += [f"{r << (nbytes * 8 - bw):0{nbytes * 2}X}" for r in rows]
        out.append("ENDCHAR")
    out.append("ENDFONT")
    return ("\n".join(out) + "\n").encode()


def _pattern(cp: int, w: int, h: int) -> list[int]:
    rows = []
    for y in range(h):
        r = 0
        for x in range(w):
            on = ((cp * 7 + x * 3 + y * 5) % 11 < 5) or x in (0, w - 1) and 2 <= y < h - 2
            r = (r << 1) | int(on)
        rows.append(r)
    return rows


def synth_cell16(salt: int = 0) -> bytes:
    """8x16 monospace ASCII plus three 16x16 wide CJK cells (hex/psf friendly).
    salt != 0 gives a same-size regional variant with different CJK artwork."""
    g = [(cp, 8, (8, 16, 0, -2), _pattern(cp, 8, 16)) for cp in range(0x20, 0x7F)]
    g += [(cp, 16, (16, 16, 0, -2), _pattern(cp + salt, 16, 16)) for cp in (0x4E00, 0x4E2D, 0x5B57)]
    return _bdf("P6 Cell Sixteen" + (" JP" if salt else ""), 16, 14, 2, g)


def synth_prop7() -> bytes:
    """Proportional 7px face with varied bearings, empty and ink-less glyphs."""
    g = [(0x20, 3, (0, 0, 0, 0), [])]
    for i, cp in enumerate(range(0x41, 0x5B)):
        w = 3 + i % 4
        h = 5 + i % 3
        g.append((cp, w + 1 + (i % 2), (w, h, i % 2, -(i % 3 == 0)), _pattern(cp, w, h)))
    g.append((0x2026, 7, (5, 1, 1, 0), [0b10101]))
    g.append((0x201C, 4, (3, 2, 0, 5), [0b101, 0b101]))
    return _bdf("P6 Prop Seven", 7, 6, 1, g)


def synth_hex() -> dict[str, bytes]:
    a = "".join(f"{cp:04X}:{''.join(f'{r:02X}' for r in _pattern(cp, 8, 16))}\n" for cp in range(0x41, 0x5B))
    wide = "".join(f"{cp:04X}:{''.join(f'{r:04X}' for r in _pattern(cp, 16, 16))}\n" for cp in (0x4E00, 0x4E2D))
    eight = "".join(f"{cp:04X}:{''.join(f'{r:02X}' for r in _pattern(cp, 8, 8))}\n" for cp in range(0x30, 0x3A))
    return {"synthetic-8x16.hex": a.encode(), "synthetic-mixed-16.hex": ("".join(a.splitlines(True)[:4]) + wide).encode(),
            "synthetic-8x8.hex": eight.encode()}


# ------------------------------------------------------------ sfnt helpers
def sfnt_tables(b: bytes) -> dict[str, bytes]:
    n = struct.unpack(">H", b[4:6])[0]
    out = {}
    for i in range(n):
        tag, _, off, ln = struct.unpack(">4sIII", b[12 + 16 * i: 28 + 16 * i])
        out[tag.decode("latin1")] = b[off: off + ln]
    return out


class Gen:
    def __init__(self, binary: Path, root: Path, work: Path):
        self.binary, self.root, self.work = binary, root, work
        self.records: list[dict] = []
        self.sources: dict[str, dict] = {}
        self.skipped: list[dict] = []

    def source(self, key: str, data: bytes, origin: str, license_: str) -> Path:
        p = self.work / "src" / key
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(data)
        self.sources[key] = {"origin": origin, "sha256": sha(data), "bytes": len(data), "license": license_}
        return p

    def emit(self, target: str, name: str, data: bytes, source: str, command, license_: str):
        if len(data) > MAX_SEED:
            self.skipped.append({"target": target, "name": name, "bytes": len(data), "reason": "exceeds MAX_SEED"})
            return
        d = self.root / target
        d.mkdir(parents=True, exist_ok=True)
        fn = f"p6-{name}"
        (d / fn).write_bytes(data)
        self.records.append({"target": target, "file": f"{target}/{fn}", "sha256": sha(data), "bytes": len(data),
                             "source": source, "command": command, "license": license_})

    def convert(self, src_key: str, out_name: str, args: list[str]) -> tuple[bytes, list[str]]:
        out = self.work / "out" / out_name
        out.parent.mkdir(parents=True, exist_ok=True)
        if out.exists():
            out.unlink()
        argv = [str(self.binary), "convert", str(self.work / "src" / src_key), *args, "-o", str(out)]
        subprocess.run(argv, check=True, capture_output=True)
        shown = ["fontpixel", "convert", f"<src:{src_key}>", *args, "-o", out_name]
        return out.read_bytes(), shown

    def variants(self, base: str, lang: str, out_name: str) -> tuple[bytes, list[str]]:
        out = self.work / "out" / out_name
        out.parent.mkdir(parents=True, exist_ok=True)
        if out.exists():
            out.unlink()
        argv = [str(self.binary), "variants", "--base", str(self.work / "src" / base),
                "--lang", f"ja={self.work / 'src' / lang}", "--family", "P6 Locl", "-o", str(out)]
        subprocess.run(argv, check=True, capture_output=True)
        return out.read_bytes(), ["fontpixel", "variants", "--base", f"<src:{base}>", "--lang", f"ja=<src:{lang}>",
                                  "--family", "P6 Locl", "-o", out_name]


RENAME = ["--family", "P6 Seed Block", "--full-name", "P6 Seed Block", "--postscript-name", "P6SeedBlock"]


def generate(g: Gen):
    # sources
    g.source("cell16.bdf", synth_cell16(), "fuzz/seed_phase6.py synth_cell16()", MIT)
    g.source("prop7.bdf", synth_prop7(), "fuzz/seed_phase6.py synth_prop7()", MIT)
    g.source("cell16-jp.bdf", synth_cell16(3), "fuzz/seed_phase6.py synth_cell16(3)", MIT)
    mini = REPO / "tests/data/compat/mini-circle-1.0-0.bdf"
    g.source("mini.bdf", mini.read_bytes(), str(mini.relative_to(REPO)), "MIT (tests/data/compat/SOURCES.json)")
    block = REPO / "tests/data/bdf/3x5-block.bdf"
    g.source("block.bdf", block.read_bytes(), str(block.relative_to(REPO)), "OFL-1.1 (LICENSES/bdf-fonts/3x5-block.txt)")
    bdfs = [("cell16", "cell16.bdf", MIT, []), ("prop7", "prop7.bdf", MIT, []), ("mini", "mini.bdf", MIT, []),
            ("block", "block.bdf", OFL, RENAME)]

    def conv(target, tag, src, lic, args, ext, extra=()):
        data, cmd = g.convert(src, f"{tag}.{ext}", [*args, *extra])
        g.emit(target, f"{tag}.{ext}", data, src, cmd, lic)
        return data

    sfnt_bytes = {}
    for tag, src, lic, ren in bdfs:
        # webfonts / sfnt / collections
        for ext, extra in (("ttf", []), ("otf", []), ("woff", []), ("woff2", [])):
            data = conv({"ttf": "vector_raster", "otf": "vector_raster", "woff": "woff1", "woff2": "woff2"}[ext],
                        tag, src, lic, ren, ext, ["--to", ext])
            sfnt_bytes[(tag, ext)] = data
            if ext in ("woff", "woff2"):
                g.emit("vector_raster", f"{tag}.{ext}", data, src, ["(copy of woff1/woff2 seed)"], lic)
        data = conv("woff1", f"{tag}-bitmaps", src, lic, ren, "woff", ["--to", "woff", "--embed-bitmaps", "--shape", "dot"])
        g.emit("vector_raster", f"{tag}-bitmaps.woff", data, src, ["(copy of woff1 seed)"], lic)
        conv("woff2", f"{tag}-bitmaps", src, lic, ren, "woff2", ["--to", "woff2", "--embed-bitmaps"])
        # native bitmap writers
        conv("svg", tag, src, lic, ren, "svg", ["--to", "svg"])
        for v in ("1", "2", "3"):
            try:
                conv("fnt", f"{tag}-v{v}", src, lic, ren, "fnt", ["--to", "fnt", "--fnt-version", v])
            except subprocess.CalledProcessError:
                pass
        for v in ("1", "2"):
            try:
                conv("psf", f"{tag}-v{v}", src, lic, ren, "psf", ["--to", "psf", "--psf-version", v])
            except subprocess.CalledProcessError:
                pass
        for extra in (["--to", "hex"], ["--to", "hex", "--hex-descent", "2"]):
            try:
                conv("hex", tag, src, lic, ren, "hex", extra)
                break
            except subprocess.CalledProcessError:
                pass
    # TTC from two faces (v1 and v2 header). The CLI cannot write a WOFF2
    # collection, so the harness's ttcf-flavoured WOFF2 branch stays mutation-only.
    for v2 in (False, True):
        tag = "pair-v2" if v2 else "pair"
        out = g.work / "out" / f"{tag}.ttc"
        if out.exists():
            out.unlink()
        argv = [str(g.binary), "convert", str(g.work / "src/cell16.bdf"), str(g.work / "src/mini.bdf"),
                "--to", "ttc", *(["--collection-v2"] if v2 else []), "-o", str(out)]
        subprocess.run(argv, check=True, capture_output=True)
        data = out.read_bytes()
        cmd = ["fontpixel", "convert", "<src:cell16.bdf>", "<src:mini.bdf>", "--to", "ttc",
               *(["--collection-v2"] if v2 else []), "-o", f"{tag}.ttc"]
        g.emit("collection", f"{tag}.ttc", data, "cell16.bdf+mini.bdf", cmd, MIT)
        g.emit("vector_raster", f"{tag}.ttc", data, "cell16.bdf+mini.bdf", cmd, MIT)
    # synthetic hex
    for name, data in synth_hex().items():
        g.emit("hex", name, data, "fuzz/seed_phase6.py synth_hex()", ["synthetic"], MIT)
    # GSUB from a locl variants font
    data, cmd = g.variants("cell16.bdf", "cell16-jp.bdf", "locl.otf")
    t = sfnt_tables(data)
    g.emit("subset", "locl.otf", data, "cell16.bdf+cell16-jp.bdf", cmd, MIT)
    g.emit("sfnt", "locl.otf", data, "cell16.bdf+cell16-jp.bdf", cmd, MIT)
    if "GSUB" in t:
        g.emit("gsub", "locl.GSUB", t["GSUB"], "locl.otf", cmd + ["| extract GSUB"], MIT)
    # raw tables and whole fonts for sfnt / glyf / cff / subset / compat_ttf
    for (tag, ext), data in sfnt_bytes.items():
        lic = OFL if tag == "block" else MIT
        if ext not in ("ttf", "otf"):
            continue
        tabs = sfnt_tables(data)
        g.emit("sfnt", f"{tag}.{ext}", data, f"{tag}", ["(copy of vector_raster seed)"], lic)
        g.emit("subset", f"{tag}.{ext}", data, f"{tag}", ["(copy of vector_raster seed)"], lic)
        if ext == "ttf":
            g.emit("glyf", f"{tag}.ttf", data, tag, ["(copy of vector_raster seed)"], lic)
            g.emit("compat_ttf", f"{tag}.ttf", data, tag, ["(copy of vector_raster seed)"], lic)
            if "glyf" in tabs:
                g.emit("glyf", f"{tag}.glyf", tabs["glyf"], f"{tag}.ttf", ["| extract glyf"], lic)
        else:
            g.emit("cff", f"{tag}.otf", data, tag, ["(copy of vector_raster seed)"], lic)
            if "CFF " in tabs:
                g.emit("cff", f"{tag}.cff", tabs["CFF "], f"{tag}.otf", ["| extract CFF"], lic)
                g.emit("subset", f"{tag}.cff", tabs["CFF "], f"{tag}.otf", ["| extract CFF"], lic)
        if tag == "cell16":
            for tt in ("cmap", "name", "post", "OS/2", "hhea", "hmtx", "gasp", "vhea", "vmtx"):
                if tt in tabs:
                    g.emit("sfnt", f"{tag}-{ext}.{tt.replace('/', '_')}", tabs[tt], f"{tag}.{ext}",
                           [f"| extract {tt}"], lic)
    # verbatim repo-owned fixtures (previous runner fallback seeds, MIT)
    fixtures = {
        "sfnt": ["tests/data/sfnt/truncated-format4.cmap", "tests/data/compat/*.ttf"],
        "glyf": ["tests/data/compat/*.ttf"], "compat_ttf": ["tests/data/compat/*.ttf"],
        "subset": ["tests/data/compat/*.ttf"], "gsub": [], "cff": ["tests/data/cff/*.cff"],
        "vector_raster": ["tests/data/compat/*.ttf", "tests/data/embedded-bitmap/*.ttf"],
        "collection": ["tests/data/compat/mini-circle-1.0-0.ttf"],
        "woff1": [], "woff2": [],
        "bdf": ["tests/data/bdf/3x5-block.bdf", "tests/data/compat/*.bdf"],
        "pcf": ["tests/data/pcf/*.pcf*"], "yaff": ["tests/data/yaff/*.yaff"],
    }
    seen = set()
    for target, globs in fixtures.items():
        for gl in globs:
            for p in sorted(REPO.glob(gl)):
                data = p.read_bytes()
                if (target, sha(data)) in seen:
                    continue
                seen.add((target, sha(data)))
                lic = "OFL-1.1 verbatim (LICENSES/bdf-fonts/3x5-block.txt)" if "3x5" in p.name else \
                    f"MIT ({p.parent.relative_to(REPO)}/SOURCES*)"
                g.emit(target, f"fixture-{p.name}", data, str(p.relative_to(REPO)), ["verbatim copy"], lic)
    # README recipes: BDF/PCF synthetic oracles (MIT, crates/fontpixel/tests/data)
    cases = json.loads((REPO / "crates/fontpixel/tests/data/bdf_reference.json").read_text())
    for i, c in enumerate(cases[::8]):
        g.emit("bdf", f"oracle-{i:04}", c["source"].encode(), "crates/fontpixel/tests/data/bdf_reference.json",
               ["every 8th case['source']"], "MIT (tests/data/SOURCES.md)")
    pc = json.loads(gzip.decompress((REPO / "crates/fontpixel/tests/data/pcf_reference.json.gz").read_bytes()))
    k = 0
    for c in pc[::4]:
        if "pcf" in c:
            b = bytes.fromhex(c["pcf"])
            if len(b) <= 65536:
                g.emit("pcf", f"oracle-{k:04}", b, "crates/fontpixel/tests/data/pcf_reference.json.gz",
                       ["every 4th case['pcf'] <= 64 KiB"], "MIT (tests/data/SOURCES.md)")
                k += 1
    # the source fonts themselves as text seeds for bdf
    for key in ("cell16.bdf", "prop7.bdf"):
        g.emit("bdf", f"synthetic-{key}", (g.work / "src" / key).read_bytes(), key, ["synthetic"], MIT)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    ap.add_argument("--root", type=Path, default=FUZZ / "corpus")
    ap.add_argument("--manifest", type=Path, default=MANIFEST)
    ap.add_argument("--check", action="store_true", help="verify existing p6 seeds against the manifest")
    a = ap.parse_args()
    if a.check:
        m = json.loads(a.manifest.read_text())
        bad = [r["file"] for r in m["seeds"] if not (a.root / r["file"]).is_file()
               or sha((a.root / r["file"]).read_bytes()) != r["sha256"]]
        print(json.dumps({"seeds": len(m["seeds"]), "mismatched": bad}))
        return 1 if bad else 0
    with tempfile.TemporaryDirectory() as tmp:
        g = Gen(a.binary, a.root, Path(tmp))
        # clear only our own previous p6-* seeds so regeneration is exact
        for p in a.root.glob("*/p6-*"):
            p.unlink()
        generate(g)
    bin_hash = sha(a.binary.read_bytes())
    ver = subprocess.run([str(a.binary), "--version"], capture_output=True, text=True).stdout.strip()
    manifest = {
        "generator": "fuzz/seed_phase6.py", "binary": str(a.binary), "binary_sha256": bin_hash, "binary_version": ver,
        "max_seed_bytes": MAX_SEED, "sources": g.sources,
        "per_target": {t: sum(1 for r in g.records if r["target"] == t) for t in sorted({r["target"] for r in g.records})},
        "skipped": g.skipped,
        "seeds": g.records,
    }
    a.manifest.write_text(json.dumps(manifest, indent=1) + "\n")
    print(json.dumps({"seeds": len(g.records), "skipped": g.skipped, "per_target": manifest["per_target"]}, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
