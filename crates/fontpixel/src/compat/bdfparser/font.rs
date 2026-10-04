use super::bitmap::invalid;
use super::projection::*;
use super::{Bitmap, Glyph, Meta, defaultglyphname};
use crate::charset::{
    CharsetCodec, canonical_charset, charset_props, detect_gl, is_unicode_charset,
};
use crate::formats::{bdf, pcf, x11};
use crate::model::{Codepoint, Font as CoreFont, GlyphId, Property};
use crate::render::{MissingGlyph, RenderMode, RenderOptions, WritingDirection};
use crate::{Report, ReportEntry, ResourceLimits, Result};
use serde_json::{Map, Value as V, json};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

/// Parameters of `Font.create`; defaults match SPEC-v3 §2.6.
#[derive(Debug, Clone)]
pub struct CreateOptions {
    /// Pixel cell width and nominal size.
    pub pixelsize: i64,
    /// Optional baseline-to-top height.
    pub ascent: Option<i64>,
    /// Optional baseline-to-bottom depth.
    pub descent: Option<i64>,
    /// Optional FONT header.
    pub fontname: Option<String>,
    /// Family property and default FONT name.
    pub familyname: String,
    /// Horizontal resolution.
    pub xres: i64,
    /// Vertical resolution.
    pub yres: i64,
}
impl Default for CreateOptions {
    fn default() -> Self {
        Self {
            pixelsize: 16,
            ascent: None,
            descent: None,
            fontname: None,
            familyname: "Untitled".into(),
            xres: 75,
            yres: 75,
        }
    }
}
/// Numeric Unicode conversion callback; numeric scalars also preserve surrogates.
#[derive(Clone)]
pub struct Converter(Rc<dyn Fn(u32) -> Option<u32>>);
impl std::fmt::Debug for Converter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Converter(..)")
    }
}
impl Converter {
    /// Wrap an immutable conversion function, shared when a font is cloned.
    pub fn new(f: impl Fn(u32) -> Option<u32> + 'static) -> Self {
        Self(Rc::new(f))
    }
}
#[derive(Debug, Clone, Default)]
pub(super) struct State {
    pub core: CoreFont,
    pub generations: BTreeMap<GlyphId, u64>,
    pub serial: u64,
    pub report: Report,
    pub warned: BTreeSet<String>,
    pub codec: Option<CharsetCodec>,
    pub char_to_code: Option<Converter>,
    pub code_to_char: Option<Converter>,
}
/// Mutable SPEC font backed by one native font model. Attached handles share it.
#[derive(Debug, Default)]
pub struct Font {
    pub(super) state: Rc<RefCell<State>>,
}
impl Clone for Font {
    fn clone(&self) -> Self {
        Self {
            state: Rc::new(RefCell::new(self.state.borrow().clone())),
        }
    }
}
impl Font {
    pub(super) fn shared(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
    /// Wrap a native strike, retaining effective mappings and unencoded glyphs.
    pub fn from_core(mut core: CoreFont) -> Result<Self> {
        let ids = core
            .glyphs()
            .map(|(id, g)| (id, g.codepoints.clone()))
            .collect::<Vec<_>>();
        for (id, cps) in ids {
            if cps.first().is_some_and(|cp| core.glyph_id(*cp) != Some(id)) {
                core.remove_glyph(id);
            }
        }
        // Alias expansion is a compatibility view, each alias retains an independent handle.
        let original = core.clone();
        let mut normalized = core.clone();
        for (id, _) in original.glyphs() {
            normalized.remove_glyph(id);
        }
        for (_, g) in original.glyphs() {
            if g.codepoints.len() > 1 {
                for cp in &g.codepoints {
                    let mut alias = g.clone();
                    alias.codepoints = vec![*cp];
                    normalized.add_glyph(alias)?;
                }
            } else {
                normalized.add_glyph(g.clone())?;
            }
        }
        if normalized.source_format.as_deref() == Some("pcf") {
            let ids = normalized
                .glyphs()
                .filter(|(_, g)| g.metrics.bounds.width == 0 || g.metrics.bounds.height == 0)
                .map(|(id, _)| id)
                .collect::<Vec<_>>();
            for id in ids {
                normalized.edit_glyph(id, |g| {
                    let mut m = meta(g);
                    m[16] = json!([]);
                    g.properties
                        .set(super::projection::META, property(&json!(m)));
                    Ok(())
                })?;
            }
        }
        let mut s = State {
            core: normalized,
            ..State::default()
        };
        for (id, _) in s.core.glyphs() {
            s.generations.insert(id, 0);
        }
        Ok(Self {
            state: Rc::new(RefCell::new(s)),
        })
    }
    /// A detached copy of the underlying native model.
    pub fn core(&self) -> CoreFont {
        self.state.borrow().core.clone()
    }
    /// Structured parser and compatibility warnings collected so far.
    pub fn report(&self) -> Report {
        self.state.borrow().report.clone()
    }
    pub(super) fn warn(&self, msg: String) {
        let mut s = self.state.borrow_mut();
        s.report.push(ReportEntry {
            level: crate::report::Level::Warning,
            impact: crate::report::Impact::None,
            code: "bdfparser.warning".into(),
            location: Default::default(),
            message: msg,
        });
    }
    /// Load BDF text using the native source-preserving parser.
    pub fn load_string(&mut self, text: &str) -> Result<&mut Self> {
        let (f, r) = bdf::read(text.as_bytes(), &ResourceLimits::default())?;
        *self = Self::from_core(f)?;
        self.state.borrow_mut().report = r;
        Ok(self)
    }
    /// Load BDF lines without changing their supplied delimiters.
    pub fn load_lines(&mut self, lines: impl IntoIterator<Item = String>) -> Result<&mut Self> {
        self.load_string(&lines.into_iter().collect::<Vec<_>>().join("\n"))
    }
    /// Load BDF, PCF, or their gzip transport using the native decoders.
    pub fn load_bytes(&mut self, bytes: &[u8]) -> Result<&mut Self> {
        self.load_bytes_with_options(bytes, None)
    }
    /// Auto-detect BDF/PCF/TTF, optionally selecting the size of external TrueType.
    pub fn load_bytes_with_options(
        &mut self,
        bytes: &[u8],
        pixelsize: Option<i64>,
    ) -> Result<&mut Self> {
        let input = crate::Input::new(bytes);
        let limits = ResourceLimits::default();
        let context = crate::resources::ReadContext::new(&input, &limits)?;
        let bytes = context.bytes();
        if bytes.starts_with(b"OTTO")
            || bytes.starts_with(b"\0\x01\0\0")
            || bytes.starts_with(b"true")
        {
            return self.load_ttf(bytes, pixelsize);
        }
        if bytes.starts_with(b"\x01fcp") {
            let (f, r) = pcf::read(bytes, &limits)?;
            *self = Self::from_core(f)?;
            self.state.borrow_mut().report = r;
        } else {
            self.load_string(&String::from_utf8_lossy(bytes))?;
        }
        Ok(self)
    }
    /// Create a font with deterministic default headers/properties.
    pub fn create(o: &CreateOptions) -> Result<Self> {
        let descent = o.descent.unwrap_or(o.pixelsize.div_euclid(8));
        let ascent = o.ascent.unwrap_or(o.pixelsize - descent);
        let mut f = CoreFont::default();
        for (k, v) in [
            ("STARTFONT", json!(2.1)),
            ("FONT", json!(o.fontname.as_ref().unwrap_or(&o.familyname))),
            ("SIZE", json!([o.pixelsize, o.xres, o.yres])),
            (
                "FONTBOUNDINGBOX",
                json!([o.pixelsize, ascent + descent, 0, -descent]),
            ),
            ("METRICSSET", json!(0)),
        ] {
            f.headers.push(Property::new(k, property(&v)));
        }
        for (k, v) in [
            ("FAMILY_NAME", json!(o.familyname)),
            ("PIXEL_SIZE", json!(o.pixelsize)),
            ("FONT_ASCENT", json!(ascent)),
            ("FONT_DESCENT", json!(descent)),
            ("CHARSET_REGISTRY", json!("ISO10646")),
            ("CHARSET_ENCODING", json!("1")),
        ] {
            f.properties.push(Property::new(k, property(&v)));
        }
        x11::project(&mut f);
        Self::from_core(f)
    }
    /// Headers projected in SPEC insertion order.
    pub fn headers(&self) -> Map<String, V> {
        headers(&self.state.borrow().core)
    }
    /// Case-normalized properties projected in insertion order.
    pub fn props(&self) -> Map<String, V> {
        props(&self.state.borrow().core).0
    }
    /// Get a case-insensitive property.
    pub fn getprop(&self, key: &str) -> V {
        self.props()
            .get(&key.to_lowercase())
            .cloned()
            .unwrap_or(V::Null)
    }
    /// Set a property, keeping the first spelling when it already exists.
    pub fn setprop(&mut self, key: &str, v: V) -> &mut Self {
        self.state.borrow_mut().codec = None;
        let mut s = self.state.borrow_mut();
        if key.eq_ignore_ascii_case("comment") {
            s.core.properties.remove(key);
            s.core
                .properties
                .push(Property::new("comment", property(&v)));
        } else {
            s.core.properties.set(key, property(&v));
        }
        x11::project(&mut s.core);
        drop(s);
        self
    }
    /// Delete all case-insensitive occurrences of a property.
    pub fn delprop(&mut self, key: &str) -> &mut Self {
        self.state.borrow_mut().codec = None;
        let mut s = self.state.borrow_mut();
        s.core.properties.remove(key);
        x11::project(&mut s.core);
        drop(s);
        self
    }
    /// Resolve the canonical charset, including an explicit override.
    pub fn charset(&self) -> String {
        self.state.borrow().core.charset.canonical_name()
    }
    /// Override charset resolution; an empty name clears the override.
    pub fn encoding(&mut self, name: &str) -> &mut Self {
        self.state.borrow_mut().codec = None;
        self.state.borrow_mut().core.charset.name = if name.is_empty() {
            None
        } else {
            Some(canonical_charset(name))
        };
        self
    }
    pub(super) fn codec(&self) -> Result<CharsetCodec> {
        if let Some(c) = &self.state.borrow().codec {
            return Ok(c.clone());
        }
        let key = self.charset();
        let enc = self.getprop("charset_encoding");
        let enc = if enc.is_null() {
            None
        } else {
            Some(text(&enc))
        };
        let cps = self.itercps(0, &V::Null)?;
        let gl = detect_gl(
            &key,
            enc.as_deref(),
            cps.iter().filter_map(|v| u32::try_from(*v).ok()),
        );
        let codec = CharsetCodec::from_key(&key, Some(gl)).map_err(|e| invalid(&e.to_string()))?;
        if !codec.known() && self.state.borrow_mut().warned.insert(key.clone()) {
            self.warn(format!(
                "Unknown charset '{key}', characters are mapped to codepoints as Unicode"
            ));
        }
        self.state.borrow_mut().codec = Some(codec.clone());
        Ok(codec)
    }
    /// Convert a Unicode number, retaining Python's numeric surrogate identity.
    pub fn unicode2cp(&self, code: u32) -> Result<Option<u32>> {
        let custom = self.state.borrow().char_to_code.clone();
        if let Some(c) = custom {
            return Ok((c.0)(code));
        }
        Ok(self.codec()?.unicode2cp(code))
    }
    /// Convert one Unicode scalar to a legacy numeric code.
    pub fn char2cp(&self, ch: &str) -> Result<Option<u32>> {
        let mut chars = ch.chars();
        let c = chars.next();
        if chars.next().is_some() {
            return Ok(None);
        }
        match c {
            Some(c) => self.unicode2cp(c as u32),
            None => Ok(None),
        }
    }
    /// Convert a numeric code, preserving surrogate values for cross-language serialization.
    pub fn cp2unicode(&self, cp: i64) -> Result<Option<u32>> {
        let custom = self.state.borrow().code_to_char.clone();
        if let Some(c) = custom {
            return Ok(u32::try_from(cp).ok().and_then(|n| (c.0)(n)));
        }
        let codec = self.codec()?;
        Ok(u32::try_from(cp).ok().and_then(|cp| codec.cp2unicode(cp)))
    }
    /// Convert a code to a single Unicode scalar. Use cp2unicode to retain surrogates.
    pub fn cp2char(&self, cp: i64) -> Result<Option<char>> {
        Ok(self.cp2unicode(cp)?.and_then(char::from_u32))
    }
    /// Number of encoded glyphs.
    pub fn length(&self) -> usize {
        self.state
            .borrow()
            .core
            .glyphs()
            .filter(|(_, g)| !g.codepoints.is_empty())
            .count()
    }
    /// Iterate encoded codes: file, reverse-file, ascending or descending order.
    pub fn itercps(&self, order: i64, r: &V) -> Result<Vec<i64>> {
        let mut cps = self
            .state
            .borrow()
            .core
            .glyphs()
            .filter_map(|(_, g)| g.codepoints.first().map(|cp| i64::from(cp.value())))
            .filter(|cp| matches_range(*cp, r))
            .collect::<Vec<_>>();
        match order {
            0 => {}
            -1 => cps.reverse(),
            1 => cps.sort_unstable(),
            2 => {
                cps.sort_unstable();
                cps.reverse();
            }
            _ => return Err(invalid("invalid codepoint ordering")),
        }
        Ok(cps)
    }
    pub(super) fn id(&self, cp: i64) -> Option<GlyphId> {
        let cp = u32::try_from(cp).ok()?;
        let s = self.state.borrow();
        s.core
            .glyph_id(crate::model::Codepoint::Unicode(cp))
            .or_else(|| s.core.glyph_id(crate::model::Codepoint::Legacy(cp)))
    }
    /// Obtain an attached handle, or warn and return None for a missing code.
    pub fn glyphbycp(&self, cp: i64) -> Option<Glyph> {
        let id = self.id(cp);
        if id.is_none() {
            self.warn(format!("Glyph codepoint {cp} does not exist in the font"));
        }
        id.map(|id| Glyph::attached(self, id))
    }
    /// Obtain an attached handle through the font's charset conversion.
    pub fn glyph(&self, ch: &str) -> Result<Option<Glyph>> {
        Ok(self
            .char2cp(ch)?
            .and_then(|cp| self.glyphbycp(i64::from(cp))))
    }
    /// Attached handles in the requested order and range.
    pub fn iterglyphs(&self, order: i64, r: &V) -> Result<Vec<Glyph>> {
        Ok(self
            .itercps(order, r)?
            .into_iter()
            .filter_map(|cp| self.glyphbycp(cp))
            .collect())
    }
    /// Missing characters, including repeats, or None if every character exists.
    pub fn lacksglyphs(&self, text: &str) -> Result<Option<Vec<String>>> {
        let mut out = Vec::new();
        for ch in text.chars() {
            let s = ch.to_string();
            if self
                .char2cp(&s)?
                .is_none_or(|cp| self.id(i64::from(cp)).is_none())
            {
                out.push(s);
            }
        }
        Ok((!out.is_empty()).then_some(out))
    }
    /// Scalable horizontal width with SPEC's floor(v + .5) rounding.
    pub fn swidth(&self, dwx: i64) -> i64 {
        let h = self.headers();
        let p = num(h.get("pointsize").unwrap_or(&V::Null));
        let x = num(h.get("xres").unwrap_or(&V::Null));
        let v = if p != 0 && x != 0 {
            dwx as f64 * 72000.0 / (p as f64 * x as f64)
        } else {
            dwx as f64 * 1000.0 / (num(h.get("fbbx").unwrap_or(&V::Null)).max(1) as f64)
        };
        (v + 0.5).floor() as i64
    }
    pub(super) fn put(&self, mut m: Meta, replace: bool) -> Result<Option<GlyphId>> {
        if m.len() != 17 {
            return Err(invalid("glyph needs 17 slots"));
        }
        let cp = integer(&m[1])?;
        if cp < 0 {
            m[1] = json!(-1);
        }
        let old = self.id(cp);
        if old.is_some() && !replace {
            self.warn(format!(
                "Glyph codepoint {cp} already exists in the font, the old one is kept"
            ));
            return Ok(None);
        }
        let g = core_glyph(&m)?;
        let mut s = self.state.borrow_mut();
        s.codec = None;
        let id = if let Some(id) = old {
            s.core.replace_glyph(id, g)?;
            id
        } else {
            s.core.add_glyph(g)?
        };
        s.serial += 1;
        let serial = s.serial;
        s.generations.insert(id, serial);
        Ok(Some(id))
    }
    /// Add metadata with omitted BBX fields defaulting to zero. Unknown keys fail.
    pub fn addglyph(&mut self, fields: &Map<String, V>, replace: bool) -> Result<&mut Self> {
        let m = self.prepare_meta(fields)?;
        self.put(m, replace)?;
        Ok(self)
    }
    pub(super) fn prepare_meta(&self, fields: &Map<String, V>) -> Result<Meta> {
        for key in fields.keys() {
            if !super::META_TITLES.contains(&key.as_str()) {
                return Err(invalid(&format!("Unknown glyph meta key {key:?}")));
            }
        }
        let mut m = super::META_TITLES
            .iter()
            .map(|k| fields.get(*k).cloned().unwrap_or(V::Null))
            .collect::<Vec<_>>();
        let cp = integer(&m[1])?;
        if m[0].is_null() {
            m[0] = json!(defaultglyphname(cp, is_unicode_charset(&self.charset())));
        }
        for v in &mut m[2..6] {
            if v.is_null() {
                *v = json!(0);
            }
        }
        if m[16].is_null() {
            m[16] = json!([]);
        }
        Ok(m)
    }
    /// Add and attach a handle; replacing an existing slot detaches stale handles.
    pub fn add_handle(&mut self, g: &mut Glyph, replace: bool) -> Result<&mut Self> {
        let fields = super::META_TITLES
            .iter()
            .zip(&g.meta)
            .map(|(k, v)| ((*k).into(), v.clone()))
            .collect();
        if let Some(id) = self.put(self.prepare_meta(&fields)?, replace)? {
            *g = Glyph::attached(self, id);
        }
        Ok(self)
    }
    /// Remove a codepoint; missing codes are a no-op.
    pub fn removeglyph(&mut self, cp: i64) -> &mut Self {
        self.state.borrow_mut().codec = None;
        if let Some(id) = self.id(cp) {
            self.state.borrow_mut().core.remove_glyph(id);
        }
        self
    }
    /// Remove precisely a handle's attached slot, never a subsequent replacement.
    pub fn remove_handle(&mut self, g: &mut Glyph) -> &mut Self {
        if Rc::ptr_eq(&self.state, &g.font.state) && g.ever_attached {
            if g.is_attached()
                && let Some(id) = g.id
            {
                self.state.borrow_mut().core.remove_glyph(id);
            }
        } else if g.cp() >= 0 {
            self.removeglyph(g.cp());
        }
        // A removed attached handle retains its identity guard; a clone is detached.
        g.id = None;
        self
    }
    /// Create an attached glyph, then optionally set its raster (partial errors retain it).
    pub fn newglyph(
        &mut self,
        cp: i64,
        bitmap: Option<&Bitmap>,
        mode: i64,
        name: Option<&str>,
        dwidth: Option<i64>,
    ) -> Result<Glyph> {
        let dw = dwidth.unwrap_or_else(|| {
            bitmap.map_or_else(
                || num(self.headers().get("fbbx").unwrap_or(&V::Null)),
                |b| i64::from(b.width()),
            )
        });
        let fields = json!({"glyphname":name,"codepoint":cp,"bbw":0,"bbh":0,"bbxoff":0,"bbyoff":0,"swx0":self.swidth(dw),"swy0":0,"dwx0":dw,"dwy0":0,"hexdata":[]});
        let m = self.prepare_meta(
            fields
                .as_object()
                .ok_or_else(|| invalid("invalid metadata"))?,
        )?;
        let id = self
            .put(m, true)?
            .ok_or_else(|| invalid("failed to attach glyph"))?;
        let mut g = Glyph::attached(self, id);
        if let Some(b) = bitmap {
            g.setbitmap(b, mode, None, true)?;
        }
        Ok(g)
    }
    /// Replace the FONTBOUNDINGBOX with the union of all positive glyph boxes.
    pub fn updatebbox(&mut self) -> Result<&mut Self> {
        let boxes = self
            .state
            .borrow()
            .core
            .glyphs()
            .map(|(_, g)| meta(g))
            .filter(|m| num(&m[2]) > 0 && num(&m[3]) > 0)
            .collect::<Vec<_>>();
        if !boxes.is_empty() {
            let x0 = boxes.iter().map(|m| num(&m[4])).min().unwrap_or(0);
            let y0 = boxes.iter().map(|m| num(&m[5])).min().unwrap_or(0);
            let x1 = boxes
                .iter()
                .map(|m| num(&m[4]) + num(&m[2]))
                .max()
                .unwrap_or(0);
            let y1 = boxes
                .iter()
                .map(|m| num(&m[5]) + num(&m[3]))
                .max()
                .unwrap_or(0);
            let mut s = self.state.borrow_mut();
            s.core.headers.set(
                "FONTBOUNDINGBOX",
                property(&json!([x1 - x0, y1 - y0, x0, y0])),
            );
            x11::project(&mut s.core);
        }
        Ok(self)
    }
    /// Deep copy of encoded glyphs matching a SPEC range, dropping unencoded ones.
    pub fn subset(&self, r: &V) -> Result<Self> {
        let f = self.clone();
        f.state.borrow_mut().codec = None;
        let remove = f
            .state
            .borrow()
            .core
            .glyphs()
            .filter(|(_, g)| {
                g.codepoints
                    .first()
                    .is_none_or(|c| !matches_range(i64::from(c.value()), r))
            })
            .map(|(id, _)| id)
            .collect::<Vec<_>>();
        for id in remove {
            f.state.borrow_mut().core.remove_glyph(id);
        }
        Ok(f)
    }
    /// Deep-copy encoded glyphs from another font without charset conversion.
    pub fn merge(&mut self, other: &Self, replace: bool) -> Result<&mut Self> {
        let metas = other
            .state
            .borrow()
            .core
            .glyphs()
            .filter(|(_, g)| !g.codepoints.is_empty())
            .map(|(_, g)| meta(g))
            .collect::<Vec<_>>();
        for m in metas {
            self.put(m, replace)?;
        }
        Ok(self)
    }
    /// Apply a bitmap callback in file order, then unencoded order.
    pub fn transform(
        &mut self,
        mut function: impl FnMut(&mut Bitmap) -> Result<()>,
        r: &V,
        mode: i64,
    ) -> Result<&mut Self> {
        let mut ids = self
            .itercps(0, r)?
            .iter()
            .filter_map(|cp| self.id(*cp))
            .collect::<Vec<_>>();
        if r.is_null() {
            ids.extend(
                self.state
                    .borrow()
                    .core
                    .glyphs()
                    .filter(|(_, g)| g.codepoints.is_empty())
                    .map(|(id, _)| id),
            );
        }
        for id in ids {
            Glyph::attached(self, id).transform(&mut function, mode)?;
        }
        Ok(self)
    }
    /// Reencode in source order, dropping unmappable glyphs with one warning.
    pub fn charsetconvert(&mut self, name: &str) -> Result<&mut Self> {
        self.state.borrow_mut().codec = None;
        let old = self.codec()?;
        let override_cp = self.state.borrow().code_to_char.clone();
        let key = canonical_charset(name);
        let info = crate::charset::charset_table()
            .iter()
            .find(|e| e.key == key);
        let target =
            CharsetCodec::from_key(&key, Some(info.is_some_and(|e| e.gl_capable || e.force_gl)))
                .map_err(|e| invalid(&e.to_string()))?;
        let items = self
            .state
            .borrow()
            .core
            .glyphs()
            .filter(|(_, g)| !g.codepoints.is_empty())
            .map(|(id, g)| (id, meta(g)))
            .collect::<Vec<_>>();
        let mut seen = BTreeMap::new();
        let mut dropped = 0;
        for (id, mut m) in items {
            let cp = m[1]
                .as_u64()
                .and_then(|c| u32::try_from(c).ok())
                .and_then(|c| {
                    override_cp
                        .as_ref()
                        .map_or_else(|| old.cp2unicode(c), |f| (f.0)(c))
                })
                .and_then(|c| target.unicode2cp(c));
            if let Some(cp) = cp {
                if let Some(prior) = seen.insert(cp, id) {
                    self.state.borrow_mut().core.remove_glyph(prior);
                    dropped += 1;
                }
                m[1] = json!(cp);
                let g = core_glyph(&m)?;
                self.state.borrow_mut().core.replace_glyph(id, g)?;
            } else {
                self.state.borrow_mut().core.remove_glyph(id);
                dropped += 1;
            }
        }
        if dropped > 0 {
            self.warn(format!(
                "{dropped} glyph(s) could not be converted to the charset '{key}' and were dropped"
            ));
        }
        let (r, e) = charset_props(&key);
        self.setprop("CHARSET_REGISTRY", json!(r))
            .setprop("CHARSET_ENCODING", json!(e))
            .encoding("");
        self.set_converters(None, None);
        Ok(self)
    }
    /// Canonical full model used by the cross-language harness.
    pub fn model(&self) -> V {
        let s = self.state.borrow();
        let h = headers(&s.core);
        let (p, n) = props(&s.core);
        let gs = s
            .core
            .glyphs()
            .filter(|(_, g)| !g.codepoints.is_empty())
            .map(|(_, g)| {
                let m = meta(g);
                json!([m[1], m])
            })
            .collect::<Vec<_>>();
        let us = s
            .core
            .glyphs()
            .filter(|(_, g)| g.codepoints.is_empty())
            .map(|(_, g)| {
                json!([
                    g.properties.get(x11::ALT).map(value).unwrap_or(V::Null),
                    meta(g)
                ])
            })
            .collect::<Vec<_>>();
        let mut out = Map::new();
        out.insert("headers".into(), json!(h.into_iter().collect::<Vec<_>>()));
        out.insert(
            "props".into(),
            json!(
                p.iter()
                    .filter(|(k, _)| *k != "comment")
                    .collect::<Vec<_>>()
            ),
        );
        out.insert(
            "comment".into(),
            p.get("comment").cloned().unwrap_or(V::Null),
        );
        out.insert("propnames".into(), json!(n.into_iter().collect::<Vec<_>>()));
        out.insert("glyphs".into(), json!(gs));
        out.insert("unencoded".into(), json!(us));
        out.insert("charset".into(), json!(s.core.charset.canonical_name()));
        V::Object(out)
    }
    /// Serialize deterministic SPEC BDF, retaining exact raw rows and dynamic metadata.
    pub fn tobdf(&self) -> Result<String> {
        let h = self.headers();
        for key in [
            "pointsize",
            "xres",
            "yres",
            "fbbx",
            "fbby",
            "fbbxoff",
            "fbbyoff",
        ] {
            if h.get(key).is_none_or(V::is_null) {
                return Err(invalid(
                    "Cannot write BDF: missing headers (font not loaded?)",
                ));
            }
        }
        let mut lines = vec![
            format!(
                "STARTFONT {}",
                fmtver(h.get("bdfversion").unwrap_or(&json!(2.1)))
            ),
            format!("FONT {}", h.get("fontname").map(text).unwrap_or_default()),
            format!(
                "SIZE {} {} {}",
                text(h.get("pointsize").unwrap_or(&V::Null)),
                text(h.get("xres").unwrap_or(&V::Null)),
                text(h.get("yres").unwrap_or(&V::Null))
            ),
            format!(
                "FONTBOUNDINGBOX {} {} {} {}",
                text(h.get("fbbx").unwrap_or(&V::Null)),
                text(h.get("fbby").unwrap_or(&V::Null)),
                text(h.get("fbbxoff").unwrap_or(&V::Null)),
                text(h.get("fbbyoff").unwrap_or(&V::Null))
            ),
        ];
        if h.get("metricsset").is_some_and(|v| num(v) != 0) {
            lines.push(format!(
                "METRICSSET {}",
                text(h.get("metricsset").unwrap_or(&V::Null))
            ));
        }
        if let Some(v) = h.get("contentversion") {
            lines.push(format!("CONTENTVERSION {}", text(v)));
        }
        for (k, a, b) in [
            ("SWIDTH", "swx0", "swy0"),
            ("DWIDTH", "dwx0", "dwy0"),
            ("SWIDTH1", "swx1", "swy1"),
            ("DWIDTH1", "dwx1", "dwy1"),
            ("VVECTOR", "vvectorx", "vvectory"),
        ] {
            if let (Some(x), Some(y)) = (h.get(a), h.get(b))
                && !x.is_null()
                && !y.is_null()
            {
                lines.push(format!("{k} {} {}", text(x), text(y)));
            }
        }
        append_comments(&mut lines, h.get("comment"));
        let (p, n) = props(&self.state.borrow().core);
        let count = p.keys().filter(|k| *k != "comment").count();
        if count > 0
            || p.get("comment")
                .is_some_and(|v| !v.is_null() && v != &json!([]) && v != &json!(""))
        {
            lines.push(format!("STARTPROPERTIES {count}"));
            append_comments(&mut lines, p.get("comment"));
            for (k, v) in &p {
                if k == "comment" {
                    continue;
                }
                let name = n.get(k).map(text).unwrap_or_else(|| k.to_uppercase());
                lines.push(if v.is_null() {
                    name
                } else if v.is_number() {
                    format!("{name} {v}")
                } else {
                    format!("{name} \"{}\"", text(v).replace('"', "\"\""))
                });
            }
            lines.push("ENDPROPERTIES".into());
        }
        let s = self.state.borrow();
        let mut glyphs = s
            .core
            .glyphs()
            .filter(|(_, g)| !g.codepoints.is_empty())
            .collect::<Vec<_>>();
        glyphs.extend(s.core.glyphs().filter(|(_, g)| g.codepoints.is_empty()));
        lines.push(format!("CHARS {}", glyphs.len()));
        for (_, g) in glyphs {
            let m = meta(g);
            lines.push(format!("STARTCHAR {}", text(&m[0])));
            lines.push(if num(&m[1]) < 0 {
                g.properties
                    .get(x11::ALT)
                    .map(|v| format!("ENCODING -1 {}", text(&value(v))))
                    .unwrap_or("ENCODING -1".into())
            } else {
                format!("ENCODING {}", text(&m[1]))
            });
            for (k, i) in [
                ("SWIDTH", 6),
                ("DWIDTH", 8),
                ("SWIDTH1", 10),
                ("DWIDTH1", 12),
                ("VVECTOR", 14),
            ] {
                if !m[i].is_null() && !m[i + 1].is_null() {
                    lines.push(format!("{k} {} {}", text(&m[i]), text(&m[i + 1])));
                }
            }
            lines.push(format!(
                "BBX {} {} {} {}",
                text(&m[2]),
                text(&m[3]),
                text(&m[4]),
                text(&m[5])
            ));
            lines.push("BITMAP".into());
            if let Some(rows) = m[16].as_array() {
                for r in rows {
                    lines.push(
                        r.as_str()
                            .ok_or_else(|| invalid("hex row must be text"))?
                            .into(),
                    );
                }
            }
            lines.push("ENDCHAR".into());
        }
        lines.push("ENDFONT".into());
        Ok(lines.join("\n") + "\n")
    }
    /// Write PCF using the native, bounded deterministic codec.
    pub fn topcf(&self, o: &pcf::PcfWriteOptions) -> Result<Vec<u8>> {
        let mut core = self.core();
        let ids = core.glyphs().map(|(id, _)| id).collect::<Vec<_>>();
        for id in ids {
            core.edit_glyph(id, |g| {
                let m = meta(g);
                for cp in &mut g.codepoints {
                    *cp = crate::model::Codepoint::Legacy(cp.value());
                }
                for (i, key) in [(6, "SWIDTH"), (8, "DWIDTH")] {
                    if let Some(x) = m[i].as_i64() {
                        x11::set_vector(
                            &mut g.metrics,
                            key,
                            crate::model::Vector::new(x, num(&m[i + 1])),
                        );
                    }
                }
                Ok(())
            })?;
        }
        core.charset.name = None;
        core.charset.gl = None;
        x11::project(&mut core);
        x11::derive(&mut core);
        core.headers.remove(x11::SOURCE_HEADERS);
        core.headers.remove(x11::SOURCE_PROPS);
        x11::save_snapshot(&mut core);
        Ok(pcf::write(&core, o)?.0)
    }
    /// Draw explicit codepoints through the native packed layout engine.
    pub fn drawcps(&self, cps: &[i64], kw: &Map<String, V>) -> Result<Bitmap> {
        let mut core = self.core();
        x11::project(&mut core);
        let ids = core.glyphs().map(|(id, _)| id).collect::<Vec<_>>();
        for id in ids {
            core.edit_glyph(id, |g| {
                let m = meta(g);
                for (i, key) in [(8, "DWIDTH"), (12, "DWIDTH1")] {
                    let first = m[i].as_i64().filter(|x| *x != 0);
                    if let Some(n) = first.or_else(|| m[i + 1].as_i64()) {
                        x11::set_vector(&mut g.metrics, key, crate::model::Vector::new(n, 0));
                    }
                }
                Ok(())
            })?;
        }
        let key = self.charset();
        core.charset.name = Some(key);
        core.charset.gl = Some(self.codec()?.gl());
        let missing = kw
            .get("missing")
            .filter(|v| !v.is_null())
            .map(|v| {
                self.prepare_meta(
                    v.as_object()
                        .ok_or_else(|| invalid("missing glyph must be metadata"))?,
                )
                .and_then(|m| core_glyph(&m))
            })
            .transpose()?;
        let options = RenderOptions {
            line_limit: kw.get("linelimit").map(num).unwrap_or(512),
            mode: if kw.get("mode").map(num).unwrap_or(1) == 1 {
                RenderMode::Advance
            } else {
                RenderMode::Cell
            },
            direction: WritingDirection::from_spec(
                kw.get("direction").and_then(V::as_str).unwrap_or("lr"),
            ),
            use_current_glyph_spacing: kw.get("ucgs").and_then(V::as_bool).unwrap_or(false),
            missing: missing
                .as_ref()
                .map(MissingGlyph::Glyph)
                .unwrap_or_default(),
            ..RenderOptions::default()
        };
        let codes = cps.iter().map(|n| {
            let n = u32::try_from(*n).ok()?;
            // Match codepoints()' derived domain order, including raw Unicode
            // numbers: Unicode precedes Legacy, regardless of insertion order.
            [Codepoint::Unicode(n), Codepoint::Legacy(n)]
                .into_iter()
                .find(|cp| core.glyph_id(*cp).is_some())
        });
        let rendered = core.draw_codepoints(codes, &options)?;
        Ok(Bitmap::from_raster(rendered.bitmap.to_gray8_palette(0, 1)?))
    }
    /// Draw text after charset conversion.
    pub fn draw(&self, text: &str, kw: &Map<String, V>) -> Result<Bitmap> {
        let cps = text
            .chars()
            .map(|c| {
                self.char2cp(&c.to_string())
                    .map(|v| v.map(i64::from).unwrap_or(-1))
            })
            .collect::<Result<Vec<_>>>()?;
        self.drawcps(&cps, kw)
    }
    /// Draw all filtered encoded glyphs.
    pub fn drawall(&self, kw: &Map<String, V>) -> Result<Bitmap> {
        let mut opts = kw.clone();
        opts.entry("mode").or_insert(json!(0));
        self.drawcps(
            &self.itercps(
                kw.get("order").map(num).unwrap_or(1),
                kw.get("r").unwrap_or(&V::Null),
            )?,
            &opts,
        )
    }
}
pub(super) fn matches_range(cp: i64, r: &V) -> bool {
    match r {
        V::Null => true,
        V::Number(_) => cp < num(r),
        V::Array(a) if a.len() == 2 && a[0].is_number() && a[1].is_number() => {
            num(&a[0]) <= cp && cp <= num(&a[1])
        }
        V::Array(a) => a.iter().any(|v| matches_range(cp, v)),
        _ => false,
    }
}
fn append_comments(lines: &mut Vec<String>, v: Option<&V>) {
    match v {
        Some(V::Array(a)) => lines.extend(a.iter().map(|v| format!("COMMENT \"{}\"", text(v)))),
        Some(V::String(s)) => lines.extend(s.chars().map(|c| format!("COMMENT \"{c}\""))),
        _ => {}
    }
}
fn fmtver(v: &V) -> String {
    let n = v.as_f64().unwrap_or(2.1);
    if n.fract() == 0.0 {
        format!("{n:.1}")
    } else {
        n.to_string()
    }
}
impl Font {
    pub(super) fn from_parts(
        h: &Map<String, V>,
        p: &Map<String, V>,
        names: &Map<String, V>,
    ) -> Result<Self> {
        let mut core = CoreFont::default();
        let mut emitted = BTreeSet::new();
        for (k, v) in h {
            let (key, group) = match k.as_str() {
                "bdfversion" => ("STARTFONT", vec!["bdfversion"]),
                "fontname" => ("FONT", vec!["fontname"]),
                "pointsize" | "xres" | "yres" => ("SIZE", vec!["pointsize", "xres", "yres"]),
                "fbbx" | "fbby" | "fbbxoff" | "fbbyoff" => (
                    "FONTBOUNDINGBOX",
                    vec!["fbbx", "fbby", "fbbxoff", "fbbyoff"],
                ),
                "swx0" | "swy0" => ("SWIDTH", vec!["swx0", "swy0"]),
                "dwx0" | "dwy0" => ("DWIDTH", vec!["dwx0", "dwy0"]),
                "swx1" | "swy1" => ("SWIDTH1", vec!["swx1", "swy1"]),
                "dwx1" | "dwy1" => ("DWIDTH1", vec!["dwx1", "dwy1"]),
                "vvectorx" | "vvectory" => ("VVECTOR", vec!["vvectorx", "vvectory"]),
                "metricsset" => ("METRICSSET", vec!["metricsset"]),
                "contentversion" => ("CONTENTVERSION", vec!["contentversion"]),
                "comment" => ("COMMENT", vec!["comment"]),
                _ => continue,
            };
            if !emitted.insert(key) {
                continue;
            }
            let value = if group.len() == 1 {
                v.clone()
            } else {
                V::Array(
                    group
                        .iter()
                        .map(|k| h.get(*k).cloned().unwrap_or(V::Null))
                        .collect(),
                )
            };
            core.headers.push(Property::new(key, property(&value)));
        }
        for (k, v) in p {
            let name = names
                .get(k)
                .and_then(V::as_str)
                .map(str::to_owned)
                .unwrap_or(k.to_uppercase());
            core.properties.push(Property::new(name, property(v)));
        }
        x11::project(&mut core);
        Self::from_core(core)
    }
    pub(super) fn put_with_alt(&mut self, m: Meta, alt: V) -> Result<()> {
        if let Some(id) = self.put(m, true)?
            && !alt.is_null()
        {
            let mut s = self.state.borrow_mut();
            s.core.edit_glyph(id, |g| {
                g.properties.set(x11::ALT, property(&alt));
                Ok(())
            })?;
        }
        Ok(())
    }
    /// Load a glyph-based TrueType font, using its BDFP metadata or requested size.
    pub fn load_ttf(&mut self, bytes: &[u8], pixelsize: Option<i64>) -> Result<&mut Self> {
        *self = super::ttf::read_ttf(bytes, pixelsize)?;
        Ok(self)
    }
}

impl Font {
    /// Override Unicode-to-code and code-to-Unicode functions independently.
    /// Passing None restores the font's charset table in that direction.
    pub fn set_converters(
        &mut self,
        char_to_code: Option<Converter>,
        code_to_char: Option<Converter>,
    ) -> &mut Self {
        let mut s = self.state.borrow_mut();
        s.char_to_code = char_to_code;
        s.code_to_char = code_to_char;
        drop(s);
        self
    }
}
