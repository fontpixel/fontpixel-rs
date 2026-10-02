//! Canonical JSONL differential runner for the bdfparser compatibility façade.
use base64::{Engine, engine::general_purpose::STANDARD};
use fontpixel::charset::{CharsetCodec, canonical_charset, charset_props, sanitize_charset};
use fontpixel::compat::bdfparser::{Bitmap, CreateOptions, Font, Glyph};
use fontpixel::formats::pcf::{GlyphPad, Order, PcfWriteOptions, ScanUnit};
use serde_json::{Map, Value as V, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, BufWriter, Write},
};
type R<T> = Result<T, Box<dyn std::error::Error>>;
fn fail(s: &str) -> Box<dyn std::error::Error> {
    std::io::Error::other(s).into()
}
fn num(v: &V) -> i64 {
    v.as_i64().unwrap_or(0)
}
fn strv(v: &V) -> &str {
    v.as_str().unwrap_or("")
}
fn int(o: &V, k: &str, d: i64) -> i64 {
    o.get(k).map(num).unwrap_or(d)
}
fn arr(v: &V) -> &[V] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn kw(o: &V) -> Map<String, V> {
    o.get("kw")
        .and_then(V::as_object)
        .cloned()
        .unwrap_or_default()
}
fn bmp(v: &V) -> R<Bitmap> {
    Ok(Bitmap::new(
        &arr(v)
            .iter()
            .map(|v| strv(v).to_owned())
            .collect::<Vec<_>>(),
    )?)
}
fn bounds(v: &V) -> Option<[i64; 4]> {
    let a = v.as_array()?;
    if a.len() != 4 {
        return None;
    }
    Some([num(&a[0]), num(&a[1]), num(&a[2]), num(&a[3])])
}
fn bmval(b: &Bitmap) -> V {
    json!({"$bitmap":b.bindata()})
}
fn hash(s: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(s.as_ref()))
}
fn canon(v: &V) -> String {
    if let Some(units) = v
        .as_object()
        .filter(|o| o.len() == 1)
        .and_then(|o| o.get("$wtf16"))
        .and_then(V::as_array)
    {
        return format!(
            "\"{}\"",
            units
                .iter()
                .map(|v| format!("\\u{:04x}", num(v)))
                .collect::<String>()
        );
    }

    if let Some(n) = v
        .as_object()
        .filter(|o| o.len() == 1)
        .and_then(|o| o.get("$surrogate"))
        .and_then(V::as_u64)
    {
        return format!("\"\\u{n:04x}\"");
    }
    match v {
        V::Null => "null".into(),
        V::Bool(b) => b.to_string(),
        V::Number(n) => {
            let s = n.to_string();
            if s.contains(['.', 'e', 'E']) {
                jsnum(n.as_f64().unwrap_or(0.0))
            } else {
                s
            }
        }
        V::String(s) => {
            let mut out = String::from("\"");
            for c in s.encode_utf16() {
                match c {
                    34 => out.push_str("\\\""),
                    92 => out.push_str("\\\\"),
                    32..=126 => out.push(char::from(c as u8)),
                    _ => out.push_str(&format!("\\u{c:04x}")),
                }
            }
            out.push('"');
            out
        }
        V::Array(a) => format!("[{}]", a.iter().map(canon).collect::<Vec<_>>().join(",")),
        V::Object(o) => format!(
            "{{{}}}",
            o.iter()
                .map(|(k, v)| format!("{}:{}", canon(&json!(k)), canon(v)))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}
fn jsnum(f: f64) -> String {
    if f == 0.0 {
        return "0".into();
    }
    let sign = if f < 0.0 { "-" } else { "" };
    let s = f.abs().to_string();
    let (base, ex) = s
        .split_once(['e', 'E'])
        .map(|(a, b)| (a, b.parse::<i32>().unwrap_or(0)))
        .unwrap_or((&s, 0));
    let (ip, fp) = base.split_once('.').unwrap_or((base, ""));
    let raw = format!("{ip}{fp}");
    let digits = raw.trim_start_matches('0');
    let n = ip.len() as i32 - (raw.len() - digits.len()) as i32 + ex;
    let digits = digits.trim_end_matches('0');
    let k = digits.len() as i32;
    let body = if k <= n && n <= 21 {
        format!("{}{}", digits, "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{}", "0".repeat((-n) as usize), digits)
    } else {
        let e = n - 1;
        format!(
            "{}{}e{}{eabs}",
            &digits[..1],
            if k > 1 {
                format!(".{}", &digits[1..])
            } else {
                String::new()
            },
            if e >= 0 { "+" } else { "-" },
            eabs = e.abs()
        )
    };
    format!("{sign}{body}")
}
fn shrink(s: String, full: bool) -> String {
    if full || s.len() <= 600 {
        s
    } else {
        canon(&json!({"$sha256":hash(&s),"n":s.len()}))
    }
}
fn digest(f: &Font, full: bool) -> V {
    if full {
        f.model()
    } else {
        let m = f.model();
        json!({"model":hash(canon(&m)),"n":f.length(),"u":arr(&m["unencoded"]).len()})
    }
}
fn unicode(n: Option<u32>) -> V {
    match n {
        Some(n) if (0xd800..=0xdfff).contains(&n) => json!({"$surrogate":n}),
        Some(n) => json!(char::from_u32(n)),
        None => V::Null,
    }
}
fn bytes(b: &[u8], full: bool) -> V {
    let mut o = Map::new();
    o.insert("$bytes".into(), json!(b.len()));
    o.insert("sha256".into(), json!(hash(b)));
    if full {
        o.insert("b64".into(), json!(STANDARD.encode(b)));
    }
    V::Object(o)
}
#[allow(clippy::field_reassign_with_default)]
fn pcfopts(o: &V) -> R<PcfWriteOptions> {
    let kw = kw(o);
    let v = V::Object(kw);
    let mut options = PcfWriteOptions::default();
    options.byte_order = if v
        .get("byteorder")
        .and_then(V::as_str)
        .unwrap_or("msb")
        .eq_ignore_ascii_case("msb")
    {
        Order::Msb
    } else {
        Order::Lsb
    };
    options.bit_order = if v
        .get("bitorder")
        .and_then(V::as_str)
        .unwrap_or("msb")
        .eq_ignore_ascii_case("msb")
    {
        Order::Msb
    } else {
        Order::Lsb
    };
    options.glyph_pad = match int(&v, "glyphpad", 4) {
        1 => GlyphPad::One,
        2 => GlyphPad::Two,
        4 => GlyphPad::Four,
        8 => GlyphPad::Eight,
        _ => return Err(fail("invalid glyphpad")),
    };
    options.scan_unit = match int(&v, "scanunit", 1) {
        1 => ScanUnit::One,
        2 => ScanUnit::Two,
        4 => ScanUnit::Four,
        _ => return Err(fail("invalid scanunit")),
    };
    Ok(options)
}
fn bop(b: &mut Bitmap, m: &str, a: &[V]) -> R<Option<V>> {
    let n = |i: usize, d: i64| a.get(i).map(num).unwrap_or(d);
    match m {
        "crop" => {
            b.crop(n(0, 0), n(1, 0), n(2, 0), n(3, 0))?;
        }
        "overlay" => {
            b.overlay(&bmp(&a[0])?);
        }
        "overlay_same" => {
            let mut x = n(0, 0).rem_euclid(65537);
            let rows = (0..b.height())
                .map(|_| {
                    (0..b.width())
                        .map(|_| {
                            x = (x * 75 + 74) % 65537;
                            match x % 7 {
                                0 | 1 => '1',
                                2 => '2',
                                _ => '0',
                            }
                        })
                        .collect()
                })
                .collect::<Vec<String>>();
            b.overlay(&Bitmap::new(&rows)?);
        }
        "concat" => {
            b.concat(&bmp(&a[0])?, n(1, 1), n(2, 1), n(3, 0))?;
        }
        "concatall" => {
            let bs = arr(&a[0]).iter().map(bmp).collect::<R<Vec<_>>>()?;
            let gaps = a
                .get(3)
                .map(arr)
                .unwrap_or(&[])
                .iter()
                .map(num)
                .collect::<Vec<_>>();
            *b = Bitmap::concatall(&bs, n(1, 1), n(2, 1), &gaps)?;
        }
        "enlarge" => {
            b.enlarge(n(0, 1), n(1, 1))?;
        }
        "replace" => {
            let text = |v: &V| v.as_str().map(str::to_owned).unwrap_or(v.to_string());
            b.replace(&text(&a[0]), &text(&a[1]))?;
        }
        "shadow" => {
            b.shadow(n(0, 1), n(1, -1))?;
        }
        "glow" => {
            b.glow(n(0, 0))?;
        }
        "bytepad" => {
            b.bytepad(n(0, 8))?;
        }
        "setpixel" => {
            b.setpixel(n(0, 0), n(1, 0), n(2, 1))?;
        }
        "rotate" => {
            b.rotate(n(0, 90))?;
        }
        "shift" => {
            b.shift(n(0, 0), n(1, 0))?;
        }
        "bold" => {
            b.bold(n(0, 1))?;
        }
        "italic" => {
            b.italic(n(0, 2), n(1, 0))?;
        }
        "fliph" => {
            b.fliph()?;
        }
        "flipv" => {
            b.flipv()?;
        }
        "invert" => {
            b.invert();
        }
        "trim" => {
            b.trim()?;
        }
        "paste" => {
            b.paste(&bmp(&a[0])?, n(1, 0), n(2, 0));
        }
        "clone" => {}
        "fromstring" => *b = Bitmap::fromstring(strv(&a[0]))?,
        "blank" => *b = Bitmap::blank(n(0, 0), n(1, 0))?,
        "todata" => return Ok(Some(b.todata(n(0, 1))?)),
        "getpixel" => return Ok(Some(json!(b.getpixel(n(0, 0), n(1, 0))?))),
        "bbox" => {
            let r = b.bbox();
            return Ok(Some(json!([r.width, r.height, r.x, r.y])));
        }
        "width" => return Ok(Some(json!(b.width()))),
        "height" => return Ok(Some(json!(b.height()))),
        "str" => return Ok(Some(json!(b.to_string()))),
        "equals" => return Ok(Some(json!(b.equals(&bmp(&a[0])?)))),
        "tosvg" => {
            return Ok(Some(json!(b.tosvg(
                a.first().and_then(V::as_f64).unwrap_or(1.0),
                a.get(1).map(strv).unwrap_or("path"),
                a.get(2).and_then(V::as_bool).unwrap_or(false),
                a.get(3).and_then(V::as_f64).unwrap_or(0.0),
                a.get(4).and_then(V::as_f64).unwrap_or(0.0),
                a.get(5).map(strv).unwrap_or("square")
            )?)));
        }
        "contours" | "raster" => {
            let cs = fontpixel::compat::bdfparser::outline::bitmap_contours(
                &b.bindata(),
                a.first().map(strv).unwrap_or("square"),
                a.get(1).and_then(V::as_f64).unwrap_or(1.0),
                n(2, 1),
                n(3, 0),
                n(4, 0),
            )?;
            return Ok(Some(if m == "contours" {
                json!(cs)
            } else {
                json!(
                    fontpixel::compat::bdfparser::outline::rasterize(
                        &cs,
                        n(2, 1) as f64,
                        0,
                        0,
                        b.width() as i64,
                        b.height() as i64
                    )?
                    .bindata()
                )
            }));
        }
        _ => return Err(fail("unknown bitmap method")),
    }
    Ok(None)
}
fn apply_fn(b: &mut Bitmap, steps: &V) -> fontpixel::Result<()> {
    for step in arr(steps) {
        let a = arr(step);
        if let Some(m) = a.first() {
            bop(b, strv(m), &a[1..]).map_err(|e| fontpixel::Error::InvalidInput(e.to_string()))?;
        }
    }
    Ok(())
}
#[derive(Default)]
struct Ctx {
    fonts: BTreeMap<String, Font>,
    handles: Vec<Option<Glyph>>,
    bitmap: Bitmap,
}
fn font<'a>(ctx: &'a mut Ctx, op: &V) -> R<&'a mut Font> {
    ctx.fonts
        .get_mut(op.get("f").and_then(V::as_str).unwrap_or("F"))
        .ok_or_else(|| fail("font is not loaded"))
}
fn run_op(ctx: &mut Ctx, case: &V, o: &V, full: bool) -> R<V> {
    let name = strv(&o["op"]);
    let to = |d: &str| o.get("to").and_then(V::as_str).unwrap_or(d).to_owned();
    match name {
        "load" => {
            let mut f = Font::default();
            let src = o.get("src").and_then(V::as_str).unwrap_or("bdf");
            let how = o.get("how").and_then(V::as_str).unwrap_or("string");
            if how == "string" || how == "lines" {
                f.load_string(strv(&case[src]))?;
            } else {
                let b = if src == "file" {
                    fs::read(strv(&case["file"]))?
                } else if let Some(s) = case.get(format!("{src}_b64")).and_then(V::as_str) {
                    STANDARD.decode(s)?
                } else {
                    strv(&case[src]).as_bytes().to_vec()
                };
                if b.starts_with(b"\0\x01\0\0") || b.starts_with(b"true") {
                    f.load_ttf(&b, o.get("pixelsize").and_then(V::as_i64))?;
                } else {
                    f.load_bytes(&b)?;
                }
            }
            let v = json!([digest(&f, full), f.charset(), f.length()]);
            ctx.fonts.insert(to("F"), f);
            Ok(v)
        }
        "create" => {
            let k = V::Object(kw(o));
            let opts = CreateOptions {
                pixelsize: int(&k, "pixelsize", 16),
                ascent: k.get("ascent").and_then(V::as_i64),
                descent: k.get("descent").and_then(V::as_i64),
                fontname: k.get("fontname").and_then(V::as_str).map(str::to_owned),
                familyname: k
                    .get("familyname")
                    .and_then(V::as_str)
                    .unwrap_or("Untitled")
                    .into(),
                xres: int(&k, "xres", 75),
                yres: int(&k, "yres", 75),
            };
            let f = Font::create(&opts)?;
            let v = digest(&f, full);
            ctx.fonts.insert(to("F"), f);
            Ok(v)
        }
        "model" => Ok(font(ctx, o)?.model()),
        "digest" => Ok(digest(font(ctx, o)?, full)),
        "length" => {
            let f = font(ctx, o)?;
            Ok(json!([f.length(), f.charset()]))
        }
        "tobdf" => Ok(json!(font(ctx, o)?.tobdf()?)),
        "pcf" => Ok(bytes(&font(ctx, o)?.topcf(&pcfopts(o)?)?, full)),
        "pcfrt" => {
            let b = font(ctx, o)?.topcf(&pcfopts(o)?)?;
            let mut f = Font::default();
            f.load_bytes(&b)?;
            let v = json!([digest(&f, full), f.tobdf()?]);
            ctx.fonts.insert(to("P"), f);
            Ok(v)
        }
        "svgfont" => {
            let k = V::Object(kw(o));
            Ok(json!(font(ctx, o)?.tosvgfont(
                k.get("xoff").and_then(V::as_f64).unwrap_or(0.0),
                k.get("yoff").and_then(V::as_f64).unwrap_or(0.0),
                k.get("ccns").and_then(V::as_bool).unwrap_or(false),
                k.get("uf").and_then(V::as_str).unwrap_or("hexncr")
            )?))
        }
        "ttf" => {
            let k = V::Object(kw(o));
            let options = fontpixel::compat::bdfparser::ttf::TtfOptions {
                shape: k
                    .get("shape")
                    .and_then(V::as_str)
                    .unwrap_or("square")
                    .into(),
                dotsize: k.get("dotsize").and_then(V::as_f64).unwrap_or(1.0),
                ppu: int(&k, "ppu", 64),
                metadata: k.get("metadata").and_then(V::as_bool).unwrap_or(true),
                timestamp: int(&k, "timestamp", 0),
            };
            Ok(bytes(&font(ctx, o)?.tottf(&options)?, full))
        }
        "ttfrt" => {
            let k = V::Object(kw(o));
            let options = fontpixel::compat::bdfparser::ttf::TtfOptions {
                shape: k
                    .get("shape")
                    .and_then(V::as_str)
                    .unwrap_or("square")
                    .into(),
                dotsize: k.get("dotsize").and_then(V::as_f64).unwrap_or(1.0),
                ppu: int(&k, "ppu", 64),
                metadata: k.get("metadata").and_then(V::as_bool).unwrap_or(true),
                timestamp: int(&k, "timestamp", 0),
            };
            let b = font(ctx, o)?.tottf(&options)?;
            let f = fontpixel::compat::bdfparser::ttf::read_ttf(
                &b,
                o.get("pixelsize").and_then(V::as_i64),
            )?;
            let v = json!([digest(&f, full), f.tobdf()?]);
            ctx.fonts.insert(to("T"), f);
            Ok(v)
        }
        "itercps" => Ok(json!(font(ctx, o)?.itercps(int(o, "order", 1), &o["r"])?)),
        "iterglyphs" => Ok(json!(
            font(ctx, o)?
                .iterglyphs(int(o, "order", 1), &o["r"])?
                .iter()
                .map(|g| g.meta().clone())
                .collect::<Vec<_>>()
        )),
        "draw" => {
            let f = font(ctx, o)?;
            let cps = codepoints(&o["text"])
                .iter()
                .map(|n| f.unicode2cp(*n).map(|v| v.map(i64::from).unwrap_or(-1)))
                .collect::<fontpixel::Result<Vec<_>>>()?;
            Ok(bmval(&f.drawcps(&cps, &kw(o))?))
        }
        "drawall" => Ok(bmval(&font(ctx, o)?.drawall(&kw(o))?)),
        "drawcps" => Ok(bmval(
            &font(ctx, o)?.drawcps(
                &arr(&o["cps"])
                    .iter()
                    .map(|v| v.as_i64().unwrap_or(-1))
                    .collect::<Vec<_>>(),
                &kw(o),
            )?,
        )),
        "gdraw" => Ok(match font(ctx, o)?.glyphbycp(num(&o["cp"])) {
            Some(g) => bmval(&g.draw(int(o, "mode", 0), bounds(&o["bb"]))?),
            None => V::Null,
        }),
        "gstr" => Ok(match font(ctx, o)?.glyphbycp(num(&o["cp"])) {
            Some(g) => json!(g.draw(0, None)?.to_string()),
            None => V::Null,
        }),
        "gorigin" => {
            let g = font(ctx, o)?.glyphbycp(num(&o["cp"]));
            if let Some(g) = g {
                let a = arr(&o["a"]);
                Ok(json!(g.origin(
                    a.first().map(num).unwrap_or(0),
                    a.get(1).and_then(V::as_bool).unwrap_or(false),
                    a.get(2).and_then(V::as_i64),
                    a.get(3).and_then(V::as_i64)
                )?))
            } else {
                Ok(V::Null)
            }
        }
        "gchr" => {
            let f = font(ctx, o)?;
            let v = arr(&o["cps"])
                .iter()
                .map(|c| f.glyphbycp(num(c)).and_then(|g| g.chr().ok().flatten()))
                .collect::<Vec<_>>();
            Ok(json!(v))
        }
        "gbychar" => {
            let f = font(ctx, o)?;
            let v = arr(&o["chars"])
                .iter()
                .map(|c| {
                    c2p(f, c).map(|cp| {
                        cp.and_then(|cp| f.glyphbycp(cp as i64))
                            .map(|g| g.meta().clone())
                    })
                })
                .collect::<fontpixel::Result<Vec<_>>>()?;
            Ok(json!(v))
        }
        "lacks" => {
            let f = font(ctx, o)?;
            let mut missing = Vec::new();
            for n in codepoints(&o["text"]) {
                if f.unicode2cp(n)?
                    .is_none_or(|cp| f.glyphbycp(cp as i64).is_none())
                {
                    missing.push(unicode(Some(n)));
                }
            }
            Ok(if missing.is_empty() {
                V::Null
            } else {
                json!(missing)
            })
        }
        "c2p" => {
            let f = font(ctx, o)?;
            Ok(json!(
                arr(&o["chars"])
                    .iter()
                    .map(|c| c2p(f, c))
                    .collect::<fontpixel::Result<Vec<_>>>()?
            ))
        }
        "p2c" => {
            let f = font(ctx, o)?;
            Ok(json!(
                arr(&o["cps"])
                    .iter()
                    .map(|c| f.cp2unicode(num(c)).map(unicode))
                    .collect::<fontpixel::Result<Vec<_>>>()?
            ))
        }
        "setprop" => {
            let f = font(ctx, o)?;
            f.setprop(strv(&o["key"]), o["value"].clone());
            Ok(digest(f, full))
        }
        "delprop" => {
            let f = font(ctx, o)?;
            f.delprop(strv(&o["key"]));
            Ok(digest(f, full))
        }
        "getprop" => {
            let f = font(ctx, o)?;
            Ok(json!(
                arr(&o["keys"])
                    .iter()
                    .map(|k| f.getprop(strv(k)))
                    .collect::<Vec<_>>()
            ))
        }
        "addglyph" => {
            let f = font(ctx, o)?;
            f.addglyph(
                o["meta"]
                    .as_object()
                    .ok_or_else(|| fail("meta is not an object"))?,
                o.get("replace").and_then(V::as_bool).unwrap_or(true),
            )?;
            Ok(digest(f, full))
        }
        "removeglyph" => {
            let f = font(ctx, o)?;
            f.removeglyph(num(&o["cp"]));
            Ok(digest(f, full))
        }
        "newglyph" => {
            let b = o
                .get("rows")
                .filter(|v| !v.is_null())
                .map(bmp)
                .transpose()?;
            let f = font(ctx, o)?;
            let g = f.newglyph(
                num(&o["cp"]),
                b.as_ref(),
                int(o, "mode", 0),
                o.get("name").and_then(V::as_str),
                o.get("dwidth").and_then(V::as_i64),
            )?;
            let v = json!([ctx.handles.len(), g.meta(), digest(&g.font(), full)]);
            ctx.handles.push(Some(g));
            Ok(v)
        }
        "gget" => {
            let g = font(ctx, o)?.glyphbycp(num(&o["cp"]));
            let v = json!([ctx.handles.len(), g.as_ref().map(Glyph::meta)]);
            ctx.handles.push(g);
            Ok(v)
        }
        "updatebbox" => {
            let f = font(ctx, o)?;
            f.updatebbox()?;
            let h = f.headers();
            Ok(json!([
                h.get("fbbx"),
                h.get("fbby"),
                h.get("fbbxoff"),
                h.get("fbbyoff")
            ]))
        }
        "subset" => {
            let f = font(ctx, o)?.subset(&o["r"])?;
            let v = digest(&f, full);
            ctx.fonts.insert(to("S"), f);
            Ok(v)
        }
        "clone" => {
            let f = font(ctx, o)?.clone();
            let v = digest(&f, full);
            ctx.fonts.insert(to("C"), f);
            Ok(v)
        }
        "merge" => {
            let other = ctx
                .fonts
                .get(strv(&o["g"]))
                .ok_or_else(|| fail("missing merge source"))?
                .clone();
            let f = font(ctx, o)?;
            f.merge(
                &other,
                o.get("replace").and_then(V::as_bool).unwrap_or(false),
            )?;
            Ok(digest(f, full))
        }
        "transform" => {
            let f = font(ctx, o)?;
            f.transform(|b| apply_fn(b, &o["fn"]), &o["r"], int(o, "mode", 0))?;
            Ok(digest(f, full))
        }
        "charsetconvert" => {
            let f = font(ctx, o)?;
            f.charsetconvert(strv(&o["charset"]))?;
            Ok(json!([digest(f, full), f.charset()]))
        }
        "encoding" => {
            let f = font(ctx, o)?;
            f.encoding(strv(&o["charset"]));
            Ok(json!(f.charset()))
        }
        "swidth" => {
            let f = font(ctx, o)?;
            Ok(json!(
                arr(&o["dwxs"])
                    .iter()
                    .map(|v| f.swidth(num(v)))
                    .collect::<Vec<_>>()
            ))
        }
        "bset" => {
            ctx.bitmap = bmp(&o["rows"])?;
            Ok(V::Null)
        }
        "bglyph" => {
            ctx.bitmap = font(ctx, o)?
                .glyphbycp(num(&o["cp"]))
                .map(|g| g.draw(int(o, "mode", 0), None))
                .transpose()?
                .unwrap_or_default();
            Ok(bmval(&ctx.bitmap))
        }
        "bdrawall" => {
            ctx.bitmap = font(ctx, o)?.drawall(&kw(o))?;
            Ok(bmval(&ctx.bitmap))
        }
        "bop" => Ok(bop(&mut ctx.bitmap, strv(&o["m"]), arr(&o["a"]))?
            .unwrap_or_else(|| bmval(&ctx.bitmap))),
        "cs" => {
            let key = canonical_charset(strv(&o["charset"]));
            let c = CharsetCodec::from_key(&key, o.get("gl").and_then(V::as_bool))?;
            Ok(json!([
                key,
                arr(&o["cps"])
                    .iter()
                    .map(|n| u32::try_from(num(n))
                        .ok()
                        .and_then(|n| c.cp2unicode(n))
                        .map(|n| unicode(Some(n))))
                    .collect::<Vec<_>>(),
                arr(&o["chars"])
                    .iter()
                    .map(|v| {
                        let cs = codepoints(v);
                        if cs.len() == 1 {
                            c.unicode2cp(cs[0])
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            ]))
        }
        "csname" => Ok(json!([
            arr(&o["pairs"])
                .iter()
                .map(|p| sanitize_charset(p[0].as_str(), p[1].as_str()))
                .collect::<Vec<_>>(),
            arr(&o["names"])
                .iter()
                .map(|n| canonical_charset(strv(n)))
                .collect::<Vec<_>>(),
            arr(&o["names"])
                .iter()
                .map(|n| charset_props(&canonical_charset(strv(n))))
                .collect::<Vec<_>>()
        ])),
        name if name.starts_with('h') => {
            let i = usize::try_from(int(o, "h", -1)).unwrap_or(usize::MAX);
            let Some(g) = ctx.handles.get_mut(i).and_then(Option::take) else {
                return Ok(json!("nohandle"));
            };
            let mut g = g;
            let result = (|| -> R<V> {
                match name {
                    "hmeta" => return Ok(json!(g.meta())),
                    "hdraw" => return Ok(bmval(&g.draw(int(o, "mode", 0), None)?)),
                    "hclone" => {
                        let v = json!([ctx.handles.len(), g.meta()]);
                        ctx.handles.push(Some(g.clone()));
                        return Ok(v);
                    }
                    "hsetbitmap" => {
                        g.setbitmap(
                            &bmp(&o["rows"])?,
                            int(o, "mode", 0),
                            bounds(&o["bb"]),
                            o.get("trim").and_then(V::as_bool).unwrap_or(true),
                        )?;
                    }
                    "hsetmeta" => {
                        g.setmeta(o["meta"].as_object().ok_or_else(|| fail("invalid meta"))?)?;
                    }
                    "hshift" => {
                        g.shift(int(o, "dx", 0), int(o, "dy", 0))?;
                    }
                    "htransform" => {
                        g.transform(|b| apply_fn(b, &o["fn"]), int(o, "mode", 0))?;
                    }
                    "haddglyph" => {
                        font(ctx, o)?.add_handle(
                            &mut g,
                            o.get("replace").and_then(V::as_bool).unwrap_or(true),
                        )?;
                    }
                    "hremove" => {
                        font(ctx, o)?.remove_handle(&mut g);
                    }
                    _ => return Err(fail("unknown handle method")),
                }
                Ok(json!([g.meta(), digest(&g.font(), full)]))
            })();
            ctx.handles[i] = Some(g);
            result
        }
        _ => Err(fail("unknown operation")),
    }
}
fn run_case(c: &V, full: bool) -> V {
    let mut ctx = Ctx::default();
    let mut res = Vec::new();
    for (i, o) in arr(&c["ops"]).iter().enumerate() {
        let name = strv(&o["op"]);
        match run_op(&mut ctx, c, o, full) {
            Ok(v) => res.push(json!([i, name, shrink(canon(&v), full), V::Null])),
            Err(e) => {
                let mut entry = vec![
                    json!(i),
                    json!(name),
                    V::Null,
                    json!(format!("Error: {e}").chars().take(300).collect::<String>()),
                ];
                if [
                    "setprop",
                    "delprop",
                    "addglyph",
                    "removeglyph",
                    "newglyph",
                    "updatebbox",
                    "merge",
                    "transform",
                    "charsetconvert",
                    "encoding",
                    "hsetbitmap",
                    "hsetmeta",
                    "hshift",
                    "htransform",
                    "haddglyph",
                    "hremove",
                ]
                .contains(&name)
                    && let Ok(f) = font(&mut ctx, o)
                {
                    entry.push(json!(shrink(canon(&digest(f, full)), full)));
                }
                res.push(V::Array(entry));
                if name == "load" {
                    break;
                }
            }
        }
    }
    json!({"id":c["id"],"res":res})
}
fn tables(inp: &str, out: &str) -> R<()> {
    let spec: V = serde_json::from_slice(&fs::read(inp)?)?;
    let mut all = Map::new();
    for ent in arr(&spec["charsets"]) {
        let key = canonical_charset(strv(&ent["name"]));
        let codec = CharsetCodec::from_key(&key, ent.get("gl").and_then(V::as_bool))?;
        let mut p2c = Vec::new();
        for cp in arr(&spec["cps"]).iter().chain(arr(&ent["extra_cps"])) {
            if let Some(ch) = u32::try_from(num(cp))
                .ok()
                .and_then(|n| codec.cp2unicode(n))
            {
                p2c.push(json!([cp, unicode(Some(ch))]));
            }
        }
        let mut c2p = Vec::new();
        for ch in arr(&spec["chars"]) {
            if let Some(cp) = u32::try_from(num(ch))
                .ok()
                .and_then(char::from_u32)
                .and_then(|c| codec.char2cp(c))
            {
                c2p.push(json!([ch, cp]));
            }
        }
        all.insert(
            strv(&ent["id"]).into(),
            json!({"key":key,"known":codec.known(),"p2c":p2c,"c2p":c2p}),
        );
    }
    fs::write(out, canon(&V::Object(all)))?;
    Ok(())
}
fn main() -> R<()> {
    let a = std::env::args().skip(1).collect::<Vec<_>>();
    if a.first().map(String::as_str) == Some("--tables") {
        return tables(
            a.get(1).ok_or_else(|| fail("missing table spec"))?,
            a.get(2).ok_or_else(|| fail("missing output"))?,
        );
    }
    if a.len() < 2 {
        return Err(fail(
            "usage: fontpixel-parity CASES.jsonl OUT.jsonl [--full] [--only IDs]",
        ));
    }
    let full = a.iter().any(|s| s == "--full");
    let only = a
        .iter()
        .position(|s| s == "--only")
        .and_then(|i| a.get(i + 1))
        .map(|s| s.split(',').collect::<Vec<_>>());
    let mut out = BufWriter::new(fs::File::create(&a[1])?);
    for line in BufReader::new(fs::File::open(&a[0])?).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if line.len() > 64 * 1024 * 1024 {
            return Err(fail("case exceeds 64 MiB"));
        }
        let c: V = parse_case(&line)?;
        if only
            .as_ref()
            .is_some_and(|ids| !ids.contains(&strv(&c["id"])))
        {
            continue;
        }
        writeln!(out, "{}", run_case(&c, full))?;
    }
    out.flush()?;
    Ok(())
}
// JSON permits lone UTF-16 surrogates while Rust strings do not. Preserve such
// values as tagged code-unit sequences inside the runner, never replace them.
fn parse_case(text: &str) -> R<V> {
    let b = text.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'"' {
            let start = i;
            while i < b.len() && b[i] != b'"' {
                i += 1;
            }
            out.push_str(&text[start..i]);
            continue;
        }
        let start = i;
        i += 1;
        while i < b.len() {
            if b[i] == b'\\' {
                i += 2;
            } else if b[i] == b'"' {
                i += 1;
                break;
            } else {
                i += 1;
            }
        }
        let token = text
            .get(start..i)
            .ok_or_else(|| fail("truncated JSON string"))?;
        if serde_json::from_str::<String>(token).is_ok() {
            out.push_str(token);
            continue;
        }
        let raw = token
            .get(1..token.len().saturating_sub(1))
            .ok_or_else(|| fail("invalid JSON string"))?;
        let mut units = Vec::<u16>::new();
        let mut cs = raw.chars();
        while let Some(c) = cs.next() {
            if c != '\\' {
                units.extend(c.encode_utf16(&mut [0; 2]).iter().copied());
                continue;
            }
            let e = cs.next().ok_or_else(|| fail("truncated JSON escape"))?;
            if e == 'u' {
                let mut n = 0;
                for _ in 0..4 {
                    n = n * 16
                        + cs.next()
                            .and_then(|c| c.to_digit(16))
                            .ok_or_else(|| fail("invalid JSON unicode escape"))?;
                }
                units.push(n as u16);
            } else {
                units.push(match e {
                    '"' => 34,
                    '\\' => 92,
                    '/' => 47,
                    'b' => 8,
                    'f' => 12,
                    'n' => 10,
                    'r' => 13,
                    't' => 9,
                    _ => return Err(fail("invalid JSON escape")),
                });
            }
        }
        out.push_str(&json!({"$wtf16":units}).to_string());
    }
    Ok(serde_json::from_str(&out)?)
}
fn codepoints(v: &V) -> Vec<u32> {
    if let Some(s) = v.as_str() {
        return s.chars().map(|c| c as u32).collect();
    }
    let units = arr(&v["$wtf16"]);
    let mut out = Vec::new();
    let mut i = 0;
    while i < units.len() {
        let n = num(&units[i]) as u32;
        i += 1;
        if (0xd800..=0xdbff).contains(&n) && i < units.len() {
            let low = num(&units[i]) as u32;
            if (0xdc00..=0xdfff).contains(&low) {
                out.push(0x10000 + ((n - 0xd800) << 10) + (low - 0xdc00));
                i += 1;
                continue;
            }
        }
        out.push(n);
    }
    out
}
fn c2p(f: &Font, v: &V) -> fontpixel::Result<Option<u32>> {
    let cps = codepoints(v);
    if cps.len() != 1 {
        return Ok(None);
    }
    f.unicode2cp(cps[0])
}
