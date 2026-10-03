# OpenType CFF container status

W2.1 implements the sfnt container/common tables. W2.3 implements the CFF1
outline-table reader/encoder documented in [cff.md](cff.md), with native bitmap
outline generation and one shared cubic circle subroutine. Feature `otf` enables
`cff`, which enables `sfnt`; `cff` can also be selected directly.

There is intentionally no native `otf` registry read/write hook yet. Embedded
bitmap strikes belong to W2.4, and pixel-grid detection/raster/FontSet projection
to W2.7. Neither a borrowed sfnt parser nor a CFF outline table is represented as
a completed FontSet codec. Independent tests assemble real OTF containers using
the production CFF and W2.1 table/container APIs.
