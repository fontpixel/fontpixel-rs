"""Per-format typed options: discover, validate and apply them.

    python typed_options.py [INPUT] [OUTPUT_DIR]

Every reader/writer option struct of the Rust crate is a plain dict with the
Rust field names; `fontpixel.types` has a `TypedDict` for each, so a type
checker validates keys and enum strings. At run time unknown keys, wrong
types and unknown enum names raise `ValueError`.

Writes `font.psf` (PSF1, no Unicode table), `font-msb.pcf` (big-endian PCF
with 4-byte row padding) and `font.psf2` (PSF2 with a Unicode table) to
OUTPUT_DIR.
"""
from __future__ import annotations

import sys

import fontpixel as fp
from _common import arguments
from fontpixel.formats import fzx, psf
from fontpixel.types import PcfWriteOptions, PsfWriteOptions


def main(argv: list[str] | None = None) -> int:
    source, out = arguments(__doc__ or "", argv)
    fonts = fp.read(source.read_bytes(), name=source.name).fonts
    assert isinstance(fonts, fp.FontSet)

    # 1. Discover: which formats take typed options, and their defaults.
    configurable = fp.formats.configurable_formats()
    print("formats with write options:", len(configurable["write"]))
    print("PSF option types:", [t for t in fp.formats.option_types() if t.startswith("formats::psf::")])
    defaults = fp.formats.normalize("formats::psf::PsfWriteOptions")
    print("PSF defaults:", {k: v for k, v in defaults.items() if k != "limits"})

    # 2. Per-codec namespace: the module's own Rust functions. PSF's writer
    #    returns (bytes, report); bundle writers return a Written.
    psf_options: PsfWriteOptions = {"version": "1", "unicode_table": "omit"}
    data, report = psf.write(fonts, psf_options)
    (out / "font.psf").write_bytes(data)
    print("PSF1 magic:", data[:2].hex(), "| report entries:", len(report["entries"]))

    # 3. Generic write() with the format's options dict.
    pcf_options: PcfWriteOptions = {"byte_order": "msb", "glyph_pad": "4"}
    pcf = fp.write(fonts, "pcf", pcf_options)
    (out / "font-msb.pcf").write_bytes(pcf.output.into_single())

    # 4. A Registry carries options for later reads and writes.
    registry = fp.Registry()
    registry.set_psf_write_options({"version": "2", "unicode_table": "include"})
    psf2 = registry.write(fonts, "psf")
    (out / "font.psf2").write_bytes(psf2.output.into_single())
    print("PSF detect:", psf.detect((out / "font.psf2").read_bytes()))

    # A writer that cannot represent the model reports errors and returns
    # nothing: FZX stores byte codes, so Unicode mappings are rejected unless
    # the `unicode_as_legacy` option allows them.
    try:
        fzx.write(fonts, {"tracking": 1})
    except fp.ReportedErrorsError as error:
        first = error.report["entries"][0]
        print(f"fzx: {error.kind}: {first['code']}: {first['message']}")

    # 5. Validation: unknown keys and enum names fail before any work.
    for bad in ({"version": "3"}, {"no_such_key": True}):
        try:
            fp.formats.normalize("formats::psf::PsfWriteOptions", bad)
        except ValueError as error:
            print("rejected:", error)
    return 0


if __name__ == "__main__":
    sys.exit(main())
