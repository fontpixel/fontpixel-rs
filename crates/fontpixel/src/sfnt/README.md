# sfnt common layer

Implemented by W2.1 behind feature `sfnt`. See `docs/formats/sfnt.md` for table
contracts, strictness, independent validation and boundaries. This layer does
not register planned font codecs or construct placeholder glyph data.

W2.3 adds feature `cff` (also enabled by `otf`) for bounded CFF1 Type 2 decoding,
canonical unhinted writing, and shared cubic-dot subroutines. See
`docs/formats/cff.md` for exact supported-program and normalization boundaries.
