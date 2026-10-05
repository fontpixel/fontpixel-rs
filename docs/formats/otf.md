# OpenType CFF container status

W2.1 implements the sfnt container/common tables. W2.3 implements the CFF1
outline-table reader/encoder documented in [cff.md](cff.md), with native bitmap
outline generation and one shared cubic circle subroutine. Feature `otf` enables
`cff`, which enables `sfnt`; `cff` can also be selected directly.

The restored baseline already registers the native `otf` reader/writer through
`formats::vector_raster`; registry installation is guarded by the `otf` feature.
It projects supported CFF outlines/embedded strikes into the bitmap model and
writes CFF OpenType from the model. This corrects the earlier W2.3-only status;
it adds no codec or dialect. Low-level CFF/sfnt APIs remain available separately.
See [vector-raster.md](vector-raster.md), [cff.md](cff.md) and [ttc.md](ttc.md)
for geometry, table-preservation and collection boundaries. Variable CFF2 and
hint interpreter execution are not implied. Phase-6 recovery history is
recorded in [recovery status](../recovery-status.md).

Native CFF axis-aligned paths pack alternating hlineto/vlineto operands in runs
of at most 40 (below the Type 2 48-entry stack limit), splitting at other command
kinds or a repeated axis. This affects binary encoding only; coordinates,
command budgets and the circular global subroutine are unchanged.
