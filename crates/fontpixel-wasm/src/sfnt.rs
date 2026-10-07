//! `sfnt`: the bounded sfnt container, TrueType collections, CFF reading and
//! writing, and Unicode subsetting (`fontpixel::sfnt`).
//!
//! The Rust parsers borrow the font bytes (`Sfnt<'a>`, `Collection<'a>`,
//! `Cff<'a>`). The JS handles (`Sfnt`, `Collection`, `Cff`) instead own a
//! copy of the input and the limits used to validate it, and re-run the same
//! parser for every call (parsing reads only the directory, so this costs
//! microseconds and needs no self-referential storage), exactly as the
//! Python binding does. Table records and collection faces are plain objects
//! whose `data`/`signature` are fresh `Uint8Array`s.
//!
//! Functions live in namespaces mirroring the Rust modules:
//! `sfnt.checksum`, `sfnt.collection.encode`, `sfnt.cff.encode`,
//! `sfnt.subset.subset`, …; the table codecs are in [`crate::sfnt_tables`].
//! Results that Python returns as tuples are objects here
//! (`{bytes, report}`).
use crate::conv;
use crate::errors::{IntoJs, JsResult};
use crate::handles::borrow_mut;
use crate::sfnt_support::{
    byte_result, bytes_js, handle_of, int, mono, object, offer, opt, req, tag_arg, tag_str,
    with_bytes,
};
use fontpixel::sfnt::{self, Flavor, Sfnt, SfntBuilder, TableRecord, cff, subset};
use js_sys::{Array, Uint8Array};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const SFNT_TS: &str = r#"
/** A four-byte OpenType tag: four Latin-1 characters (`"OS/2"`) or four bytes. */
export type TagLike = string | BytesLike;

/** `T` with the byte fields `K` as `Uint8Array`s (results of the sfnt/woff codecs). */
export type BytesFields<T, K extends keyof T> = Omit<T, K> & { [P in K]: Uint8Array };

/** A table directory record (`fontpixel::sfnt::TableRecord`); `data` is a copy of the unpadded table. */
export interface SfntTableRecord {
  tag: string;
  checksum: number;
  offset: number;
  data: Uint8Array;
}

/** A collection face (`fontpixel::sfnt::collection::Face`). */
export interface CollectionFace {
  flavor: Flavor;
  offset: number;
  tables: SfntTableRecord[];
}

/** Output bytes with the operation's report. */
export interface BytesWithReport {
  bytes: Uint8Array;
  report: Report;
}

/** A TrueType contour (`fontpixel::outline::Contour`): `[x, y, onCurve]` points. */
export type GlyfContour = [number, number, boolean][];

/** `fontpixel::sfnt::subset::Subset` with `bytes` as a `Uint8Array`. */
export type SubsetResult = BytesFields<Subset, "bytes">;
"#;

// ---------------------------------------------------------------- container

/// The bytes and limits an `Sfnt` handle owns.
pub struct SfntData {
    /// The font bytes.
    pub data: Vec<u8>,
    /// Limits used for parsing.
    pub limits: sfnt::Limits,
}

impl SfntData {
    /// Run `f` on a fresh parse of the owned bytes.
    pub fn with<R>(&self, f: impl FnOnce(&Sfnt<'_>) -> JsResult<R>) -> JsResult<R> {
        let s = Sfnt::parse(&self.data, &self.limits).js()?;
        f(&s)
    }
}

/// The plain record of a table (`{tag, checksum, offset, data}`).
fn record(t: &TableRecord<'_>) -> JsResult<JsValue> {
    object(&[
        ("tag", JsValue::from_str(&tag_str(t.tag))),
        ("checksum", JsValue::from_f64(f64::from(t.checksum))),
        ("offset", JsValue::from_f64(f64::from(t.offset))),
        ("data", bytes_js(t.data)),
    ])
}

fn records(tables: &[TableRecord<'_>]) -> JsResult<Array> {
    tables.iter().map(record).collect()
}

/// A parsed standalone sfnt (`fontpixel::sfnt::Sfnt`).
///
/// The handle owns a copy of the input bytes and the `SfntLimits` it was
/// validated with; every method re-parses the directory from them.
#[wasm_bindgen(js_name = Sfnt)]
pub struct JsSfnt {
    pub(crate) inner: Rc<SfntData>,
}

impl JsSfnt {
    fn create(data: &JsValue, limits: Option<JsValue>) -> JsResult<Self> {
        let data = conv::bytes_from_js(data, "data")?;
        let limits: sfnt::Limits = opt(limits)?;
        Sfnt::parse(&data, &limits).js()?;
        Ok(Self {
            inner: Rc::new(SfntData { data, limits }),
        })
    }
}

/// The `SfntData` of an `Sfnt` handle argument.
pub fn sfnt_arg(value: &JsValue) -> JsResult<Rc<SfntData>> {
    handle_of::<Rc<SfntData>>(value, "Sfnt")?
        .ok_or_else(|| crate::errors::invalid("expected an Sfnt handle"))
}

#[wasm_bindgen(js_class = Sfnt)]
impl JsSfnt {
    /// Parse a standalone sfnt (same as `Sfnt.parse(data, limits)`).
    #[wasm_bindgen(constructor)]
    pub fn constructor(
        #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "SfntLimits")] limits: Option<JsValue>,
    ) -> JsResult<JsSfnt> {
        Self::create(&data, limits)
    }
    /// Parse a standalone sfnt directory, checking table bounds and limits
    /// (`Sfnt::parse`). `limits` is an `SfntLimits` object (see
    /// `sfnt.defaults()`); checksums are checked by `validateChecksums()`.
    pub fn parse(
        #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "SfntLimits")] limits: Option<JsValue>,
    ) -> JsResult<JsSfnt> {
        Self::create(&data, limits)
    }
    /// A copy of the font bytes this handle owns.
    #[wasm_bindgen(getter)]
    pub fn data(&self) -> Uint8Array {
        byte_result(&self.inner.data)
    }
    /// The limits used for parsing.
    #[wasm_bindgen(getter, unchecked_return_type = "SfntLimits")]
    pub fn limits(&self) -> JsResult<JsValue> {
        conv::to_js(&self.inner.limits)
    }
    /// Outline flavor: `"true_type"`, `"cff"` or `"apple_true_type"`.
    #[wasm_bindgen(getter, unchecked_return_type = "Flavor")]
    pub fn flavor(&self) -> JsResult<JsValue> {
        self.inner.with(|s| conv::to_js(&s.flavor()))
    }
    /// Directory records in tag order (`Sfnt::tables`).
    #[wasm_bindgen(unchecked_return_type = "SfntTableRecord[]")]
    pub fn tables(&self) -> JsResult<Array> {
        self.inner.with(|s| records(s.tables()))
    }
    /// A copy of table `tag` (`"OS/2"` or four bytes), or `undefined`.
    pub fn table(
        &self,
        #[wasm_bindgen(unchecked_param_type = "TagLike")] tag: JsValue,
    ) -> JsResult<Option<Uint8Array>> {
        let t = tag_arg(&tag, "tag")?;
        self.inner.with(|s| Ok(s.table(t).map(byte_result)))
    }
    /// Check every table checksum and the head checksum adjustment
    /// (`InvalidInputError` on a mismatch).
    #[wasm_bindgen(js_name = validateChecksums)]
    pub fn validate_checksums(&self) -> JsResult<()> {
        self.inner.with(|s| s.validate_checksums().js())
    }
    /// `Sfnt(true_type, tables=11, bytes=1234)`.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> JsResult<String> {
        self.inner.with(|s| {
            let flavor = serde_json::to_value(s.flavor())
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default();
            Ok(format!(
                "Sfnt({flavor}, tables={}, bytes={})",
                s.tables().len(),
                self.inner.data.len()
            ))
        })
    }
    /// Internal: share this handle's storage with the running call.
    #[wasm_bindgen(js_name = __fpShareStorage, skip_typescript)]
    pub fn share_storage(&self) -> JsResult<()> {
        offer(Box::new(self.inner.clone()));
        Ok(())
    }
}

/// Assembles a standalone sfnt (`fontpixel::sfnt::SfntBuilder`).
#[wasm_bindgen(js_name = SfntBuilder)]
pub struct JsSfntBuilder {
    builder: RefCell<SfntBuilder>,
}

#[wasm_bindgen(js_class = SfntBuilder)]
impl JsSfntBuilder {
    /// Start an empty font of `flavor` (`"true_type"`, `"cff"`,
    /// `"apple_true_type"`).
    #[wasm_bindgen(constructor)]
    pub fn constructor(
        #[wasm_bindgen(unchecked_param_type = "Flavor")] flavor: JsValue,
    ) -> JsResult<JsSfntBuilder> {
        Ok(Self {
            builder: RefCell::new(SfntBuilder::new(req::<Flavor>(&flavor)?)),
        })
    }
    /// Add table `tag` with a copy of `table`; duplicate or malformed tags
    /// throw `InvalidInputError`.
    pub fn insert(
        &self,
        #[wasm_bindgen(unchecked_param_type = "TagLike")] tag: JsValue,
        #[wasm_bindgen(unchecked_param_type = "BytesLike")] table: JsValue,
    ) -> JsResult<()> {
        let t = tag_arg(&tag, "tag")?;
        let data = conv::bytes_from_js(&table, "table")?;
        borrow_mut(&self.builder, "SfntBuilder")?
            .insert(t, data)
            .js()
    }
    /// Build the font with sorted tables, checksums and head adjustment.
    pub fn build(
        &self,
        #[wasm_bindgen(unchecked_optional_param_type = "SfntLimits")] limits: Option<JsValue>,
    ) -> JsResult<Uint8Array> {
        let l: sfnt::Limits = opt(limits)?;
        let bytes = crate::handles::borrow(&self.builder, "SfntBuilder")?
            .build(&l)
            .js()?;
        Ok(byte_result(&bytes))
    }
}

/// `fontpixel::sfnt::checksum`: the wrapping sum of big-endian 32-bit
/// words (a short last word is zero padded).
#[wasm_bindgen(js_namespace = sfnt, js_name = checksum)]
pub fn sfnt_checksum(
    #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
) -> JsResult<u32> {
    Ok(sfnt::checksum(&conv::bytes_from_js(&data, "data")?))
}

/// `Flavor::version`: the four-byte sfnt version of a flavor.
#[wasm_bindgen(js_namespace = sfnt, js_name = flavorVersion)]
pub fn flavor_version(
    #[wasm_bindgen(unchecked_param_type = "Flavor")] flavor: JsValue,
) -> JsResult<u32> {
    Ok(req::<Flavor>(&flavor)?.version())
}

/// Default limit and option objects of `sfnt` and its namespaces, keyed by
/// Rust type path relative to `fontpixel::sfnt` (`"Limits"`,
/// `"collection.Limits"`, `"glyf.Limits"`, `"cff.Limits"`,
/// `"cff.DotOptions"`, `"bitmap.Limits"`, `"gsub.LayoutLimits"`,
/// `"subset.Options"`; `collection.Limits` only where collections are
/// built in).
#[wasm_bindgen(js_namespace = sfnt, js_name = defaults, unchecked_return_type = "Record<string, unknown>")]
pub fn sfnt_defaults() -> JsResult<JsValue> {
    use fontpixel::sfnt::{glyf, gsub};
    let mut items = vec![("Limits", conv::to_js(&sfnt::Limits::default())?)];
    #[cfg(feature = "all-formats")]
    items.push((
        "collection.Limits",
        conv::to_js(&sfnt::collection::Limits::default())?,
    ));
    items.extend([
        ("glyf.Limits", conv::to_js(&glyf::Limits::default())?),
        ("cff.Limits", conv::to_js(&cff::Limits::default())?),
        ("cff.DotOptions", conv::to_js(&cff::DotOptions::default())?),
        (
            "bitmap.Limits",
            conv::to_js(&sfnt::bitmap::Limits::default())?,
        ),
        (
            "gsub.LayoutLimits",
            conv::to_js(&gsub::LayoutLimits::default())?,
        ),
        ("subset.Options", conv::to_js(&subset::Options::default())?),
    ]);
    object(&items)
}

// ---------------------------------------------------------------- collection

#[cfg(feature = "all-formats")]
mod collection_api {
    use super::*;
    use fontpixel::sfnt::collection::{self, Collection};

    fn bytes_report(bytes: &[u8], report: &fontpixel::Report) -> JsResult<JsValue> {
        object(&[("bytes", bytes_js(bytes)), ("report", conv::to_js(report)?)])
    }

    /// A parsed TrueType/OpenType collection
    /// (`fontpixel::sfnt::collection::Collection`).
    ///
    /// Owns a copy of the bytes and the limits; each call re-parses (every
    /// face is checked).
    #[wasm_bindgen(js_name = Collection)]
    pub struct JsCollection {
        data: Vec<u8>,
        limits: collection::Limits,
    }

    impl JsCollection {
        fn with<R>(&self, f: impl FnOnce(&Collection<'_>) -> JsResult<R>) -> JsResult<R> {
            let c = Collection::parse(&self.data, &self.limits).js()?;
            f(&c)
        }
        fn create(data: &JsValue, limits: Option<JsValue>) -> JsResult<Self> {
            let data = conv::bytes_from_js(data, "data")?;
            let limits: collection::Limits = opt(limits)?;
            Collection::parse(&data, &limits).js()?;
            Ok(Self { data, limits })
        }
    }

    #[wasm_bindgen(js_class = Collection)]
    impl JsCollection {
        /// Parse a `ttcf` collection (same as `Collection.parse(data, limits)`).
        #[wasm_bindgen(constructor)]
        pub fn constructor(
            #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
            #[wasm_bindgen(unchecked_optional_param_type = "CollectionLimits")] limits: Option<
                JsValue,
            >,
        ) -> JsResult<JsCollection> {
            Self::create(&data, limits)
        }
        /// Parse a `ttcf` collection, checking every face before exposing any
        /// (`Collection::parse`).
        pub fn parse(
            #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
            #[wasm_bindgen(unchecked_optional_param_type = "CollectionLimits")] limits: Option<
                JsValue,
            >,
        ) -> JsResult<JsCollection> {
            Self::create(&data, limits)
        }
        /// A copy of the collection bytes this handle owns.
        #[wasm_bindgen(getter)]
        pub fn data(&self) -> Uint8Array {
            byte_result(&self.data)
        }
        /// TTC version (`0x00010000` or `0x00020000`).
        #[wasm_bindgen(getter)]
        pub fn version(&self) -> JsResult<u32> {
            self.with(|c| Ok(c.version))
        }
        /// Faces as `{flavor, offset, tables}` with records as in `Sfnt.tables()`.
        #[wasm_bindgen(getter, unchecked_return_type = "CollectionFace[]")]
        pub fn faces(&self) -> JsResult<Array> {
            self.with(|c| {
                c.faces
                    .iter()
                    .map(|f| {
                        object(&[
                            ("flavor", conv::to_js(&f.flavor)?),
                            ("offset", JsValue::from_f64(f64::from(f.offset))),
                            ("tables", records(&f.tables)?.into()),
                        ])
                    })
                    .collect()
            })
        }
        /// The opaque version-2 DSIG block, or `undefined`.
        #[wasm_bindgen(getter)]
        pub fn signature(&self) -> JsResult<Option<Uint8Array>> {
            self.with(|c| Ok(c.signature.map(byte_result)))
        }
        /// Check every face's table checksums and head adjustment.
        #[wasm_bindgen(js_name = validateChecksums)]
        pub fn validate_checksums(&self) -> JsResult<()> {
            self.with(|c| c.validate_checksums().js())
        }
        /// Extract face `index` as a standalone sfnt: `{bytes, report}`
        /// (`Collection::extract`; `limits` defaults to the parse limits).
        #[wasm_bindgen(unchecked_return_type = "BytesWithReport")]
        pub fn extract(
            &self,
            index: f64,
            #[wasm_bindgen(unchecked_optional_param_type = "CollectionLimits")] limits: Option<
                JsValue,
            >,
        ) -> JsResult<JsValue> {
            let index: usize = int(index, "index")?;
            let l = match limits {
                Some(l) if !l.is_undefined() && !l.is_null() => req(&l)?,
                _ => self.limits.clone(),
            };
            self.with(|c| {
                let (bytes, report) = c.extract(index, &l).js()?;
                bytes_report(&bytes, &report)
            })
        }
    }

    /// `collection::encode`: build a collection (`version` `0x10000` or
    /// `0x20000`) from standalone fonts, sharing identical tables. Returns
    /// `{bytes, report}`.
    #[wasm_bindgen(js_namespace = ["sfnt", "collection"], js_name = encode, unchecked_return_type = "BytesWithReport")]
    pub fn collection_encode(
        #[wasm_bindgen(unchecked_param_type = "BytesLike[]")] fonts: JsValue,
        version: f64,
        #[wasm_bindgen(unchecked_optional_param_type = "CollectionLimits")] limits: Option<JsValue>,
    ) -> JsResult<JsValue> {
        let l: collection::Limits = opt(limits)?;
        let version: u32 = int(version, "version")?;
        let inputs = byte_list(&fonts, "fonts")?;
        let slices: Vec<&[u8]> = inputs.iter().map(Vec::as_slice).collect();
        let (bytes, report) = collection::encode(&slices, version, &l).js()?;
        bytes_report(&bytes, &report)
    }
}
#[cfg(feature = "all-formats")]
pub use collection_api::*;

/// A list of byte buffers (array of `BytesLike`).
pub fn byte_list(value: &JsValue, what: &str) -> JsResult<Vec<Vec<u8>>> {
    let list = value
        .dyn_ref::<Array>()
        .ok_or_else(|| crate::errors::invalid(format!("{what}: expected an array of bytes")))?;
    list.iter()
        .enumerate()
        .map(|(i, b)| conv::bytes_from_js(&b, &format!("{what}[{i}]")))
        .collect()
}

// ---------------------------------------------------------------- CFF

enum CffSource {
    Raw(Vec<u8>, u16),
    Sfnt(Rc<SfntData>),
}

/// A parsed CFF1 table (`fontpixel::sfnt::cff::Cff`).
///
/// Owns the table (or the containing `Sfnt`'s storage) and the limits; each
/// call re-parses.
#[wasm_bindgen(js_name = Cff)]
pub struct JsCff {
    source: CffSource,
    limits: cff::Limits,
}

impl JsCff {
    fn with<R>(&self, f: impl FnOnce(&cff::Cff<'_>) -> JsResult<R>) -> JsResult<R> {
        match &self.source {
            CffSource::Raw(d, upm) => f(&cff::Cff::parse(d, *upm, &self.limits).js()?),
            CffSource::Sfnt(data) => data.with(|s| f(&cff::Cff::from_sfnt(s, &self.limits).js()?)),
        }
    }
    fn limits_or(&self, limits: Option<JsValue>) -> JsResult<cff::Limits> {
        match limits {
            Some(l) if !l.is_undefined() && !l.is_null() => req(&l),
            _ => Ok(self.limits.clone()),
        }
    }
    fn checked(self) -> JsResult<Self> {
        self.with(|_| Ok(()))?;
        Ok(self)
    }
}

#[wasm_bindgen(js_class = Cff)]
impl JsCff {
    /// Parse a raw `CFF ` table with the containing font's units per em
    /// (`Cff::parse`).
    pub fn parse(
        #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
        #[wasm_bindgen(js_name = unitsPerEm)] units_per_em: f64,
        #[wasm_bindgen(unchecked_optional_param_type = "CffLimits")] limits: Option<JsValue>,
    ) -> JsResult<JsCff> {
        Self {
            source: CffSource::Raw(
                conv::bytes_from_js(&data, "data")?,
                int(units_per_em, "unitsPerEm")?,
            ),
            limits: opt(limits)?,
        }
        .checked()
    }
    /// The `CFF ` table of an `OTTO` `Sfnt` handle (requires maxp 0.5 and
    /// head; `Cff::from_sfnt`).
    #[wasm_bindgen(js_name = fromSfnt)]
    pub fn from_sfnt(
        font: &JsSfnt,
        #[wasm_bindgen(unchecked_optional_param_type = "CffLimits")] limits: Option<JsValue>,
    ) -> JsResult<JsCff> {
        Self {
            source: CffSource::Sfnt(font.inner.clone()),
            limits: opt(limits)?,
        }
        .checked()
    }
    /// A copy of the CFF table bytes.
    #[wasm_bindgen(getter)]
    pub fn data(&self) -> JsResult<Uint8Array> {
        self.with(|c| Ok(byte_result(c.data())))
    }
    /// Number of glyphs.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> JsResult<usize> {
        self.with(|c| Ok(c.len()))
    }
    /// Number of glyphs (`Cff::len`).
    pub fn len(&self) -> JsResult<usize> {
        self.with(|c| Ok(c.len()))
    }
    /// Whether the font has no glyphs.
    #[wasm_bindgen(js_name = isEmpty)]
    pub fn is_empty(&self) -> JsResult<bool> {
        self.with(|c| Ok(c.is_empty()))
    }
    /// Whether this is a CID-keyed font.
    #[wasm_bindgen(js_name = isCid)]
    pub fn is_cid(&self) -> JsResult<bool> {
        self.with(|c| Ok(c.is_cid()))
    }
    /// The glyph name bytes of a name-keyed font, or `undefined`.
    #[wasm_bindgen(js_name = glyphName)]
    pub fn glyph_name(&self, gid: f64) -> JsResult<Option<Uint8Array>> {
        let gid: u16 = int(gid, "gid")?;
        self.with(|c| Ok(c.glyph_name(gid).map(byte_result)))
    }
    /// The SID or CID of glyph `gid`, or `undefined`.
    pub fn identifier(&self, gid: f64) -> JsResult<Option<u16>> {
        let gid: u16 = int(gid, "gid")?;
        self.with(|c| Ok(c.identifier(gid)))
    }
    /// Number of global subroutines.
    #[wasm_bindgen(js_name = globalSubrCount)]
    pub fn global_subr_count(&self) -> JsResult<usize> {
        self.with(|c| Ok(c.global_subr_count()))
    }
    /// Evaluate one glyph: `{advance, commands}` (`DecodedGlyph`).
    #[wasm_bindgen(unchecked_return_type = "DecodedGlyph")]
    pub fn glyph(
        &self,
        gid: f64,
        #[wasm_bindgen(unchecked_optional_param_type = "CffLimits")] limits: Option<JsValue>,
    ) -> JsResult<JsValue> {
        let gid: u16 = int(gid, "gid")?;
        let l = self.limits_or(limits)?;
        self.with(|c| conv::to_js(&c.glyph(gid, &l).js()?))
    }
    /// Evaluate every glyph under one whole-font budget.
    #[wasm_bindgen(js_name = readAll, unchecked_return_type = "DecodedGlyph[]")]
    pub fn read_all(
        &self,
        #[wasm_bindgen(unchecked_optional_param_type = "CffLimits")] limits: Option<JsValue>,
    ) -> JsResult<JsValue> {
        let l = self.limits_or(limits)?;
        self.with(|c| conv::to_js(&c.read_all(&l).js()?))
    }
    /// Subset to `glyphs` (old IDs in new order, `.notdef` first): CFF bytes.
    pub fn subset(
        &self,
        #[wasm_bindgen(unchecked_param_type = "number[]")] glyphs: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "CffLimits")] limits: Option<JsValue>,
    ) -> JsResult<Uint8Array> {
        let glyphs: Vec<u16> = req(&glyphs)?;
        let l = self.limits_or(limits)?;
        self.with(|c| Ok(byte_result(&c.subset(&glyphs, &l).js()?)))
    }
}

/// `cff::Font::encode`: encode a CFF `Font` (`CffFont` plain data); returns
/// `{data, bounds, maxp}` with `data`/`maxp` as `Uint8Array`s.
#[wasm_bindgen(js_namespace = ["sfnt", "cff"], js_name = encode, unchecked_return_type = "BytesFields<CffEncoded, \"data\" | \"maxp\">")]
pub fn cff_encode(
    #[wasm_bindgen(unchecked_param_type = "CffFont")] font: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "CffLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let f: cff::Font = req(&font)?;
    let l: cff::Limits = opt(limits)?;
    let mut e = f.encode(&l).js()?;
    let data = std::mem::take(&mut e.data);
    let maxp = std::mem::take(&mut e.maxp);
    with_bytes(&e, &[("data", &data), ("maxp", &maxp)])
}

/// `cff::Glyph::from_bitmap`: a CFF glyph (`SfntCffGlyph`) from a Mono
/// `Bitmap` handle or `GlyphBitmap` plain data; `options` is an outline `Options`
/// object.
#[wasm_bindgen(js_namespace = ["sfnt", "cff"], js_name = glyphFromBitmap, unchecked_return_type = "SfntCffGlyph")]
pub fn glyph_from_bitmap(
    name: String,
    advance: f64,
    #[wasm_bindgen(unchecked_param_type = "Bitmap | GlyphBitmap")] bitmap: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "OutlineOptions")] options: Option<JsValue>,
    #[wasm_bindgen(unchecked_optional_param_type = "CffLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let b = mono(&bitmap)?;
    let o: fontpixel::outline::Options = opt(options)?;
    let l: cff::Limits = opt(limits)?;
    conv::to_js(&cff::Glyph::from_bitmap(name, advance, &b, &o, &l).js()?)
}

// ---------------------------------------------------------------- subset

/// `subset::subset`: subset a standalone sfnt. Returns `{bytes,
/// glyph_mapping, report}`; `glyph_mapping` maps old to new glyph IDs.
#[wasm_bindgen(js_namespace = ["sfnt", "subset"], js_name = subset, unchecked_return_type = "SubsetResult")]
pub fn subset_subset(
    #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "SubsetOptions")] options: Option<JsValue>,
) -> JsResult<JsValue> {
    let input = conv::bytes_from_js(&data, "data")?;
    let o: subset::Options = opt(options)?;
    let mut s = subset::subset(&input, &o).js()?;
    let bytes = std::mem::take(&mut s.bytes);
    with_bytes(&s, &[("bytes", &bytes)])
}

/// `subset::Options::text`: options selecting every scalar of `text`.
#[wasm_bindgen(js_namespace = ["sfnt", "subset"], js_name = optionsText, unchecked_return_type = "SubsetOptions")]
pub fn options_text(text: &str) -> JsResult<JsValue> {
    conv::to_js(&subset::Options::text(text))
}
