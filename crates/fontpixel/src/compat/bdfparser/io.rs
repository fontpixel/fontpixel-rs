//! Bounded file/stream conveniences for the compatibility API.
use super::{Font, ttf::TtfOptions};
use crate::{Error, ResourceLimits, Result};
use std::{
    io::{Read, Write},
    path::Path,
};
/// Strongly typed writer selection for `Font.save`.
#[derive(Debug, Clone)]
pub enum SaveOptions {
    /// Canonical BDF text.
    Bdf,
    /// Native deterministic PCF options.
    Pcf(crate::formats::pcf::PcfWriteOptions),
    /// SPEC-v3 TrueType options.
    Ttf(TtfOptions),
    /// Default SVG font settings.
    Svg,
}
impl Font {
    /// Load from a file using the same bounded format detection as load_bytes.
    pub fn load_file_path(&mut self, path: impl AsRef<Path>) -> Result<&mut Self> {
        self.load_file_obj(std::fs::File::open(path)?)
    }
    /// Load a binary Read stream, refusing excessive input before decoding.
    pub fn load_file_obj(&mut self, reader: impl Read) -> Result<&mut Self> {
        let max = ResourceLimits::default().max_input_bytes;
        let mut data = Vec::new();
        reader.take(max as u64 + 1).read_to_end(&mut data)?;
        if data.len() > max {
            return Err(Error::ResourceLimit {
                resource: "compatibility input bytes",
                limit: max,
            });
        }
        self.load_bytes(&data)
    }
    /// Serialize to any Write stream; data is fully validated before the first write.
    pub fn save_to(&self, mut writer: impl Write, options: &SaveOptions) -> Result<()> {
        let bytes = match options {
            SaveOptions::Bdf => self.tobdf()?.into_bytes(),
            SaveOptions::Pcf(o) => self.topcf(o)?,
            SaveOptions::Ttf(o) => self.tottf(o)?,
            SaveOptions::Svg => self.tosvgfont(0., 0., false, "hexncr")?.into_bytes(),
        };
        writer.write_all(&bytes)?;
        Ok(())
    }
    /// Save to a file. Without options, infer bdf/pcf/ttf/svg from its extension.
    /// Serialization is validated before creating or truncating the destination.
    pub fn save(&self, path: impl AsRef<Path>, options: Option<&SaveOptions>) -> Result<()> {
        let path = path.as_ref();
        let selected = if let Some(o) = options {
            o.clone()
        } else {
            match path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str()
            {
                "bdf" => SaveOptions::Bdf,
                "pcf" => SaveOptions::Pcf(Default::default()),
                "ttf" => SaveOptions::Ttf(Default::default()),
                "svg" => SaveOptions::Svg,
                _ => {
                    return Err(Error::InvalidInput(
                        "Unknown output format (use bdf, pcf, ttf or svg)".into(),
                    ));
                }
            }
        };
        let mut bytes = Vec::new();
        self.save_to(&mut bytes, &selected)?;
        std::fs::write(path, bytes)?;
        Ok(())
    }
}
