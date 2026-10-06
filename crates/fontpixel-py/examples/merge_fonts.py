"""Merge glyphs from several fonts: the library call and the CLI operation.

    python merge_fonts.py [INPUT] [OUTPUT_DIR]

Takes ASCII from INPUT and "A"/"B" from a second font (later parts only fill
codepoints earlier parts lack, so the alternative glyphs are taken by listing
that part first). Writes `merged.bdf` (library) and `merged-ops.bdf` plus
`merged-ops.pcf` (operation, in memory) to OUTPUT_DIR.
"""
from __future__ import annotations

import sys

import fontpixel as fp
from _common import ALT, arguments
from fontpixel import merge, ops
from fontpixel.types import MergeOptions, MergePartArgs


def main(argv: list[str] | None = None) -> int:
    source, out = arguments(__doc__ or "", argv)
    base = fp.read(source.read_bytes(), name=source.name).fonts
    alt = fp.read(ALT.read_bytes(), name=ALT.name).fonts
    assert isinstance(base, fp.FontSet) and isinstance(alt, fp.FontSet)

    # 1. Library: parts in priority order; selections are expressions or
    #    Selection handles. The fonts are copied for the call.
    selection = merge.Selection("U+41-42")
    print("selection:", selection.expression, "scalars:", selection.scalar_count())
    parts: list[MergePartArgs] = [
        {"font": alt[0], "selection": selection, "label": "alternates"},
        {"font": base[0], "selection": "U+20-7E", "label": "base"},
    ]
    options: MergeOptions = {"family": "Merged Example"}
    result = merge.merge(parts, options)
    merged = result["font"]
    print("merged glyphs:", len(merged), "| family:", result["report"]["family"])
    for part in result["report"]["parts"]:
        print(f"  part {part['label']!r}: took {part['taken_glyphs']} glyphs")
    written = fp.write(merged, "bdf")
    (out / "merged.bdf").write_bytes(written.output.into_single())

    # 2. The `fontpixel merge` operation, in memory: request keys are the
    #    command-line options; files= names the input buffers.
    files = {"base.bdf": source.read_bytes(), "alt.bdf": ALT.read_bytes()}
    op = ops.merge(
        {
            "parts": [{"input": "alt.bdf", "take": "U+41-42"},
                      {"input": "base.bdf", "take": "U+20-7E"}],
            "outputs": [{"destination": "merged-ops.bdf"}, {"destination": "merged-ops.pcf"}],
            "family": "Merged Example",
        },
        files=files,
    )
    for key, data in sorted(op.outputs.items()):
        (out / key).write_bytes(data)
    print("operation outputs:", sorted(op.outputs), "| exit code:", op.exit_code)

    # Both routes produce the same glyphs.
    from_ops = fp.read(op.outputs["merged-ops.bdf"], format="bdf").fonts
    assert isinstance(from_ops, fp.FontSet)
    assert len(from_ops[0]) == len(merged)
    return 0


if __name__ == "__main__":
    sys.exit(main())
