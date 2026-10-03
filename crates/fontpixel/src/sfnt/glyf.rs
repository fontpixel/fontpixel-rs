//! Editable, bounded TrueType simple/composite outlines and short/long loca.
//! Glyph bytecode is retained, never interpreted. Flattening is unhinted.
use super::{bounded, bytes, invalid, p16, p32, pi16, to16, to32, u16_at, u32_at};
use crate::{
    Result,
    outline::{self, Contour},
};
use read_fonts::{FontData, FontRead, tables::glyf::SimpleGlyph as ReadSimple};

/// Geometry and work limits in addition to the common sfnt byte budgets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Container/table/glyph budgets.
    pub sfnt: super::Limits,
    /// Per-glyph expanded geometry budgets.
    pub geometry: outline::Limits,
    /// Maximum components in one composite.
    pub max_components: usize,
    /// Maximum nested composite depth (hard ceiling 64).
    pub max_depth: usize,
    /// Maximum graph-node visits while expanding one glyph, including empty nodes.
    pub max_visits: usize,
    /// Cumulative simple points across the font.
    pub max_total_points: usize,
    /// Cumulative component records across the font.
    pub max_total_components: usize,
    /// Cumulative instruction bytes across the font.
    pub max_total_instructions: usize,
    /// Cumulative graph-node visits while encoding an entire font.
    pub max_total_visits: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            sfnt: super::Limits::default(),
            geometry: outline::Limits::default(),
            max_components: 65535,
            max_depth: 32,
            max_visits: 1024 * 1024,
            max_total_points: 16 * 1024 * 1024,
            max_total_components: 4 * 1024 * 1024,
            max_total_instructions: 16 * 1024 * 1024,
            max_total_visits: 32 * 1024 * 1024,
        }
    }
}
/// Placement of a component after its linear transform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    /// Signed design-unit translation.
    Offset(i16, i16),
    /// Match an already accumulated parent point with a transformed child point.
    Point {
        /// Parent point index.
        parent: u16,
        /// Child point index.
        child: u16,
    },
}
/// How the transform applies to offset arguments.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OffsetMode {
    /// Neither flag present; use recommended unscaled-offset semantics.
    #[default]
    Default,
    /// Transform the offset vector.
    Scaled,
    /// Leave the offset vector untransformed.
    Unscaled,
}
/// One composite component. Matrix words are signed exact F2DOT14 values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Component {
    /// Child glyph ID.
    pub glyph_id: u16,
    /// Offset or point attachment.
    pub anchor: Anchor,
    /// `(xx, xy, yx, yy)`; x'=xx*x+yx*y, y'=xy*x+yy*y.
    pub transform: [i16; 4],
    /// Offset transform policy.
    pub offset_mode: OffsetMode,
    /// Retain ROUND_XY_TO_GRID; unhinted design-space flattening does not grid fit.
    pub round_to_grid: bool,
    /// Retain USE_MY_METRICS for a caller's metrics resolver.
    pub use_my_metrics: bool,
    /// Component contours may overlap.
    pub overlap: bool,
    /// Retain historical NON_OVERLAPPING flag (bit 4, obsolete).
    pub non_overlapping: bool,
}
impl Component {
    /// Construct an explicitly unscaled translation with an identity transform.
    pub fn translated(glyph_id: u16, x: i16, y: i16) -> Self {
        Self {
            glyph_id,
            anchor: Anchor::Offset(x, y),
            transform: [16384, 0, 0, 16384],
            offset_mode: OffsetMode::Unscaled,
            round_to_grid: false,
            use_my_metrics: false,
            overlap: false,
            non_overlapping: false,
        }
    }
}
/// Simple glyph's ordered points and opaque interpreter program.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SimpleGlyph {
    /// Closed contours, with integral signed 16-bit design coordinates.
    pub contours: Vec<Contour>,
    /// Preserved instruction bytes.
    pub instructions: Vec<u8>,
    /// OVERLAP_SIMPLE marker.
    pub overlap: bool,
}
/// Composite glyph's ordered child references and opaque interpreter program.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompositeGlyph {
    /// Components in point-numbering order; must not be empty.
    pub components: Vec<Component>,
    /// Preserved instruction bytes.
    pub instructions: Vec<u8>,
}
/// One glyph entry, retaining component relationships rather than flattening them.
#[derive(Clone, Debug, PartialEq)]
pub enum Glyph {
    /// No record, represented by equal adjacent loca offsets.
    Empty,
    /// Direct quadratic contours.
    Simple(SimpleGlyph),
    /// Transformed child glyphs.
    Composite(CompositeGlyph),
}
impl Glyph {
    /// Construct an unhinted simple glyph, or an empty glyph for no contours.
    pub fn simple(contours: Vec<Contour>) -> Self {
        if contours.is_empty() {
            Self::Empty
        } else {
            Self::Simple(SimpleGlyph {
                contours,
                ..Default::default()
            })
        }
    }
    /// Construct one shared-pixel component for every ink cell.
    pub fn pixels(
        bitmap: &crate::Bitmap,
        options: &outline::Options,
        shared_glyph: u16,
        limits: &Limits,
    ) -> Result<Self> {
        let origins = outline::pixel_origins(bitmap, options, &limits.geometry)?;
        bounded(origins.len(), limits.max_components, "glyf components")?;
        let mut components = Vec::with_capacity(origins.len());
        for (x, y) in origins {
            components.push(Component::translated(shared_glyph, coord(x)?, coord(y)?));
        }
        Ok(if components.is_empty() {
            Self::Empty
        } else {
            Self::Composite(CompositeGlyph {
                components,
                instructions: Vec::new(),
            })
        })
    }
}
/// Parsed glyph order, beginning with glyph 0 (`.notdef`).
#[derive(Clone, Debug, PartialEq)]
pub struct GlyfTable {
    /// Stable glyph-ID order.
    pub glyphs: Vec<Glyph>,
}
/// Loca serialization choice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LocaFormat {
    /// Choose short offsets when all half-offsets fit in 16 bits.
    #[default]
    Auto,
    /// Require short (uint16 offset divided by two) entries.
    Short,
    /// Use uint32 byte offsets.
    Long,
}
/// Geometry maxima for an unhinted maxp table or an existing maxp update.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Statistics {
    /// Simple glyph's maximum point count.
    pub max_points: u16,
    /// Simple glyph's maximum contour count.
    pub max_contours: u16,
    /// Maximum expanded composite point count.
    pub max_composite_points: u16,
    /// Maximum expanded composite contour count.
    pub max_composite_contours: u16,
    /// Largest instruction program length.
    pub max_size_of_instructions: u16,
    /// Largest number of immediate child components.
    pub max_component_elements: u16,
    /// Largest nesting depth; a simple child has depth zero.
    pub max_component_depth: u16,
}
impl Statistics {
    /// Generate maxp 1.0 for unhinted output. Hinted glyphs require caller-supplied
    /// interpreter maxima, so this helper rejects any instruction programs.
    pub fn unhinted_maxp(&self, num_glyphs: u16) -> Result<Vec<u8>> {
        if num_glyphs == 0 || self.max_size_of_instructions != 0 {
            return Err(invalid("unhinted maxp requires glyphs and no bytecode"));
        }
        let mut out = Vec::with_capacity(32);
        p32(&mut out, 0x10000);
        for n in [
            num_glyphs,
            self.max_points,
            self.max_contours,
            self.max_composite_points,
            self.max_composite_contours,
            1,
            0,
            0,
            0,
            0,
            0,
            0,
            self.max_component_elements,
            self.max_component_depth,
        ] {
            p16(&mut out, n);
        }
        Ok(out)
    }
}
/// Encoded outlines and values the caller must reconcile with head/maxp.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Encoded {
    /// Glyph data, each entry aligned to two bytes.
    pub glyf: Vec<u8>,
    /// Location table with `numGlyphs + 1` entries.
    pub loca: Vec<u8>,
    /// Value for head.indexToLocFormat (0 or 1).
    pub index_to_loc_format: i16,
    /// Whole-font control-point bounds, or zeros for all-empty fonts.
    pub bounds: [i16; 4],
    /// Recomputed geometry maxima.
    pub statistics: Statistics,
}
fn coord(v: f64) -> Result<i16> {
    if !v.is_finite() || v.fract() != 0.0 || v < f64::from(i16::MIN) || v > f64::from(i16::MAX) {
        Err(invalid("glyf coordinate is not an integral int16"))
    } else {
        Ok(v as i16)
    }
}
fn bounds(cs: &[Contour]) -> Result<Option<[i16; 4]>> {
    let mut b = None::<[f64; 4]>;
    for &(x, y, _) in cs.iter().flatten() {
        if !x.is_finite() || !y.is_finite() {
            return Err(invalid("non-finite transformed coordinate"));
        }
        if let Some(b) = &mut b {
            b[0] = b[0].min(x);
            b[1] = b[1].min(y);
            b[2] = b[2].max(x);
            b[3] = b[3].max(y);
        } else {
            b = Some([x, y, x, y]);
        }
    }
    b.map(|b| {
        Ok([
            coord(b[0].floor())?,
            coord(b[1].floor())?,
            coord(b[2].ceil())?,
            coord(b[3].ceil())?,
        ])
    })
    .transpose()
}
fn count(cs: &[Contour]) -> Result<usize> {
    cs.iter().try_fold(0usize, |n, c| {
        n.checked_add(c.len())
            .ok_or_else(|| invalid("point count overflow"))
    })
}
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let b = bytes(self.data, self.pos, n)?;
        self.pos += n;
        Ok(b)
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn word(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }
    fn signed(&mut self) -> Result<i16> {
        Ok(self.word()? as i16)
    }
    fn instructions(&mut self, limit: usize) -> Result<Vec<u8>> {
        let n = self.word()? as usize;
        bounded(n, limit, "glyf instructions")?;
        Ok(self.take(n)?.to_vec())
    }
}
impl GlyfTable {
    /// Decode loca-delimited glyphs. Fontations interprets points after strict
    /// flag, count, coordinate and glyph-slice completeness checks.
    pub fn decode(
        glyf: &[u8],
        loca: &[u8],
        num_glyphs: u16,
        index_to_loc_format: i16,
        l: &Limits,
    ) -> Result<Self> {
        l.sfnt.glyphs(num_glyphs as usize)?;
        l.sfnt.table(glyf.len())?;
        l.sfnt.table(loca.len())?;
        let size = match index_to_loc_format {
            0 => 2,
            1 => 4,
            _ => return Err(invalid("invalid indexToLocFormat")),
        };
        bytes(loca, 0, (num_glyphs as usize + 1) * size)?;
        let mut offsets = Vec::with_capacity(num_glyphs as usize + 1);
        for i in 0..=num_glyphs as usize {
            let n = if size == 2 {
                usize::from(u16_at(loca, i * 2)?) * 2
            } else {
                u32_at(loca, i * 4)? as usize
            };
            if n > glyf.len() || offsets.last().is_some_and(|last| *last > n) {
                return Err(invalid("invalid or descending loca offset"));
            }
            offsets.push(n);
        }
        let mut glyphs = Vec::with_capacity(num_glyphs as usize);
        let (mut points, mut components, mut instructions) = (0usize, 0usize, 0usize);
        for span in offsets.windows(2) {
            let mut remaining = l.clone();
            remaining.geometry.max_points = remaining
                .geometry
                .max_points
                .min(l.max_total_points - points);
            remaining.max_components = remaining
                .max_components
                .min(l.max_total_components - components);
            remaining.max_total_instructions = l.max_total_instructions - instructions;
            let g = decode_glyph(&glyf[span[0]..span[1]], &remaining)?;
            match &g {
                Glyph::Empty => {}
                Glyph::Simple(g) => {
                    points = points.saturating_add(count(&g.contours)?);
                    instructions = instructions.saturating_add(g.instructions.len());
                }
                Glyph::Composite(g) => {
                    components = components.saturating_add(g.components.len());
                    instructions = instructions.saturating_add(g.instructions.len());
                    if g.components.iter().any(|c| c.glyph_id >= num_glyphs) {
                        return Err(invalid("component glyph ID out of range"));
                    }
                }
            }
            bounded(points, l.max_total_points, "glyf total points")?;
            bounded(components, l.max_total_components, "glyf total components")?;
            bounded(
                instructions,
                l.max_total_instructions,
                "glyf total instructions",
            )?;
            glyphs.push(g);
        }
        let result = Self { glyphs };
        result.validate_graph(l)?;
        Ok(result)
    }
    /// Decode the outlines of a standalone sfnt through its typed head/maxp.
    pub fn from_sfnt(font: &super::Sfnt<'_>, l: &Limits) -> Result<Self> {
        use read_fonts::TableProvider;
        let h = font.font_ref().head().map_err(invalid)?;
        let m = font.font_ref().maxp().map_err(invalid)?;
        Self::decode(
            font.table(*b"glyf")
                .ok_or_else(|| invalid("missing glyf"))?,
            font.table(*b"loca")
                .ok_or_else(|| invalid("missing loca"))?,
            m.num_glyphs(),
            h.index_to_loc_format(),
            l,
        )
    }
    fn validate_graph(&self, l: &Limits) -> Result<()> {
        l.sfnt.glyphs(self.glyphs.len())?;
        let mut states = vec![0u8; self.glyphs.len()];
        let mut depths = vec![0usize; self.glyphs.len()];
        fn visit(
            table: &GlyfTable,
            gid: usize,
            depth: usize,
            states: &mut [u8],
            depths: &mut [usize],
            l: &Limits,
        ) -> Result<usize> {
            bounded(depth, l.max_depth.min(64), "glyf component depth")?;
            match states.get(gid) {
                Some(1) => return Err(invalid("cyclic composite glyph")),
                Some(2) => {
                    bounded(
                        depth.saturating_add(depths[gid]),
                        l.max_depth.min(64),
                        "glyf component depth",
                    )?;
                    return Ok(depths[gid]);
                }
                None => return Err(invalid("component glyph ID out of range")),
                _ => {}
            }
            states[gid] = 1;
            let mut d = 0;
            if let Glyph::Composite(g) = &table.glyphs[gid] {
                if g.components.is_empty() {
                    return Err(invalid("empty composite glyph"));
                }
                bounded(g.components.len(), l.max_components, "glyf components")?;
                for c in &g.components {
                    d = d.max(1 + visit(table, c.glyph_id as usize, depth + 1, states, depths, l)?);
                }
            }
            states[gid] = 2;
            depths[gid] = d;
            Ok(d)
        }
        for gid in 0..self.glyphs.len() {
            visit(self, gid, 0, &mut states, &mut depths, l)?;
        }
        Ok(())
    }
    /// Resolve unhinted outlines, including scaled offsets and point attachments.
    /// Device grid rounding, bytecode, variations and phantom points are excluded.
    pub fn contours(&self, glyph_id: u16, l: &Limits) -> Result<Vec<Contour>> {
        let mut visits = 0;
        let mut path = Vec::new();
        self.expand(glyph_id as usize, l, &mut visits, &mut path)
            .map(|v| v.0)
    }
    fn expand(
        &self,
        gid: usize,
        l: &Limits,
        visits: &mut usize,
        path: &mut Vec<usize>,
    ) -> Result<(Vec<Contour>, usize)> {
        *visits = visits.saturating_add(1);
        bounded(*visits, l.max_visits, "glyf expansion visits")?;
        bounded(path.len(), l.max_depth.min(64), "glyf component depth")?;
        if path.contains(&gid) {
            return Err(invalid("cyclic composite glyph"));
        }
        let g = self
            .glyphs
            .get(gid)
            .ok_or_else(|| invalid("glyph ID out of range"))?;
        match g {
            Glyph::Empty => Ok((Vec::new(), 0)),
            Glyph::Simple(g) => {
                bounded(
                    count(&g.contours)?,
                    l.geometry.max_points,
                    "glyf expanded points",
                )?;
                Ok((g.contours.clone(), 0))
            }
            Glyph::Composite(g) => {
                bounded(g.components.len(), l.max_components, "glyf components")?;
                if g.components.is_empty() {
                    return Err(invalid("empty composite glyph"));
                }
                path.push(gid);
                let mut out = Vec::<Contour>::new();
                let (mut points, mut depth) = (0usize, 0usize);
                for c in &g.components {
                    let (mut child, cd) = self.expand(c.glyph_id as usize, l, visits, path)?;
                    depth = depth.max(cd + 1);
                    points = points.saturating_add(count(&child)?);
                    bounded(points, l.geometry.max_points, "glyf expanded points")?;
                    let [xx, xy, yx, yy] = c.transform.map(|v| f64::from(v) / 16384.0);
                    let transform = |(x, y): (f64, f64)| (x * xx + y * yx, x * xy + y * yy);
                    for p in child.iter_mut().flatten() {
                        let (x, y) = transform((p.0, p.1));
                        p.0 = x;
                        p.1 = y;
                    }
                    let (dx, dy) = match c.anchor {
                        Anchor::Offset(x, y) => {
                            if c.offset_mode == OffsetMode::Scaled {
                                transform((f64::from(x), f64::from(y)))
                            } else {
                                (f64::from(x), f64::from(y))
                            }
                        }
                        Anchor::Point { parent, child: cp } => {
                            let a = out.iter().flatten().nth(parent as usize).ok_or_else(|| {
                                invalid(
                                    "invalid parent attachment point (phantom points unsupported)",
                                )
                            })?;
                            let b = child.iter().flatten().nth(cp as usize).ok_or_else(|| {
                                invalid(
                                    "invalid child attachment point (phantom points unsupported)",
                                )
                            })?;
                            (a.0 - b.0, a.1 - b.1)
                        }
                    };
                    for p in child.iter_mut().flatten() {
                        p.0 += dx;
                        p.1 += dy;
                    }
                    out.extend(child);
                }
                path.pop();
                Ok((out, depth))
            }
        }
    }
    /// Deterministically encode outlines, recomputing glyph/font bounds and
    /// geometry maxima. No sfnt or model metadata is silently invented.
    pub fn encode(&self, format: LocaFormat, l: &Limits) -> Result<Encoded> {
        self.validate_graph(l)?;
        let mut glyf = Vec::new();
        let mut offsets = Vec::with_capacity(self.glyphs.len() + 1);
        let mut statistics = Statistics::default();
        let mut all_bounds = None::<[i16; 4]>;
        let (mut total_points, mut total_components, mut total_instructions) =
            (0usize, 0usize, 0usize);
        let mut total_visits = 0usize;
        for (id, g) in self.glyphs.iter().enumerate() {
            offsets.push(to32(glyf.len())?);
            let (mut visits, mut path) = (0, Vec::new());
            let mut remaining = l.clone();
            remaining.max_visits = remaining
                .max_visits
                .min(l.max_total_visits.saturating_sub(total_visits));
            let (cs, depth) = self.expand(id, &remaining, &mut visits, &mut path)?;
            total_visits = total_visits.saturating_add(visits);
            let b = bounds(&cs)?;
            if let Some(b) = b {
                if let Some(all) = &mut all_bounds {
                    all[0] = all[0].min(b[0]);
                    all[1] = all[1].min(b[1]);
                    all[2] = all[2].max(b[2]);
                    all[3] = all[3].max(b[3]);
                } else {
                    all_bounds = Some(b);
                }
            }
            let points = count(&cs)?;
            match g {
                Glyph::Empty => {}
                Glyph::Simple(g) => {
                    total_points = total_points.saturating_add(points);
                    total_instructions = total_instructions.saturating_add(g.instructions.len());
                    statistics.max_points = statistics.max_points.max(to16(points)?);
                    statistics.max_contours = statistics.max_contours.max(to16(cs.len())?);
                    statistics.max_size_of_instructions = statistics
                        .max_size_of_instructions
                        .max(to16(g.instructions.len())?);
                }
                Glyph::Composite(g) => {
                    total_components = total_components.saturating_add(g.components.len());
                    total_instructions = total_instructions.saturating_add(g.instructions.len());
                    statistics.max_composite_points =
                        statistics.max_composite_points.max(to16(points)?);
                    statistics.max_composite_contours =
                        statistics.max_composite_contours.max(to16(cs.len())?);
                    statistics.max_component_elements = statistics
                        .max_component_elements
                        .max(to16(g.components.len())?);
                    statistics.max_component_depth =
                        statistics.max_component_depth.max(to16(depth)?);
                    statistics.max_size_of_instructions = statistics
                        .max_size_of_instructions
                        .max(to16(g.instructions.len())?);
                }
            }
            bounded(total_points, l.max_total_points, "glyf total points")?;
            bounded(
                total_components,
                l.max_total_components,
                "glyf total components",
            )?;
            bounded(
                total_instructions,
                l.max_total_instructions,
                "glyf total instructions",
            )?;
            let entry = encode_glyph(g, b.unwrap_or([0; 4]), l)?;
            l.sfnt.table(glyf.len().saturating_add(entry.len() + 1))?;
            glyf.extend(entry);
            if glyf.len() % 2 != 0 {
                glyf.push(0);
            }
        }
        offsets.push(to32(glyf.len())?);
        let short = match format {
            LocaFormat::Auto => glyf.len() / 2 <= u16::MAX as usize,
            LocaFormat::Short => {
                if glyf.len() / 2 > u16::MAX as usize {
                    return Err(invalid("short loca overflow"));
                }
                true
            }
            LocaFormat::Long => false,
        };
        let mut loca = Vec::with_capacity(offsets.len() * if short { 2 } else { 4 });
        for offset in offsets {
            if short {
                p16(&mut loca, (offset / 2) as u16);
            } else {
                p32(&mut loca, offset);
            }
        }
        l.sfnt.table(loca.len())?;
        bounded(
            glyf.len().saturating_add(loca.len()),
            l.sfnt.resources.max_total_bytes,
            "glyf output bytes",
        )?;
        Ok(Encoded {
            glyf,
            loca,
            index_to_loc_format: if short { 0 } else { 1 },
            bounds: all_bounds.unwrap_or([0; 4]),
            statistics,
        })
    }
}
fn decode_glyph(data: &[u8], l: &Limits) -> Result<Glyph> {
    if data.is_empty() {
        return Ok(Glyph::Empty);
    }
    let mut r = Cursor { data, pos: 0 };
    let nc = r.signed()?;
    r.take(8)?;
    if nc >= 0 {
        if nc == 0 && data.len() == 10 {
            return Ok(Glyph::Simple(SimpleGlyph::default()));
        }
        bytes(data, 10, nc as usize * 2)?;
        let mut ends = Vec::with_capacity(nc as usize);
        for _ in 0..nc {
            let e = r.word()? as usize;
            if ends.last().is_some_and(|prev| *prev >= e) {
                return Err(invalid("non-increasing contour endpoint"));
            }
            ends.push(e);
        }
        let n = ends.last().map_or(0, |v| v + 1);
        bounded(n, l.geometry.max_points, "glyf points")?;
        let instructions = r.instructions(l.max_total_instructions)?;
        let mut flags = Vec::with_capacity(n);
        while flags.len() < n {
            let f = r.byte()?;
            if f & 0x80 != 0 {
                return Err(invalid("reserved simple glyph flag"));
            }
            let repeat = if f & 8 != 0 {
                usize::from(r.byte()?) + 1
            } else {
                1
            };
            if repeat > n - flags.len() {
                return Err(invalid("glyph flag run exceeds point count"));
            }
            flags.extend(std::iter::repeat_n(f, repeat));
        }
        for (short, same) in [(2, 16), (4, 32)] {
            let mut value = 0i32;
            for f in &flags {
                let delta = if f & short != 0 {
                    let n = i32::from(r.byte()?);
                    if f & same != 0 { n } else { -n }
                } else if f & same == 0 {
                    i32::from(r.signed()?)
                } else {
                    0
                };
                value += delta;
                if i16::try_from(value).is_err() {
                    return Err(invalid("simple glyph coordinate overflow"));
                }
            }
        }
        // Typed point reading occurs only after the independent structural pass.
        let typed = ReadSimple::read(FontData::new(data)).map_err(invalid)?;
        let mut pts = vec![read_fonts::types::Point::<i32>::default(); n];
        let mut point_flags = vec![read_fonts::tables::glyf::PointFlags::default(); n];
        typed
            .read_points_fast(&mut pts, &mut point_flags)
            .map_err(invalid)?;
        let mut contours = Vec::with_capacity(ends.len());
        let mut start = 0;
        for end in ends {
            contours.push(
                (start..=end)
                    .map(|i| {
                        (
                            f64::from(pts[i].x),
                            f64::from(pts[i].y),
                            point_flags[i].is_on_curve(),
                        )
                    })
                    .collect(),
            );
            start = end + 1;
        }
        Ok(Glyph::Simple(SimpleGlyph {
            contours,
            instructions,
            overlap: flags.first().is_some_and(|f| f & 64 != 0),
        }))
    } else {
        let mut components = Vec::new();
        let final_flags = loop {
            bounded(components.len() + 1, l.max_components, "glyf components")?;
            let flags = r.word()?;
            let glyph_id = r.word()?;
            if flags & !0x1fff != 0 || (flags & 0x1800) == 0x1800 || (flags & 0xc8).count_ones() > 1
            {
                return Err(invalid("invalid composite flags"));
            }
            if flags & 0x120 == 0x120 {
                return Err(invalid("instructions flag before final component"));
            }
            let words = flags & 1 != 0;
            let xy = flags & 2 != 0;
            let anchor = if xy {
                let (x, y) = if words {
                    (r.signed()?, r.signed()?)
                } else {
                    (i16::from(r.byte()? as i8), i16::from(r.byte()? as i8))
                };
                Anchor::Offset(x, y)
            } else {
                let (parent, child) = if words {
                    (r.word()?, r.word()?)
                } else {
                    (u16::from(r.byte()?), u16::from(r.byte()?))
                };
                Anchor::Point { parent, child }
            };
            let transform = if flags & 8 != 0 {
                let scale = r.signed()?;
                [scale, 0, 0, scale]
            } else if flags & 64 != 0 {
                [r.signed()?, 0, 0, r.signed()?]
            } else if flags & 128 != 0 {
                [r.signed()?, r.signed()?, r.signed()?, r.signed()?]
            } else {
                [16384, 0, 0, 16384]
            };
            components.push(Component {
                glyph_id,
                anchor,
                transform,
                offset_mode: if flags & 0x800 != 0 {
                    OffsetMode::Scaled
                } else if flags & 0x1000 != 0 {
                    OffsetMode::Unscaled
                } else {
                    OffsetMode::Default
                },
                round_to_grid: flags & 4 != 0,
                use_my_metrics: flags & 0x200 != 0,
                overlap: flags & 0x400 != 0,
                non_overlapping: flags & 0x10 != 0,
            });
            if flags & 32 == 0 {
                break flags;
            }
        };
        let instructions = if final_flags & 0x100 != 0 {
            r.instructions(l.max_total_instructions)?
        } else {
            Vec::new()
        };
        Ok(Glyph::Composite(CompositeGlyph {
            components,
            instructions,
        }))
    }
}
fn encode_glyph(g: &Glyph, b: [i16; 4], l: &Limits) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let nc = match g {
        Glyph::Empty => return Ok(out),
        Glyph::Simple(g) => {
            i16::try_from(g.contours.len()).map_err(|_| invalid("too many contours"))?
        }
        Glyph::Composite(_) => -1,
    };
    pi16(&mut out, nc);
    for v in b {
        pi16(&mut out, v);
    }
    match g {
        Glyph::Empty => {}
        Glyph::Simple(g) => {
            let mut n = 0usize;
            for c in &g.contours {
                if c.is_empty() {
                    return Err(invalid("empty simple contour"));
                }
                n = n
                    .checked_add(c.len())
                    .ok_or_else(|| invalid("point count overflow"))?;
                p16(&mut out, to16(n - 1)?);
            }
            bounded(n, l.geometry.max_points, "glyf points")?;
            p16(&mut out, to16(g.instructions.len())?);
            out.extend(&g.instructions);
            let (mut flags, mut xs, mut ys) = (Vec::new(), Vec::new(), Vec::new());
            let (mut px, mut py) = (0i32, 0i32);
            let axis = |v: i32,
                        prev: &mut i32,
                        short: u8,
                        same: u8,
                        flag: &mut u8,
                        bytes: &mut Vec<u8>|
             -> Result<()> {
                let d = v - *prev;
                *prev = v;
                if d == 0 {
                    *flag |= same;
                } else if (-255..=255).contains(&d) {
                    *flag |= short;
                    if d > 0 {
                        *flag |= same;
                    }
                    bytes.push(d.unsigned_abs() as u8);
                } else {
                    pi16(
                        bytes,
                        i16::try_from(d).map_err(|_| invalid("coordinate delta exceeds int16"))?,
                    );
                }
                Ok(())
            };
            for &(x, y, on) in g.contours.iter().flatten() {
                let mut f = u8::from(on);
                if flags.is_empty() && g.overlap {
                    f |= 64;
                }
                axis(i32::from(coord(x)?), &mut px, 2, 16, &mut f, &mut xs)?;
                axis(i32::from(coord(y)?), &mut py, 4, 32, &mut f, &mut ys)?;
                flags.push(f);
            }
            let mut i = 0;
            while i < flags.len() {
                let mut run = 1;
                while run < 256 && i + run < flags.len() && flags[i + run] == flags[i] {
                    run += 1;
                }
                if run > 1 {
                    out.extend([flags[i] | 8, (run - 1) as u8]);
                } else {
                    out.push(flags[i]);
                }
                i += run;
            }
            out.extend(xs);
            out.extend(ys);
        }
        Glyph::Composite(g) => {
            for (i, c) in g.components.iter().enumerate() {
                let mut flags = 0u16;
                if i + 1 < g.components.len() {
                    flags |= 32;
                } else if !g.instructions.is_empty() {
                    flags |= 256;
                }
                if c.round_to_grid {
                    flags |= 4;
                }
                if c.use_my_metrics {
                    flags |= 512;
                }
                if c.overlap {
                    flags |= 1024;
                }
                if c.non_overlapping {
                    flags |= 16;
                }
                flags |= match c.offset_mode {
                    OffsetMode::Default => 0,
                    OffsetMode::Scaled => 2048,
                    OffsetMode::Unscaled => 4096,
                };
                let words = match c.anchor {
                    Anchor::Offset(x, y) => {
                        flags |= 2;
                        i8::try_from(x).is_err() || i8::try_from(y).is_err()
                    }
                    Anchor::Point { parent, child } => parent > 255 || child > 255,
                };
                if words {
                    flags |= 1;
                }
                let [xx, xy, yx, yy] = c.transform;
                let matrix = if xy != 0 || yx != 0 {
                    flags |= 128;
                    vec![xx, xy, yx, yy]
                } else if xx != yy {
                    flags |= 64;
                    vec![xx, yy]
                } else if xx != 16384 {
                    flags |= 8;
                    vec![xx]
                } else {
                    Vec::new()
                };
                p16(&mut out, flags);
                p16(&mut out, c.glyph_id);
                let (a, b) = match c.anchor {
                    Anchor::Offset(x, y) => (x as u16, y as u16),
                    Anchor::Point { parent, child } => (parent, child),
                };
                if words {
                    p16(&mut out, a);
                    p16(&mut out, b);
                } else {
                    out.extend([a as u8, b as u8]);
                }
                for v in matrix {
                    pi16(&mut out, v);
                }
            }
            if !g.instructions.is_empty() {
                p16(&mut out, to16(g.instructions.len())?);
                out.extend(&g.instructions);
            }
        }
    }
    l.sfnt.table(out.len())?;
    Ok(out)
}
