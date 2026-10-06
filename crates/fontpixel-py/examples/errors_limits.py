"""Error handling: exception classes, reports, strict mode and resource limits.

    python errors_limits.py [INPUT] [OUTPUT_DIR]

Every failure raises a subclass of `fontpixel.FontpixelError` carrying
`kind` (the Rust error variant) and `exit_code` (the command-line status),
plus structured fields: `resource`/`limit` for budgets, `format`,
`candidates`, `line`, and the full `report` for strict-mode and
reported-error failures. Writes `strict-report.json` to OUTPUT_DIR.
"""
from __future__ import annotations

import json
import sys

import fontpixel as fp
from _common import arguments
from fontpixel import ops
from fontpixel.types import ResourceLimits


def main(argv: list[str] | None = None) -> int:
    source, out = arguments(__doc__ or "", argv)
    data = source.read_bytes()
    fonts = fp.read(data, name=source.name).fonts
    assert isinstance(fonts, fp.FontSet)

    # 1. Strict mode: a lossy write raises instead of returning a report.
    lenient = fp.write(fonts, "pcf")
    losses = [e for e in lenient.report["entries"] if e["impact"] == "loss"]
    print(f"lenient PCF write: {len(losses)} loss entries, output kept")
    try:
        fp.write(fonts, "pcf", strict=True)
    except fp.StrictLossError as error:
        print(f"strict: {type(error).__name__} kind={error.kind} exit_code={error.exit_code}")
        entry = error.report["entries"][0]
        print(f"  first loss: [{entry['level']}/{entry['impact']}] {entry['code']}: {entry['message']}")
        (out / "strict-report.json").write_text(json.dumps(error.report, indent=1))

    # 2. Resource limits: budgets are dicts; exceeding one raises with the
    #    resource name and the limit that was hit.
    tight: ResourceLimits = {"max_glyphs": 10}
    try:
        fp.read(data, name=source.name, limits=tight)
    except fp.ResourceLimitError as error:
        print(f"limits: {error.resource} > {error.limit} (exit {error.exit_code})")

    defaults = fp.default_resource_limits()
    print("default max_input_bytes:", defaults.get("max_input_bytes"))

    # 3. Format errors.
    try:
        fp.write(fonts, "no-such-format")
    except fp.UnknownFormatError as error:
        print(f"unknown format: {error.format!r}")
    try:
        fp.read(b"\x00\x01 not a font")
    except fp.UnrecognizedFormatError as error:
        print("unrecognized:", error)

    # 4. Typed options are validated before any work (ValueError).
    try:
        fp.write(fonts, "psf", {"version": "9"})
    except ValueError as error:
        print("bad option:", error)

    # 5. Operations: failures carry the command line's exit code; a request
    #    the command line would reject raises InvalidInputError with code 2.
    files = {"font.bdf": data}
    try:
        ops.convert(inputs=["font.bdf"], output="out.pcf", strict=True, files=files)
    except fp.StrictLossError as error:
        print("ops strict exit code:", error.exit_code)
    try:
        ops.convert(inputs=["font.bdf"], each=True, files=files)   # each needs `to`
    except fp.InvalidInputError as error:
        print("ops rejected request, exit code", error.exit_code)

    # 6. Rendering: missing glyphs are reported, or fatal in strict mode.
    try:
        fonts[0].draw("中", {"strict": True})
    except fp.StrictLossError as error:
        print("render strict:", [e["code"] for e in error.report["entries"]])

    # Every class derives from FontpixelError, so one handler catches all;
    # BDF syntax errors also carry the line number.
    try:
        fp.read(b"STARTFONT 2.1\nFONTBOUNDINGBOX x\n", format="bdf")
    except fp.FontpixelError as error:
        line = error.line if isinstance(error, fp.BdfParseError) else None
        print(f"catch-all: {type(error).__name__} at line {line}: {error}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
