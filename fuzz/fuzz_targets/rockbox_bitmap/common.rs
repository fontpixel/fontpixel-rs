use fontpixel::resources::{
    Input, OutputBundle, ReadContext, ResourceLimits, ResourcePath, ResourceResolver, Written,
};
use fontpixel::{FontSet, Report, Result, WriteOptions};
pub fn limits() -> ResourceLimits {
    ResourceLimits {
        max_input_bytes: 256 * 1024,
        max_resource_bytes: 256 * 1024,
        max_total_bytes: 16 * 1024 * 1024,
        max_decompressed_bytes: 1024 * 1024,
        max_glyphs: 1024,
        max_pixels_per_glyph: 16384,
        max_table_bytes: 256 * 1024,
        max_resources: 4,
    }
}
struct Bundle<'a>(&'a OutputBundle);
impl ResourceResolver for Bundle<'_> {
    fn resolve(&self, p: &ResourcePath, max: usize) -> Result<Vec<u8>> {
        let b = &self
            .0
            .assets()
            .iter()
            .find(|a| a.path == *p)
            .ok_or_else(|| fontpixel::Error::MissingResource(p.to_string()))?
            .bytes;
        if b.len() > max {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "fuzz sidecar",
                limit: max,
            });
        }
        Ok(b.clone())
    }
}
pub fn exercise(
    data: &[u8],
    read: impl Fn(&mut ReadContext<'_>) -> Result<(FontSet, Report)>,
    write: impl Fn(&FontSet, &WriteOptions) -> Result<Written>,
) {
    let limits = limits();
    let input = Input::new(data).with_name("fuzz.fnt");
    let Ok(mut context) = ReadContext::new(&input, &limits) else {
        return;
    };
    let Ok((font, _)) = read(&mut context) else {
        return;
    };
    let options = WriteOptions::new(ResourcePath::new("fuzz.fnt").unwrap());
    let Ok(out) = write(&font, &options) else {
        return;
    };
    let second = write(&font, &options).expect("deterministic write succeeds twice");
    assert_eq!(out, second);
    let resolver = Bundle(&out.output);
    let bytes = &out
        .output
        .assets()
        .iter()
        .find(|a| a.path == *out.output.primary())
        .unwrap()
        .bytes;
    let input = Input::new(bytes)
        .with_name("fuzz.fnt")
        .with_resources(&resolver);
    let mut context = ReadContext::new(&input, &limits).expect("writer respects primary input cap");
    read(&mut context).expect("writer output is readable under the same limits");
    let strict = WriteOptions {
        strict: true,
        ..options
    };
    let result = write(&font, &strict);
    if out.report.has_loss() {
        assert!(result.is_err());
    } else {
        assert!(result.is_ok());
    }
}
