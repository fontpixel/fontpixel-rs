//! Shared regional variants fixture for ledger and allocation tests.
use fontpixel::{
    Bitmap, Font, Glyph, Variants,
    bitmap::Gray8,
    model::{Codepoint, Vector},
    variants::ScriptVariant,
};

pub fn face(glyphs: &[(u32, &str, i64)]) -> Font {
    let mut f = Font::default();
    f.metrics.pixel_size = Some(14);
    f.metrics.ascent = Some(12);
    f.metrics.descent = Some(2);
    f.names.family = Some("Ledger".into());
    for &(cp, pattern, advance) in glyphs {
        let mut g = Glyph::new(pattern.parse::<Bitmap>().unwrap());
        g.codepoints = vec![Codepoint::Unicode(cp)];
        g.metrics.advance = Some(Vector::new(advance, 0));
        f.add_glyph(g).unwrap();
    }
    f
}

/// Base, two equal and one distinct language face, an alternative-only
/// character, a supplementary-plane character and a script face.
pub fn variants() -> Variants {
    let mut v = Variants::new(face(&[
        (0x41, "#.\n##", 3),
        (0x42, "###\n#.#\n###", 4),
        (0x4e00, "#####\n.....\n#####", 6),
        (0x20000, "#.#\n.#.\n#.#", 4),
    ]));
    let tc = face(&[
        (0x42, "###\n#..\n###", 4),
        (0x4e00, "#####\n.....\n#####", 6),
        (0x4e01, "####\n.#..\n.#..", 5),
    ]);
    v.add_language("zh-Hant", tc.clone()).unwrap();
    v.add_language("zh-HK", tc).unwrap();
    v.add_language(
        "ja",
        face(&[(0x42, "###\n#..\n###", 4), (0x4e00, "####\n....\n####", 6)]),
    )
    .unwrap();
    v.scripts.push(ScriptVariant {
        scripts: vec![*b"latn", *b"grek", *b"cyrl"],
        font: face(&[(0x41, "##\n##", 3)]),
    });
    v
}

/// A single face with aliases, an unencoded glyph, an unrepresentable name
/// and one gray glyph.
pub fn single() -> Font {
    let mut f = face(&[(0x41, "#.\n##", 3), (0x42, "##\n.#", 3)]);
    let mut alias = Glyph::new("#\n#".parse::<Bitmap>().unwrap());
    alias.codepoints = vec![Codepoint::Unicode(0x43), Codepoint::Unicode(0x44)];
    alias.name = Some("bad name!".into());
    f.add_glyph(alias).unwrap();
    let mut unencoded = Glyph::new("###".parse::<Bitmap>().unwrap());
    unencoded.name = Some("extra".into());
    f.add_glyph(unencoded).unwrap();
    let mut gray = Bitmap::<Gray8>::new(2, 2).unwrap();
    gray.set_pixel(0, 0, 200).unwrap();
    gray.set_pixel(1, 1, 90).unwrap();
    let mut g = Glyph::new(gray);
    g.codepoints = vec![Codepoint::Unicode(0x45)];
    f.add_glyph(g).unwrap();
    f
}
