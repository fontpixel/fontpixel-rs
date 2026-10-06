"""Multi-file formats: output bundles, sidecar resources, memory vs filesystem.

    python resources_sidecars.py [INPUT] [OUTPUT_DIR]

A BMFont is a `.fnt` descriptor plus PNG page images. Writing returns every
file in an `OutputBundle`; reading needs the pages as *resources*. Nothing in
the native API reads the filesystem implicitly: sidecars come from a
`ResourceSet`, a `{name: bytes}` dict or a resolver callable. Only
`fontpixel.ops` has a filesystem mode (`files=None`). Writes the BMFont files
to OUTPUT_DIR/bmfont and an ops conversion to OUTPUT_DIR/ops.
"""
from __future__ import annotations

import sys

import fontpixel as fp
from _common import arguments
from fontpixel import ops


def main(argv: list[str] | None = None) -> int:
    source, out = arguments(__doc__ or "", argv)
    fonts = fp.read(source.read_bytes(), name=source.name).fonts
    assert isinstance(fonts, fp.FontSet)

    # 1. A multi-asset write: primary file plus sidecars, all in memory.
    written = fp.write(fonts, "bmfont")
    bundle = written.output
    print("primary:", bundle.primary, "| assets:", [a.path for a in bundle.assets])
    target = out / "bmfont"
    target.mkdir(exist_ok=True)
    for asset in bundle:                       # OutputAsset: path + bytes (copies)
        (target / asset.path).write_bytes(asset.data)

    # 2. Reading the descriptor alone fails: the page is a missing resource.
    descriptor = (target / bundle.primary).read_bytes()
    try:
        fp.read(descriptor, name=bundle.primary)
    except fp.MissingResourceError as error:
        print("without sidecars:", error.kind, error.path)

    # 3. Supply sidecars: a ResourceSet, a dict, or a resolver callable.
    pages = {a.path: a.data for a in bundle.assets if a.path != bundle.primary}
    from_set = fp.read(descriptor, name=bundle.primary, resources=fp.ResourceSet(pages))
    from_dict = fp.read(descriptor, name=bundle.primary, resources=pages)

    def resolve(path: str, max_bytes: int) -> bytes:
        data = (target / path).read_bytes()     # the caller decides what to read
        assert len(data) <= max_bytes
        return data

    from_disk = fp.read(descriptor, name=bundle.primary, resources=resolve)
    assert from_set.format == from_dict.format == from_disk.format == "bmfont"

    # A Written/OutputBundle reads back with its own name and sidecars, and
    # an Input carries data, name and resources together.
    again = fp.read(written)
    packed = fp.Input(descriptor, name=bundle.primary, resources=pages)
    print("read back:", again.format, "| via Input:", fp.read(packed).format)

    # 4. Operations: memory mode returns outputs; filesystem mode writes them
    #    (transactionally, never overwriting the source) like the CLI. `.fnt`
    #    is ambiguous (Windows FNT, BMFont, ...), so `to` names the format.
    memory = ops.convert(inputs=["font.bdf"], output="font.fnt", to="bmfont",
                         files={"font.bdf": source.read_bytes()})
    print("memory outputs:", sorted(memory.outputs))
    ops_dir = out / "ops"
    ops_dir.mkdir(exist_ok=True)
    disk = ops.convert(inputs=[source], output=ops_dir / "font.fnt", to="bmfont")
    print("filesystem outputs:", sorted(p.name for p in ops_dir.iterdir()),
          "| result.outputs is empty:", disk.outputs == {})
    return 0


if __name__ == "__main__":
    sys.exit(main())
