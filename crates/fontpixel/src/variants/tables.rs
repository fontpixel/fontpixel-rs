//! Admission for sfnt table encoders that do not take a working ledger yet.
//!
//! Each adapter admits a conservative, source-derived bound for the encoder's
//! scratch plus output *before* calling it. When the encoder returns, all of its
//! temporaries have dropped. Outputs are then shrunk to their exact length while
//! the scratch charge is still held, and only their actual capacity stays
//! charged. The bounds follow the current encoder sources (named on each item).
//! Every growable `Vec` is bounded by `grown`: a `Vec` that reaches `n` bytes by
//! amortized growth never has more than `3 * max(n, 8 elements)` live during a
//! reallocation.
//!
//! Merge point: once the table codecs take a `working::Budget` directly, these
//! adapters should be replaced by those entry points.
use crate::{
    Result, outline,
    sfnt::{self, glyf, name::NameTable},
    working::{Budget, Lease, Owned},
};
use std::mem::size_of;

/// Saturating sum; an overflowing bound can never be admitted.
fn sum(values: &[usize]) -> usize {
    values.iter().fold(0usize, |a, &b| a.saturating_add(b))
}

/// Live bytes of a `Vec<T>` grown to `count` elements, including the overlap
/// of old and new buffers during its last reallocation.
fn grown<T>(count: usize) -> usize {
    count
        .max(8)
        .saturating_mul(size_of::<T>())
        .saturating_mul(3)
}

/// Admit `bound` (scratch plus outputs), run the encoder, then keep exactly
/// the outputs' capacities charged.
fn admit<T>(
    budget: &Budget,
    bound: usize,
    run: impl FnOnce() -> Result<T>,
    shrink: impl FnOnce(&mut T) -> usize,
) -> Result<Owned<T>> {
    let mut total = budget.reserve(bound)?;
    let mut value = run()?;
    // Shrinking reallocates while the scratch charge still covers both buffers.
    let bytes = shrink(&mut value);
    // Unit tests reject a stale bound; other builds (including fuzzing) admit
    // any excess below instead of panicking.
    #[cfg(test)]
    assert!(bytes <= bound, "table output exceeded its admitted bound");
    if bytes > total.bytes() {
        total.grow_to(bytes)?;
    }
    let output = total.split(bytes)?;
    drop(total);
    Ok(Owned::from_parts(value, output))
}

fn exact(bytes: &mut Vec<u8>) -> usize {
    bytes.shrink_to_fit();
    bytes.capacity()
}

/// Expanded points and contours of one glyph, following components.
fn expanded(table: &glyf::GlyfTable, id: usize, depth: usize) -> (usize, usize) {
    // glyf rejects deeper graphs; a saturated bound is never admitted.
    if depth > 64 {
        return (usize::MAX, usize::MAX);
    }
    match table.glyphs.get(id) {
        Some(glyf::Glyph::Simple(g)) => (g.contours.iter().map(Vec::len).sum(), g.contours.len()),
        Some(glyf::Glyph::Composite(g)) => g.components.iter().fold((0, 0), |(p, c), k| {
            let (kp, kc) = expanded(table, usize::from(k.glyph_id), depth + 1);
            (p.saturating_add(kp), c.saturating_add(kc))
        }),
        _ => (0, 0),
    }
}

/// Flag plus x/y delta bytes of a simple glyph's points (sfnt/glyf.rs
/// `encode_glyph`, before flag-run compression): zero, one or two bytes per
/// axis; non-integral coordinates are rejected by the encoder.
fn simple_points(contours: &[outline::Contour]) -> usize {
    let axis = |d: f64| {
        if d == 0. {
            0
        } else if (-255. ..=255.).contains(&d) {
            1
        } else {
            2
        }
    };
    let (mut x, mut y, mut bytes) = (0., 0., 0usize);
    for &(px, py, _) in contours.iter().flatten() {
        bytes = bytes.saturating_add(1 + axis(px - x) + axis(py - y));
        (x, y) = (px, py);
    }
    bytes
}

/// One composite record: flags, glyph ID, byte or word arguments, and the
/// scale/two-scale/matrix words implied by its transform.
fn component(c: &glyf::Component) -> usize {
    let words = match c.anchor {
        glyf::Anchor::Offset(x, y) => i8::try_from(x).is_err() || i8::try_from(y).is_err(),
        glyf::Anchor::Point { parent, child } => parent > 255 || child > 255,
    };
    let [xx, xy, yx, yy] = c.transform;
    let matrix = if xy != 0 || yx != 0 {
        8
    } else if xx != yy {
        4
    } else if xx != 16384 {
        2
    } else {
        0
    };
    4 + if words { 4 } else { 2 } + matrix
}

/// `GlyfTable::encode` (sfnt/glyf.rs): graph validation arrays, offsets, the
/// growing glyf buffer, loca, and per glyph the expansion clone plus
/// `encode_glyph`'s flag/x/y/output buffers.
pub(super) fn glyf(
    table: &glyf::GlyfTable,
    limits: &glyf::Limits,
    budget: &Budget,
) -> Result<Owned<glyf::Encoded>> {
    let n = table.glyphs.len();
    let point = size_of::<outline::Point>();
    let contour = size_of::<outline::Contour>();
    let mut entries = 0usize;
    let mut transient = 0usize;
    for (id, g) in table.glyphs.iter().enumerate() {
        let (points, contours) = expanded(table, id, 0);
        let entry = match g {
            glyf::Glyph::Empty => 0,
            glyf::Glyph::Simple(g) => sum(&[
                12,
                g.contours.len().saturating_mul(2),
                g.instructions.len(),
                simple_points(&g.contours),
            ]),
            glyf::Glyph::Composite(g) => sum(&[
                12,
                g.instructions.len(),
                g.components
                    .iter()
                    .fold(0usize, |a, c| a.saturating_add(component(c))),
            ]),
        };
        entries = sum(&[entries, entry, 1]);
        let work = sum(&[
            // Expanded contours (exact clones) and every nested growing array.
            points.saturating_mul(point),
            grown::<outline::Contour>(contours).saturating_mul(2),
            contours.saturating_mul(contour),
            // Entry plus flags/xs/ys (at most 1/2/2 bytes per point).
            grown::<u8>(entry),
            grown::<u8>(points),
            grown::<u8>(points.saturating_mul(2)).saturating_mul(2),
            // Component path and per-component matrix scratch.
            grown::<usize>(64),
            64,
        ]);
        transient = transient.max(work);
    }
    let bound = sum(&[
        n.saturating_mul(1 + size_of::<usize>()),
        (n + 1).saturating_mul(4).saturating_mul(2),
        grown::<u8>(entries),
        transient,
    ]);
    admit(
        budget,
        bound,
        || table.encode(glyf::LocaFormat::Auto, limits),
        |e| exact(&mut e.glyf).saturating_add(exact(&mut e.loca)),
    )
}

/// Fixed-size encoders: unhinted `maxp` (32 bytes) and `gasp` (8 bytes for
/// one range) each fill one exact buffer of at most this many bytes.
pub(super) fn small(
    budget: &Budget,
    run: impl FnOnce() -> Result<Vec<u8>>,
) -> Result<Owned<Vec<u8>>> {
    admit(budget, 128, run, exact)
}

/// `os2::code_page_ranges` (sfnt/os2.rs) builds one charset codec per page;
/// its canonical key strings are a few short names, dropped per page.
pub(super) fn code_pages(
    budget: &Budget,
    run: impl FnOnce() -> Result<[u32; 2]>,
) -> Result<[u32; 2]> {
    let _scratch = budget.reserve(4096)?;
    run()
}

/// `NameTable::encode` (sfnt/name.rs): record references and their stable sort
/// scratch, the growing string pool, the dedup tree with copied keys, UTF-16
/// language tags and one exact output.
pub(super) fn name(
    table: &NameTable,
    limits: &sfnt::Limits,
    budget: &Budget,
) -> Result<Owned<Vec<u8>>> {
    let records = table
        .records
        .len()
        .saturating_add(table.language_tags.len());
    let values = table
        .records
        .iter()
        .fold(0usize, |a, r| a.saturating_add(r.value.len()));
    let tags = table
        .language_tags
        .iter()
        .fold(0usize, |a, t| a.saturating_add(t.len().saturating_mul(2)));
    let pool = values.saturating_add(tags);
    let bound = sum(&[
        records.saturating_mul(2 * size_of::<usize>()),
        grown::<u8>(pool),
        super::ledger::tree_bytes::<Vec<u8>, u16>(budget, records)?,
        pool,
        grown::<u8>(tags),
        sum(&[8, records.saturating_mul(12), pool]),
    ]);
    admit(budget, bound, || table.encode(limits), exact)
}

/// Assembled sfnt tables that keep their admitted bytes until the output is built.
pub(super) struct Builder {
    builder: sfnt::SfntBuilder,
    tables: Lease,
    tree: Lease,
}

/// Most tables a variants font writes (TrueType with vertical metrics,
/// GSUB and an embedded strike, or the CFF equivalent).
const MAX_TABLES: usize = 16;

impl Builder {
    pub(super) fn new(flavor: sfnt::Flavor, budget: &Budget) -> Result<Self> {
        Ok(Self {
            tree: super::ledger::reserve_tree::<[u8; 4], Vec<u8>>(budget, MAX_TABLES)?,
            tables: budget.reserve(0)?,
            builder: sfnt::SfntBuilder::new(flavor),
        })
    }

    pub(super) fn insert(&mut self, tag: [u8; 4], data: Owned<Vec<u8>>) -> Result<()> {
        let (data, mut lease) = data.into_parts();
        self.tables.merge(&mut lease)?;
        self.builder.insert(tag, data)
    }

    /// Insert a table whose admitted storage is already charged by a lease
    /// covering several outputs.
    pub(super) fn insert_charged(&mut self, tag: [u8; 4], data: Vec<u8>) -> Result<()> {
        self.builder.insert(tag, data)
    }

    pub(super) fn charge(&mut self, mut lease: Lease) -> Result<()> {
        self.tables.merge(&mut lease)
    }

    /// Build while every table remains charged; the builder drops before its leases.
    pub(super) fn build(self, limits: &sfnt::Limits, budget: &Budget) -> Result<Owned<Vec<u8>>> {
        let output = self.builder.build_with_budget(limits, budget)?;
        drop(self.builder);
        drop(self.tables);
        drop(self.tree);
        Ok(output)
    }
}

#[cfg(all(test, feature = "cff"))]
mod tests {
    use super::*;
    use sfnt::cff::{Command, DotOptions, Font, Glyph, Outline};

    fn glyph(name: &str, advance: f64, outline: Outline) -> Glyph {
        Glyph {
            name: name.into(),
            advance,
            outline,
        }
    }

    #[test]
    fn charstring_lengths_match_the_writer_exactly() {
        // 50 alternating h/v lines exceed one 40-operand run.
        let mut stairs = vec![Command::MoveTo((0., 0.))];
        let (mut x, mut y) = (0., 0.);
        for i in 0..50 {
            if i % 2 == 0 {
                x += 100.;
            } else {
                y += 100.;
            }
            stairs.push(Command::LineTo((x, y)));
        }
        stairs.extend([Command::LineTo((0., y)), Command::Close]);
        let mixed = vec![
            Command::MoveTo((10.5, 3.25)),
            Command::LineTo((200., 3.25)),
            Command::LineTo((150., 400.)),
            Command::CurveTo((160.125, 500.), (300., 520.), (400., 0.)),
            Command::LineTo((400., -2000.)),
            Command::LineTo((400., -2000.)),
            Command::Close,
            Command::MoveTo((400., 120.)),
            Command::LineTo((-800., 120.)),
            Command::Close,
        ];
        let font = Font {
            postscript_name: "Ledger".into(),
            full_name: "Ledger".into(),
            units_per_em: 1000,
            glyphs: vec![
                glyph(".notdef", 500., Outline::Path(Vec::new())),
                glyph("stairs", 2600., Outline::Path(stairs)),
                glyph("mixed", 612.5, Outline::Path(mixed)),
                glyph(
                    "dots",
                    300.,
                    Outline::Dots(vec![(0., 0.), (100., 0.), (100., 100.), (2000., -100.)]),
                ),
            ],
            dots: DotOptions {
                cell_size: 100.,
                diameter: 87.3,
            },
        };
        let encoded = font.encode(&Default::default()).unwrap();
        let parsed = sfnt::cff::Cff::parse(&encoded.data, 1000, &Default::default()).unwrap();
        let programs = parsed.fontations().charstrings();
        for (i, g) in font.glyphs.iter().enumerate() {
            assert_eq!(
                programs.get(i).unwrap().len(),
                sfnt::cff::program_len(g, font.dots),
                "{}",
                g.name
            );
        }
    }
}
