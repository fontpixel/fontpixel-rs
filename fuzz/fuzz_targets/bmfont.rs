#![no_main]
use fontpixel::{
    Error, ResourceLimits, Result,
    formats::bmfont::{self, BmfontEncoding, BmfontReadOptions, BmfontWriteOptions},
    resources::{Input, OutputBundle, ReadContext, ResourcePath, ResourceResolver},
};
use libfuzzer_sys::fuzz_target;
struct Page<'a>(&'a [u8]);
impl ResourceResolver for Page<'_> {
    fn resolve(&self, _: &ResourcePath, max: usize) -> Result<Vec<u8>> {
        if self.0.len() > max {
            return Err(Error::ResourceLimit {
                resource: "fuzz page",
                limit: max,
            });
        }
        Ok(self.0.to_vec())
    }
}
struct Bundle<'a>(&'a OutputBundle);
impl ResourceResolver for Bundle<'_> {
    fn resolve(&self, path: &ResourcePath, max: usize) -> Result<Vec<u8>> {
        let bytes = &self
            .0
            .assets()
            .iter()
            .find(|a| &a.path == path)
            .ok_or_else(|| Error::MissingResource(path.to_string()))?
            .bytes;
        if bytes.len() > max {
            return Err(Error::ResourceLimit {
                resource: "fuzz output page",
                limit: max,
            });
        }
        Ok(bytes.clone())
    }
}
fuzz_target!(|data: &[u8]| {
    let Some(prefix) = data.get(..4) else {
        return;
    };
    let n = u32::from_le_bytes(prefix.try_into().unwrap()) as usize;
    let Some(end) = n.checked_add(4) else {
        return;
    };
    let Some(description) = data.get(4..end) else {
        return;
    };
    let _ = bmfont::detect(description);
    let page = Page(&data[end..]);
    let limits = ResourceLimits {
        max_input_bytes: 65536,
        max_resource_bytes: 65536,
        max_decompressed_bytes: 262144,
        max_total_bytes: 4 * 1024 * 1024,
        max_table_bytes: 32768,
        max_glyphs: 128,
        max_pixels_per_glyph: 16384,
        max_resources: 16,
    };
    let input = Input::new(description).with_resources(&page);
    let Ok(mut context) = ReadContext::new(&input, &limits) else {
        return;
    };
    if let Ok((font, _)) = bmfont::read(&mut context, &BmfontReadOptions::default()) {
        for encoding in [
            BmfontEncoding::Text,
            BmfontEncoding::Xml,
            BmfontEncoding::Binary,
            BmfontEncoding::Json,
        ] {
            let options = BmfontWriteOptions {
                encoding,
                page_width: 32,
                page_height: 32,
                padding: 1,
                limits: limits.clone(),
            };
            if let Ok(written) =
                bmfont::write(&font, &ResourcePath::new("fuzz.fnt").unwrap(), &options)
            {
                let bundle = Bundle(&written.output);
                let primary = written
                    .output
                    .assets()
                    .iter()
                    .find(|a| &a.path == written.output.primary())
                    .expect("primary asset");
                let input = Input::new(&primary.bytes).with_resources(&bundle);
                let mut context =
                    ReadContext::new(&input, &limits).expect("output input under identical limits");
                let (back, _) = bmfont::read(&mut context, &BmfontReadOptions::default())
                    .expect("BMFont output readable under identical limits");
                if !written.report.has_loss() {
                    assert_eq!(font, back);
                }
            }
        }
    }
});
