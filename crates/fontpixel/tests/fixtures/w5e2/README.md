# Original E2 fixtures

All glyph artwork is original, MIT, produced from the repository's own adapters.
No third-party font samples are included. Reproduce external fixtures with
`tests/crosscheck/w5e2.py`; it pins the Java libraries and records tool exits.

- `rfont-original.png`: Bits’N’Picas RFont exporter, three Unicode pages, Gray8,
  negative/positive bearings, native text and rfVM/rfHM version 1.
- `sfont-original.png`: Bits’N’Picas SFont exporter, original five characters
  among its 94 native slots; positive first bearing permits Java self-import.
- `sfont-overhang.png`: same producer with negative first bearing; Java's own
  importer rejects its vertical detection column. Both Rust and unchanged SDL
  accept it and agree on every rendered coverage sample and advance.
- `processing-original.vlw`: Processing PFont.save version 11, three glyphs,
  a blank advance, NUL/supplementary modified UTF and non-ASCII scalar.
- `pillow-original.pil` / `.pbm`: Pillow FontFile.save, original monochrome masks
  in native byte positions 65 and 255. The `.pbm` file is native PNG.

Hashes below identify the exact original fixture bytes:

```json
{
  "rfont-original.png": {
    "bytes": 525,
    "sha256": "497a5fec441ceda9c7936fcd786e350bdf6600f14b6e5bbd24942aec2a860e13"
  },
  "pillow-original.pbm": {
    "bytes": 73,
    "sha256": "7d4fbedf2ceeef79f594c2eaa8accec27a64351dc23e751f3f7e8cca07929ac1"
  },
  "pillow-original.pil": {
    "bytes": 5142,
    "sha256": "4cfff13702e0c4adb1fc918f11725f2115a966b2990f0352f51256fa41623701"
  },
  "sfont-overhang.png": {
    "bytes": 339,
    "sha256": "da4622e0cadf103aff339b06ada0fc098b8922c624e473ccbc7a5ba62e6a1674"
  },
  "processing-original.vlw": {
    "bytes": 151,
    "sha256": "9130aaaae8382831e53c365f10860d370aee6f5ec0b7a7ffba80f9769c7b93de"
  },
  "sfont-original.png": {
    "bytes": 340,
    "sha256": "930382df102c9f4aac2daf9f46d2f40eccc3945f5866a7fc5226629e20ea8954"
  }
}
```
