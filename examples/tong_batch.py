#!/usr/bin/env python3
"""Emit phase-6 batch TOML for the complete Tong BDF/PCF/OTB/locl export.

Only writes the explicitly requested TOML file. Does not modify source fonts,
run conversions, create output directories, delete stale files or publish a site.
"""
from __future__ import annotations
import argparse
import json
from pathlib import Path

COPYRIGHT = ("Tong Pixel Sans. Derived from Source Han Sans, Source Sans 3, "
             "Source Code Pro (Adobe), the Noto fonts (Google), Plangothic P1; "
             "Han bitmaps partly after TUMBLED (TsFreddie).")
LICENSE = "This Font Software is licensed under the SIL Open Font License, Version 1.1."


def value(item):
    """Encode the small typed subset used by this example as TOML."""
    if isinstance(item, str):
        return json.dumps(item, ensure_ascii=False)
    if isinstance(item, bool):
        return str(item).lower()
    if isinstance(item, (int, float)):
        return str(item)
    if isinstance(item, list):
        return "[" + ", ".join(value(x) for x in item) + "]"
    if isinstance(item, dict):
        return "{ " + ", ".join(f"{value(k)} = {value(v)}" for k, v in item.items()) + " }"
    raise TypeError(type(item))


def tasks(bdf: Path, out: Path, version: str, sizes, only=None, skip_otf=False,
          quality=11, compression_threads=1, dot_reuse="patterns"):
    result = []
    for size in sizes:
        for spacing in ("", "Mono"):
            stem = f"TongPixelSans{spacing}14{size}"
            for region in ("SC", "TC", "JP", "KR", "Latin"):
                source = bdf / f"{stem}-{region}.bdf"
                for kind in (("otb",) if only == "otb" else ("bdf", "pcf", "otb")):
                    result.append(dict(id=f"{stem}-{region}-{kind}", kind="convert",
                        inputs=[str(source)], output=str(out / kind / f"{stem}-{region}.{kind}"),
                        **{"from": "bdf", "to": kind}))
            if only == "otb":
                continue
            family = f"Tong Pixel Sans{' Mono' if spacing else ''} 14 {size}"
            for shape in ((only,) if only else ("square", "dot")):
                for kind in (("ttf", "woff2") if skip_otf else ("ttf", "otf", "woff2")):
                    options = dict(shape=shape, dotsize=0.9 if shape == "dot" else 1.0,
                        ppu=100, upm=1400, style=shape.capitalize(), font_version=version,
                        vendor="TONG", copyright=COPYRIGHT, license=LICENSE,
                        license_url="https://openfontlicense.org", dot_reuse=dot_reuse,
                        woff2_source="auto", woff2_quality=quality,
                        woff2_threads=compression_threads)
                    result.append(dict(id=f"{stem}-{shape}-{kind}", kind="variants",
                        base=str(bdf / f"{stem}-SC.bdf"), family=family,
                        languages={"ZHT,ZHH":str(bdf / f"{stem}-TC.bdf"),
                                   "ja":str(bdf / f"{stem}-JP.bdf"),
                                   "ko":str(bdf / f"{stem}-KR.bdf")},
                        scripts={"latn,grek,cyrl":str(bdf / f"{stem}-Latin.bdf")},
                        output=str(out / kind / f"{stem}-{shape.capitalize()}.{kind}"),
                        to=kind, options=options))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bdf-dir", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--version", required=True, help="deterministic font version, not today's date")
    parser.add_argument("--sizes", nargs="+", choices=["Large", "Small"], default=["Large", "Small"])
    parser.add_argument("--only", choices=["square", "dot", "otb"])
    parser.add_argument("--skip-otf", action="store_true")
    parser.add_argument("--jobs", type=int, default=2)
    parser.add_argument("--quality", type=int, choices=range(12), default=11)
    parser.add_argument("--compression-threads", type=int, choices=range(1,17), default=1)
    parser.add_argument("--dot-reuse", choices=["literal", "rows", "patterns"], default="patterns")
    parser.add_argument("--write", type=Path, required=True)
    args = parser.parse_args()
    if not 1 <= args.jobs <= 64:
        parser.error("--jobs must be 1..64")
    sizes = list(dict.fromkeys(args.sizes))
    bdf, out = args.bdf_dir.resolve(), args.out.resolve()
    rows = tasks(bdf, out, args.version, sizes, args.only, args.skip_otf,
                 args.quality, args.compression_threads, args.dot_reuse)
    required = {Path(path) for row in rows for path in row.get("inputs", [])}
    required.update(Path(row["base"]) for row in rows if "base" in row)
    required.update(Path(path) for row in rows for field in ("languages", "scripts")
                    for path in row.get(field, {}).values())
    missing = sorted(str(path) for path in required if not path.is_file())
    if missing:
        parser.error("missing BDF source(s): " + ", ".join(missing))
    destinations = {Path(row["output"]).resolve() for row in rows}
    if required & destinations:
        parser.error("output must not replace a source BDF")
    text = f"# Generated by examples/tong_batch.py; paths are explicit.\nversion = 1\njobs = {args.jobs}\n"
    for row in rows:
        text += "\n[[tasks]]\n"
        text += "".join(f"{key} = {value(val)}\n" for key, val in row.items())
    # Exclusive creation avoids replacing an unrelated task file accidentally.
    with args.write.open("x", encoding="utf-8") as handle:
        handle.write(text)
    print(f"Wrote {len(rows)} tasks to {args.write}; create output parents, then run fontpixel batch.")


if __name__ == "__main__":
    main()
