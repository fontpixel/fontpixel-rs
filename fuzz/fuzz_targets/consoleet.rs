//! Consoleet directory parsing and successful writer same-limit reopen.
#![no_main]
use fontpixel::formats::{WriteOptions, consoleet::*};
use fontpixel::resources::{ReadContext, ResourceResolver};
use fontpixel::{Input, ResourceLimits, ResourcePath};
use libfuzzer_sys::fuzz_target;
struct Files(Vec<(String, Vec<u8>)>);
impl ResourceResolver for Files {
    fn resolve(&self, p: &ResourcePath, max: usize) -> fontpixel::Result<Vec<u8>> {
        let b = &self
            .0
            .iter()
            .find(|(n, _)| n == p.as_str())
            .ok_or_else(|| fontpixel::Error::MissingResource(p.to_string()))?
            .1;
        if b.len() > max {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "fuzz bytes",
                limit: max,
            });
        }
        Ok(b.clone())
    }
    fn list(
        &self,
        _: Option<&ResourcePath>,
        max: usize,
        bytes: usize,
    ) -> fontpixel::Result<Vec<ResourcePath>> {
        if self.0.len() > max
            || self
                .0
                .iter()
                .map(|(n, _)| 2 * (size_of::<ResourcePath>() + n.len()))
                .sum::<usize>()
                > bytes
        {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "fuzz listing",
                limit: bytes,
            });
        }
        self.0.iter().map(|(n, _)| ResourcePath::new(n)).collect()
    }
}
fuzz_target!(|data: &[u8]| {
    if data.is_empty() || data.len() > 65536 {
        return;
    }
    let limits = ResourceLimits {
        max_input_bytes: 65536,
        max_resource_bytes: 65536,
        max_total_bytes: 4 * 1024 * 1024,
        max_table_bytes: 65536,
        max_resources: 128,
        max_glyphs: 100,
        max_pixels_per_glyph: 65536,
        ..Default::default()
    };
    let files = Files(vec![(format!("{:04x}.txt", data[0]), data[1..].to_vec())]);
    let input = Input::directory("font", &files);
    let Ok(mut c) = ReadContext::new(&input, &limits) else {
        return;
    };
    let Ok((s, _)) = read(&mut c, &Default::default()) else {
        return;
    };
    if let Ok(w) = write(
        &s,
        &WriteOptions::new(ResourcePath::new("font").unwrap()),
        &ConsoleetWriteOptions {
            limits: limits.clone(),
            ..Default::default()
        },
    ) {
        let files = Files(
            w.output
                .into_assets()
                .into_iter()
                .map(|a| (a.path.as_str()[5..].into(), a.bytes))
                .collect(),
        );
        let input = Input::directory("font", &files);
        let mut c = ReadContext::new(&input, &limits).unwrap();
        assert!(read(&mut c, &Default::default()).is_ok());
    }
});
