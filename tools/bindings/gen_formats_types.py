#!/usr/bin/env python3
"""Generate crates/fontpixel-py/src/formats_types.rs from the rustdoc inventory.

The file is the `normalize_table!` behind `fontpixel.formats.normalize` and
`option_types()`: one row per public type of `fontpixel::formats` and
`fontpixel::resources` that implements both Serialize and Deserialize.

Usage:
    tools/bindings/gen_formats_types.py [INVENTORY.json] [--check]

INVENTORY defaults to `$FONTPIXEL_RUSTDOC_TARGET/inventory.json` (else
`target/rustdoc-json/inventory.json`), as written by tools/bindings/gate.sh.
`--check` writes nothing and exits 1 when the committed file differs from what
the inventory produces (for example after a new serde type was added).
"""
import argparse
import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "crates/fontpixel-py/src/formats_types.rs"
# Canonical inventory path -> public Rust path where the defining module is private.
PUBLIC = {
    "formats::tex_common::TexWriteOptions": "formats::pk::PkWriteOptions",
    "formats::w5a1_common::NativeWriteOptions": "formats::dosstart::DosstartWriteOptions",
    "formats::w5a2_common::NativeReadOptions": "formats::bbc::BbcReadOptions",
    "formats::w5a2_common::NativeWriteOptions": "formats::bbc::BbcWriteOptions",
}
# Private submodules whose items are re-exported one level up.
FLATTENED = ("reader", "writer", "imageset", "presets", "metadata")


def default_inventory():
    target = os.environ.get("FONTPIXEL_RUSTDOC_TARGET") or os.path.join(
        os.environ.get("CARGO_TARGET_DIR", "target"), "rustdoc-json")
    return os.path.join(target, "inventory.json")


def render(items):
    rows = []
    for i in items:
        p = i["path"]
        if not (p.startswith("fontpixel::formats::") or p.startswith("fontpixel::resources::")):
            continue
        if i["kind"] not in ("struct", "enum") or "Deserialize" not in i.get("serde", []):
            continue
        key = p.replace("fontpixel::", "", 1)
        rust = PUBLIC.get(key, key)
        parts = rust.split("::")
        if len(parts) > 3 and parts[2] in FLATTENED:
            rust = "::".join(parts[:2] + parts[3:])
        rows.append((key, rust))
    rows.sort()
    out = ["// Generated from the rustdoc inventory: one row per serde type.",
           "// Regenerate with tools/bindings/gen_formats_types.py; do not edit by hand.",
           "// Every serde type of `fontpixel::formats` and `fontpixel::resources`.",
           "normalize_table! {"]
    out += [f'    "{key}" => fontpixel::{rust},' for key, rust in rows]
    out.append("}")
    return "\n".join(out) + "\n", len(rows)


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("inventory", nargs="?", default=default_inventory())
    p.add_argument("--check", action="store_true")
    a = p.parse_args()
    with open(a.inventory) as f:
        text, count = render(json.load(f)["items"])
    if a.check:
        current = OUT.read_text() if OUT.exists() else ""
        if current != text:
            print(f"{OUT.relative_to(ROOT)} is out of date; run tools/bindings/gen_formats_types.py",
                  file=sys.stderr)
            sys.exit(1)
        print(f"{OUT.relative_to(ROOT)} up to date ({count} types)")
        return
    OUT.write_text(text)
    print(f"wrote {OUT.relative_to(ROOT)} ({count} types)")


if __name__ == "__main__":
    main()
