use fontpixel::resources::{
    Input, ReadContext, ResourceLimits, ResourcePath, ResourceResolver, Written,
};
use fontpixel::{FontSet, Report, Result, WriteOptions};
use std::collections::BTreeMap;
pub fn limits() -> ResourceLimits {
    ResourceLimits {
        max_input_bytes: 256 * 1024,
        max_resource_bytes: 256 * 1024,
        max_total_bytes: 16 * 1024 * 1024,
        max_decompressed_bytes: 1024 * 1024,
        max_glyphs: 256,
        max_pixels_per_glyph: 16384,
        max_table_bytes: 256 * 1024,
        max_resources: 64,
    }
}
struct Assets<'a>(BTreeMap<ResourcePath, &'a [u8]>);
impl ResourceResolver for Assets<'_> {
    fn resolve(&self, p: &ResourcePath, max: usize) -> Result<Vec<u8>> {
        let bytes = self
            .0
            .get(p)
            .ok_or_else(|| fontpixel::Error::MissingResource(p.to_string()))?;
        if bytes.len() > max {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "fuzz sidecar",
                limit: max,
            });
        }
        Ok(bytes.to_vec())
    }
    fn list(
        &self,
        p: Option<&ResourcePath>,
        max: usize,
        bytes: usize,
    ) -> Result<Vec<ResourcePath>> {
        assert!(p.is_none());
        if self.0.len() > max
            || self
                .0
                .keys()
                .map(|p| p.as_str().len() * 2 + 64)
                .sum::<usize>()
                > bytes
        {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "fuzz listing",
                limit: max,
            });
        }
        Ok(self.0.keys().cloned().collect())
    }
}
fn packet(data: &[u8]) -> Option<Assets<'_>> {
    if !data.starts_with(b"FPSEED2\0") {
        return None;
    }
    let count = usize::from(*data.get(8)?);
    if count > 32 {
        return None;
    }
    let mut pos = 9;
    let mut assets = BTreeMap::new();
    for _ in 0..count {
        let len = usize::from(*data.get(pos)?);
        pos += 1;
        let name = std::str::from_utf8(data.get(pos..pos + len)?).ok()?;
        pos += len;
        let size = u32::from_le_bytes(data.get(pos..pos + 4)?.try_into().ok()?) as usize;
        pos += 4;
        let path = ResourcePath::new(name).ok()?;
        let bytes = data.get(pos..pos.checked_add(size)?)?;
        pos += size;
        if assets.insert(path, bytes).is_some() {
            return None;
        }
    }
    if pos != data.len() {
        return None;
    }
    Some(Assets(assets))
}
pub fn exercise(
    data: &[u8],
    name: &str,
    directory: bool,
    read: impl Fn(&mut ReadContext<'_>) -> Result<(FontSet, Report)>,
    write: impl Fn(&FontSet, &WriteOptions) -> Result<Written>,
) {
    let limits = limits();
    let resources = packet(data).unwrap_or_else(|| Assets(BTreeMap::new()));
    let primary = ResourcePath::new(name).unwrap();
    let raw = resources.0.get(&primary).copied().unwrap_or(data);
    let input = if directory {
        Input::directory(name, &resources)
    } else {
        Input::new(raw).with_name(name).with_resources(&resources)
    };
    let Ok(mut context) = ReadContext::new(&input, &limits) else {
        return;
    };
    let Ok((font, _)) = read(&mut context) else {
        return;
    };
    let options = WriteOptions::new(primary);
    let Ok(out) = write(&font, &options) else {
        return;
    };
    assert_eq!(
        out,
        write(&font, &options).expect("deterministic second write")
    );
    let output = &out.output;
    let assets = Assets(
        output
            .assets()
            .iter()
            .map(|a| {
                let name = if output.is_directory() {
                    a.path
                        .as_str()
                        .strip_prefix(output.primary().as_str())
                        .unwrap()
                        .strip_prefix('/')
                        .unwrap()
                } else {
                    a.path.as_str()
                };
                (ResourcePath::new(name).unwrap(), a.bytes.as_slice())
            })
            .collect(),
    );
    let primary = output.primary();
    let input = if output.is_directory() {
        Input::directory(primary.as_str(), &assets)
    } else {
        Input::new(assets.0[primary])
            .with_name(primary.as_str())
            .with_resources(&assets)
    };
    let mut context = ReadContext::new(&input, &limits).expect("writer primary cap");
    read(&mut context).expect("writer output reads under same limits");
    let strict = WriteOptions {
        strict: true,
        ..options
    };
    assert_eq!(write(&font, &strict).is_err(), out.report.has_loss());
}
