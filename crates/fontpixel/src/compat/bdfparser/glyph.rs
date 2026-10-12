use super::bitmap::{dim, invalid};
use super::projection::{core_glyph, meta, num, text};
use super::{Bitmap, Font, Meta};
use crate::model::GlyphId;
use crate::{
    Result,
    bitmap::{Bitmap as Raster, Mono},
};
use serde_json::{Map, Value as V, json};
use std::rc::Rc;

/// The exact ordering of SPEC-v3's 17 glyph metadata slots.
pub const META_TITLES: [&str; 17] = [
    "glyphname",
    "codepoint",
    "bbw",
    "bbh",
    "bbxoff",
    "bbyoff",
    "swx0",
    "swy0",
    "dwx0",
    "dwy0",
    "swx1",
    "swy1",
    "dwx1",
    "dwy1",
    "vvectorx",
    "vvectory",
    "hexdata",
];
/// Default Unicode/PostScript-compatible glyph name or legacy decimal name.
pub fn defaultglyphname(cp: i64, unicode: bool) -> String {
    if cp < 0 || !unicode {
        format!("char{cp}")
    } else if cp <= 0xffff {
        format!("uni{cp:04X}")
    } else {
        format!("u{cp:X}")
    }
}
/// A metadata snapshot with an optional live native-model attachment.
#[derive(Debug)]
pub struct Glyph {
    pub(super) font: Font,
    pub(super) id: Option<GlyphId>,
    pub(super) generation: u64,
    pub(super) ever_attached: bool,
    /// This handle's snapshot; shared with `synced` until it is edited.
    pub(super) meta: Rc<Meta>,
    pub(super) synced: Rc<Meta>,
}
impl Clone for Glyph {
    fn clone(&self) -> Self {
        Self {
            font: self.font.shared(),
            id: None,
            generation: 0,
            ever_attached: false,
            meta: self.meta.clone(),
            synced: Rc::default(),
        }
    }
}
impl Glyph {
    /// Construct a detached glyph from permissive SPEC metadata.
    pub fn new(fields: &Map<String, V>, font: &Font) -> Result<Self> {
        let meta = META_TITLES
            .iter()
            .map(|k| fields.get(*k).cloned().unwrap_or(V::Null))
            .collect();
        Ok(Self {
            font: font.shared(),
            id: None,
            generation: 0,
            ever_attached: false,
            meta: Rc::new(meta),
            synced: Rc::default(),
        })
    }
    /// An attached handle on slot `id` at `generation` holding `snapshot`
    /// (what [`attached`](Self::attached) built when the slot held it).
    pub(super) fn with_snapshot(font: &Font, id: GlyphId, generation: u64, snapshot: Meta) -> Self {
        let m = Rc::new(snapshot);
        Self {
            font: font.shared(),
            id: Some(id),
            generation,
            ever_attached: true,
            meta: m.clone(),
            synced: m,
        }
    }
    pub(super) fn attached(font: &Font, id: GlyphId) -> Self {
        let s = font.state.borrow();
        let m = Rc::new(s.core().glyph(id).map(meta).unwrap_or(vec![V::Null; 17]));
        Self {
            font: font.shared(),
            id: Some(id),
            generation: s.generations.get(&id).copied().unwrap_or(0),
            ever_attached: true,
            meta: m.clone(),
            synced: m,
        }
    }
    /// The metadata snapshot owned by this handle, in SPEC's 17-slot order.
    pub fn meta(&self) -> &Meta {
        &self.meta
    }
    fn meta_mut(&mut self) -> &mut Meta {
        Rc::make_mut(&mut self.meta)
    }
    /// Shared owning font handle (unlike Font::clone, this is not a deep copy).
    pub fn font(&self) -> Font {
        self.font.shared()
    }
    /// The slot this handle refers to now: its own, or where a codepoint
    /// change (through any handle on the same glyph) moved it.
    fn follow(&mut self) {
        let s = self.font.state.borrow();
        if let Some(mut id) = self.id {
            let mut generation = self.generation;
            while let Some(&(next, g)) = s.moved.get(&(id, generation)) {
                id = next;
                generation = g;
            }
            self.id = Some(id);
            self.generation = generation;
        }
    }
    /// Whether the glyph's slot (followed through codepoint moves) still exists
    /// and has not been replaced.
    pub fn is_attached(&self) -> bool {
        let s = self.font.state.borrow();
        let Some(mut id) = self.id else {
            return false;
        };
        let mut generation = self.generation;
        while let Some(&(next, g)) = s.moved.get(&(id, generation)) {
            id = next;
            generation = g;
        }
        s.core().glyph(id).is_some() && s.generations.get(&id) == Some(&generation)
    }
    /// The attached slot, followed through codepoint moves.
    pub(super) fn slot(&mut self) -> Option<GlyphId> {
        self.follow();
        if self.is_attached() { self.id } else { None }
    }
    /// Numeric encoding; unencoded glyphs (and a None or non-integer
    /// codepoint) have -1.
    pub fn cp(&self) -> i64 {
        self.meta[1].as_i64().unwrap_or(-1)
    }
    /// Unicode character through the owner's codec.
    pub fn chr(&self) -> Result<Option<char>> {
        self.font.cp2char(self.cp())
    }
    fn sync(&mut self) -> Result<()> {
        self.follow();
        if !self.is_attached() {
            self.id = None;
            return Ok(());
        }
        let id = self.id.ok_or_else(|| invalid("detached glyph"))?;
        let current = self
            .font
            .state
            .borrow()
            .core()
            .glyph(id)
            .map(meta)
            .ok_or_else(|| invalid("missing glyph"))?;
        let oldcp = num(&current[1]);
        let mut next = (*self.meta).clone();
        for i in 0..17 {
            if next[i] == self.synced[i] && current[i] != self.synced[i] {
                next[i] = current[i].clone();
            }
        }
        if next[16].is_null() {
            next[16] = json!([]);
        }
        if next[1].is_null() || num(&next[1]) < 0 {
            next[1] = json!(-1);
        }
        let cp = num(&next[1]);
        let mut g = core_glyph(&next)?;
        if cp < 0
            && oldcp < 0
            && let Some(alt) = self
                .font
                .state
                .borrow()
                .core()
                .glyph(id)
                .and_then(|g| g.properties.get(crate::formats::x11::ALT))
                .cloned()
        {
            g.properties.set(crate::formats::x11::ALT, alt);
        }
        if cp != oldcp {
            self.font.state.borrow_mut().codec = None;
            let existing = self.font.id(cp);
            let mut s = self.font.state.borrow_mut();
            s.core_mut().remove_glyph(id);
            let old = (id, self.generation);
            if let Some(existing) = existing {
                s.core_mut().replace_glyph(existing, g)?;
                s.serial += 1;
                let generation = s.serial;
                s.generations.insert(existing, generation);
                self.id = Some(existing);
                self.generation = generation;
            } else {
                let new_id = s.core_mut().add_glyph(g)?;
                s.generations.insert(new_id, self.generation);
                self.id = Some(new_id);
            }
            // Other handles on the moved glyph follow it (bdfparser shares one list).
            s.moved
                .insert(old, (self.id.unwrap_or(id), self.generation));
        } else {
            self.font
                .state
                .borrow_mut()
                .core_mut()
                .replace_glyph(id, g)?;
        }
        let next = Rc::new(next);
        self.meta = next.clone();
        self.synced = next;
        Ok(())
    }
    /// Draw in the font cell (0), glyph box (1), source padding (2), or custom box (-1).
    pub fn draw(&self, mode: i64, bb: Option<[i64; 4]>) -> Result<Bitmap> {
        if !matches!(mode, -1..=2) {
            return Err(invalid("Unknown draw mode"));
        }
        if mode == -1 && bb.is_none() {
            return Err(invalid(
                "Parameter bb in draw() method must be set when mode=-1",
            ));
        }
        let w = dim(num(&self.meta[2]))?;
        let h = dim(num(&self.meta[3]))?;
        crate::bitmap::BitmapLimits::default().check(
            w,
            h,
            (w as usize).saturating_mul(h as usize),
        )?;
        let hex = self.meta[16].as_array().cloned().unwrap_or_default();
        let total = hex
            .iter()
            .filter_map(V::as_str)
            .try_fold(0usize, |a, s| a.checked_add(s.len().saturating_mul(4)))
            .ok_or_else(|| invalid("hex rows overflow"))?;
        if total > 16 * 1024 * 1024 {
            return Err(invalid("hex rows exceed bitmap budget"));
        }
        let mut rows = Vec::new();
        for (i, r) in hex.iter().enumerate() {
            let r = r.as_str().ok_or_else(|| invalid("hex row must be text"))?;
            let mut row = String::new();
            for c in r.chars() {
                let Some(v) = c.to_digit(16) else {
                    return Err(invalid(&format!(
                        "Glyph \"{}\" (codepoint {}): bitmap row {i} {} contains a non-hex character",
                        text(&self.meta[0]),
                        text(&self.meta[1]),
                        serde_json::to_string(r).unwrap_or_default()
                    )));
                };
                row.push_str(&format!("{v:04b}"));
            }
            rows.push(row);
        }
        if mode == 2 {
            return Bitmap::new(&rows);
        }
        let w = dim(num(&self.meta[2]))? as usize;
        let h = dim(num(&self.meta[3]))? as usize;
        if rows.len() < h {
            self.font.warn(format!(
                "Glyph \"{}\" (codepoint {}) has {} bitmap rows but its BBX height is {}; padded with empty rows",
                text(&self.meta[0]),
                text(&self.meta[1]),
                rows.len(),
                h
            ));
        }
        rows.resize(h, String::new());
        rows.truncate(h);
        for r in &mut rows {
            r.truncate(w);
            if r.len() < w {
                r.push_str(&"0".repeat(w - r.len()));
            }
        }
        let mut b = Bitmap::new(&rows)?;
        if mode == 1 {
            return Ok(b);
        }
        let bb = if let Some(bb) = bb.filter(|_| mode == -1) {
            bb
        } else {
            let h = self.font.headers();
            [
                num(h.get("fbbx").unwrap_or(&V::Null)),
                num(h.get("fbby").unwrap_or(&V::Null)),
                num(h.get("fbbxoff").unwrap_or(&V::Null)),
                num(h.get("fbbyoff").unwrap_or(&V::Null)),
            ]
        };
        b.crop(
            bb[0],
            bb[1],
            bb[2] - num(&self.meta[4]),
            bb[3] - num(&self.meta[5]),
        )?;
        Ok(b)
    }
    /// Displacement from drawn bottom-left to origin (or its reverse).
    pub fn origin(
        &self,
        mode: i64,
        fromorigin: bool,
        x: Option<i64>,
        y: Option<i64>,
    ) -> Result<[i64; 2]> {
        let (x, y) = match mode {
            0 => {
                let h = self.font.headers();
                (
                    num(h.get("fbbxoff").unwrap_or(&V::Null)),
                    num(h.get("fbbyoff").unwrap_or(&V::Null)),
                )
            }
            1 | 2 => (num(&self.meta[4]), num(&self.meta[5])),
            -1 => (
                x.ok_or_else(|| invalid("origin needs xoff"))?,
                y.ok_or_else(|| invalid("origin needs yoff"))?,
            ),
            _ => return Err(invalid("Unknown origin mode")),
        };
        Ok(if fromorigin { [x, y] } else { [-x, -y] })
    }
    /// Set any known metadata keys, synchronizing only fields changed by this handle.
    pub fn setmeta(&mut self, fields: &Map<String, V>) -> Result<&mut Self> {
        for k in fields.keys() {
            if !META_TITLES.contains(&k.as_str()) {
                return Err(invalid(&format!("Unknown glyph meta key {k:?}")));
            }
        }
        for (k, v) in fields {
            if let Some(i) = META_TITLES.iter().position(|t| *t == k) {
                self.meta_mut()[i] = v.clone();
            }
        }
        self.sync()?;
        Ok(self)
    }
    /// Move the glyph's ink offsets in font coordinates.
    pub fn shift(&mut self, x: i64, y: i64) -> Result<&mut Self> {
        self.meta_mut()[4] = json!(
            num(&self.meta[4])
                .checked_add(x)
                .ok_or_else(|| invalid("offset overflows"))?
        );
        self.meta_mut()[5] = json!(
            num(&self.meta[5])
                .checked_add(y)
                .ok_or_else(|| invalid("offset overflows"))?
        );
        self.sync()?;
        Ok(self)
    }
    /// Replace raster, optionally trimming it; advance metrics are unchanged.
    pub fn setbitmap(
        &mut self,
        bitmap: &Bitmap,
        mode: i64,
        bb: Option<[i64; 4]>,
        trim: bool,
    ) -> Result<&mut Self> {
        let (mut x, mut y) = match mode {
            0 => {
                let h = self.font.headers();
                (
                    num(h.get("fbbxoff").unwrap_or(&V::Null)),
                    num(h.get("fbbyoff").unwrap_or(&V::Null)),
                )
            }
            1 | 2 => (num(&self.meta[4]), num(&self.meta[5])),
            -1 => {
                let b = bb.ok_or_else(|| invalid("setbitmap mode -1 requires bb"))?;
                (b[2], b[3])
            }
            _ => return Err(invalid("Unknown setbitmap mode")),
        };
        let mut b = bitmap.raster.to_mono(1)?;
        if trim {
            let bb = b.bbox();
            if bb.width == 0 || bb.height == 0 {
                x = 0;
                y = 0;
                b = Raster::<Mono>::default();
            } else {
                x += bb.x;
                y += bb.y;
                b = b.crop_bounds(bb)?;
            }
        }
        self.meta_mut()[2] = json!(b.width());
        self.meta_mut()[3] = json!(b.height());
        self.meta_mut()[4] = json!(x);
        self.meta_mut()[5] = json!(y);
        self.meta_mut()[16] = if b.width() == 0 {
            json!([])
        } else {
            json!(b.to_hex_rows()?)
        };
        self.sync()?;
        Ok(self)
    }
    /// Draw, apply a bitmap callback, and save its trimmed result.
    pub fn transform(
        &mut self,
        mut function: impl FnMut(&mut Bitmap) -> Result<()>,
        mode: i64,
    ) -> Result<&mut Self> {
        let mut b = self.draw(mode, None)?;
        function(&mut b)?;
        self.setbitmap(&b, mode, None, true)
    }
}
impl std::fmt::Display for Glyph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let b = self.draw(0, None).map_err(|_| std::fmt::Error)?;
        write!(f, "{b}")
    }
}
