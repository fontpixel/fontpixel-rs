#![no_main]
use fontpixel::{Input, ResourceLimits, ResourcePath};
use fontpixel::formats::{FormatReader, FormatWriter, ReadOptions, WriteOptions};
use fontpixel::formats::image::{ImageCodec, ImageReadOptions, ImageWriteOptions, Mapping};
use fontpixel::resources::{OutputBundle, ReadContext, ResourceResolver};
use libfuzzer_sys::fuzz_target;
struct Resolver<'a>(&'a [u8]);
impl ResourceResolver for Resolver<'_> {
    fn resolve(&self, _: &ResourcePath, max: usize) -> fontpixel::Result<Vec<u8>> {
        if self.0.len() > max { return Err(fontpixel::Error::ResourceLimit { resource: "fuzz resource", limit: max }); }
        Ok(self.0.to_vec())
    }
}
struct Assets(OutputBundle);
impl ResourceResolver for Assets {
    fn resolve(&self, p: &ResourcePath, max: usize) -> fontpixel::Result<Vec<u8>> {
        let a = self.0.assets().iter().find(|a| &a.path == p).ok_or_else(|| fontpixel::Error::MissingResource(p.to_string()))?;
        Resolver(&a.bytes).resolve(p, max)
    }
}
fuzz_target!(|data: &[u8]| {
    if data.len() < 5 { return; }
    let l = ResourceLimits { max_input_bytes: 65536, max_resource_bytes: 65536, max_total_bytes: 2*1024*1024, max_decompressed_bytes: 262144, max_table_bytes: 65536, max_glyphs: 256, max_pixels_per_glyph: 65536, max_resources: 4 };
    let len = u32::from_le_bytes(data[1..5].try_into().unwrap()) as usize;
    let split = 5 + len.min(data.len()-5);
    let (bytes, sidecar) = (&data[5..split], &data[split..]);
    let mut codec = ImageCodec::default();
    let mode = data[0] % 4;
    if mode == 0 { codec.read_options = ImageReadOptions { mapping: Some(Mapping { pixel_size: Some(1), cell_size: Some([1,1]), characters: Some("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz".into()), ..Default::default() }), mapping_path: None }; }
    let name = match mode { 1 => "a.json", 2 => "a.toml", _ => "a.tga" };
    let resolver = Resolver(sidecar);
    let input = Input::new(bytes).with_name(name).with_resources(&resolver);
    let options = ReadOptions { limits: l.clone(), ..Default::default() };
    let Ok(mut context) = ReadContext::new(&input, &l) else { return; };
    if let Ok((font, _)) = codec.read(&mut context, &options) {
        let writer = ImageCodec { write_options: ImageWriteOptions { limits: l.clone(), ..Default::default() }, ..Default::default() };
        if let Ok(w) = writer.write(&font, &WriteOptions::new(ResourcePath::new("a.png").unwrap())) {
            let a = Assets(w.output);
            let image = a.0.assets().iter().find(|a| a.path.as_str() == "a.png").unwrap();
            let input = Input::new(&image.bytes).with_name("a.png").with_resources(&a);
            let mut context = ReadContext::new(&input, &l).unwrap();
            let (back, _) = ImageCodec::default().read(&mut context, &options).expect("image writer emits readable mapping and pixels");
            assert_eq!(font, back);
        }
    }
});
