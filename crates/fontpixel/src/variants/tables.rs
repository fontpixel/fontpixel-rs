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
    sfnt::{self, bitmap as sbit, glyf, metrics, name::NameTable, post::PostTable},
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
    debug_assert!(bytes <= bound, "table output exceeded its admitted bound");
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

/// Bytes `cff/write.rs` `number()` writes: 1/2/3-byte integers, else 5 bytes.
#[cfg(feature = "cff")]
fn operand(v: f64) -> usize {
    if !v.is_finite() || v.fract() != 0. {
        5
    } else if (-107. ..=107.).contains(&v) {
        1
    } else if (-1131. ..=1131.).contains(&v) {
        2
    } else if (-32768. ..=32767.).contains(&v) {
        3
    } else {
        5
    }
}

/// Operand bytes for a move between two coordinates. The writer tracks a
/// rounded current point, so any fractional endpoint takes the 5-byte form.
#[cfg(feature = "cff")]
fn step(from: f64, to: f64) -> usize {
    if from.fract() != 0. || to.fract() != 0. {
        5
    } else {
        operand(to - from)
    }
}

/// Upper bound of one Type 2 charstring (sfnt/cff/write.rs `PathWriter` and the
/// dot loop): the advance, two operands plus an operator per move/line (a
/// merged h/v line run writes fewer), six operands plus an operator per curve,
/// a move plus a two-byte `callgsubr` per dot, and `endchar`.
#[cfg(feature = "cff")]
fn charstring(g: &sfnt::cff::Glyph, dots: sfnt::cff::DotOptions) -> usize {
    use sfnt::cff::{Command, Outline};
    let mut bytes = operand(g.advance).saturating_add(1);
    let mut at = (0., 0.);
    let mut to = |bytes: &mut usize, p: (f64, f64)| {
        *bytes = bytes.saturating_add(step(at.0, p.0) + step(at.1, p.1));
        at = p;
    };
    match &g.outline {
        Outline::Path(path) => {
            for &c in path {
                match c {
                    Command::MoveTo(p) | Command::LineTo(p) => {
                        to(&mut bytes, p);
                        bytes = bytes.saturating_add(1);
                    }
                    Command::CurveTo(a, b, p) => {
                        to(&mut bytes, a);
                        to(&mut bytes, b);
                        to(&mut bytes, p);
                        bytes = bytes.saturating_add(1);
                    }
                    Command::Close => {}
                }
            }
        }
        Outline::Dots(origins) => {
            let fixed = |x: f64| (x * 65536.).round() / 65536.;
            let r = fixed(dots.diameter / 2.);
            let center = fixed(dots.cell_size / 2.);
            for &(x, y) in origins {
                to(&mut bytes, (x + center + r, y + center));
                bytes = bytes.saturating_add(3);
            }
        }
    }
    bytes
}

/// `cff::Font::encode` (sfnt/cff/write.rs): name set, string and charstring
/// arrays, all INDEX copies, charset, top DICTs and the final table.
#[cfg(feature = "cff")]
pub(super) fn cff(
    font: &sfnt::cff::Font,
    limits: &sfnt::cff::Limits,
    budget: &Budget,
) -> Result<Owned<sfnt::cff::Encoded>> {
    let n = font.glyphs.len();
    let names = font
        .glyphs
        .iter()
        .fold(font.full_name.len(), |a, g| a.saturating_add(g.name.len()));
    let mut programs = 0usize;
    let mut largest = 0usize;
    for g in &font.glyphs {
        let bytes = charstring(g, font.dots).max(8);
        programs = programs.saturating_add(bytes);
        largest = largest.max(bytes);
    }
    let offsets = (n + 1).saturating_mul(4).saturating_add(3);
    let strings_index = sum(&[offsets, names]);
    let charstrings_index = sum(&[offsets, programs]);
    let small = 4096 + font.postscript_name.len().saturating_mul(4);
    let table = sum(&[strings_index, charstrings_index, n.saturating_mul(2), small]);
    let bound = sum(&[
        super::ledger::tree_bytes::<&String, ()>(budget, n)?,
        grown::<Vec<u8>>(n + 1),
        names,
        n.saturating_mul(size_of::<Vec<u8>>()),
        // Charstrings keep at most twice their length, plus one growth overlap.
        programs.saturating_mul(2),
        largest,
        strings_index,
        charstrings_index,
        grown::<u8>(n.saturating_mul(2).saturating_add(1)),
        small,
        table,
    ]);
    admit(
        budget,
        bound,
        || font.encode(limits),
        |e| exact(&mut e.data).saturating_add(exact(&mut e.maxp)),
    )
}

/// `metrics::encode` (sfnt/metrics.rs): two exact buffers.
pub(super) fn metrics(
    axis: metrics::Axis,
    header: &metrics::Header,
    values: &[metrics::LongMetric],
    limits: &sfnt::Limits,
    budget: &Budget,
) -> Result<Owned<metrics::EncodedMetrics>> {
    let bound = sum(&[36, values.len().saturating_mul(4)]);
    admit(
        budget,
        bound,
        || metrics::encode(axis, header, values, true, limits),
        |e| exact(&mut e.header).saturating_add(exact(&mut e.metrics)),
    )
}

/// Fixed-size encoders: `head`, `maxp`, `OS/2` (at most 100 bytes) and `gasp`
/// each fill one exact buffer of at most this many bytes.
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

/// `PostTable::encode` (sfnt/post.rs): string references, indices, the
/// name-to-index tree and one exact output.
pub(super) fn post(
    table: &PostTable,
    num_glyphs: u16,
    limits: &sfnt::Limits,
    budget: &Budget,
) -> Result<Owned<Vec<u8>>> {
    let (n, text) = table.names.as_ref().map_or((0, 0), |names| {
        (
            names.len(),
            names.iter().fold(0usize, |a, s| a.saturating_add(s.len())),
        )
    });
    let bound = sum(&[
        grown::<&str>(n),
        grown::<u16>(n),
        super::ledger::tree_bytes::<&str, u16>(budget, n)?,
        34,
        n.saturating_mul(3),
        text,
    ]);
    admit(budget, bound, || table.encode(num_glyphs, limits), exact)
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

/// `bitmap::encode` (sfnt/bitmap.rs) for one monochrome strike: growing data
/// and location buffers, glyph-run groups with offset arrays, and one glyph's
/// encoded image at a time.
pub(super) fn embedded(
    strike: &sbit::Strike,
    num_glyphs: u16,
    limits: &sbit::Limits,
    budget: &Budget,
) -> Result<Owned<sbit::Tables>> {
    let n = strike.glyphs.len();
    let mut images = 4usize;
    let mut largest = 0usize;
    for g in strike.glyphs.values() {
        let stride = (g.bitmap.width() as usize).div_ceil(8);
        let bytes = sum(&[8, stride.saturating_mul(g.bitmap.height() as usize)]);
        images = images.saturating_add(bytes);
        largest = largest.max(bytes);
    }
    let location = sum(&[56, n.saturating_mul(24)]);
    let bound = sum(&[
        grown::<u8>(images),
        grown::<u8>(location),
        size_of::<usize>(),
        grown::<(u16, u16, u16, usize, Vec<u32>)>(n),
        // Each run's offsets start at one entry and grow from four:
        // at most 3 * max(len + 1, 4) entries per run, with runs <= glyphs.
        grown::<u32>(n.saturating_mul(5)),
        grown::<u8>(largest),
    ]);
    admit(
        budget,
        bound,
        || {
            sbit::encode(
                std::slice::from_ref(strike),
                sbit::Kind::OpenType,
                num_glyphs,
                limits,
            )
        },
        |t| exact(&mut t.data).saturating_add(exact(&mut t.location)),
    )
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
