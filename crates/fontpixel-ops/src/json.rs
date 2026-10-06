//! JSON documents exactly as the command-line tool prints them.
use fontpixel::{Error, Result};
use std::io::Write;

/// Serialize `value` followed by a newline. `pretty` selects the indented
/// standalone layout; batch task results use the compact layout.
pub fn write<T: serde::Serialize + ?Sized>(
    out: &mut dyn Write,
    value: &T,
    pretty: bool,
) -> Result<()> {
    let result = if pretty {
        serde_json::to_writer_pretty(&mut *out, value)
    } else {
        serde_json::to_writer(&mut *out, value)
    };
    result.map_err(|error| Error::InvalidInput(format!("cannot serialize JSON: {error}")))?;
    writeln!(out)?;
    Ok(())
}
/// Serialize `value` into a string with [`write()`]'s layout.
pub fn to_string<T: serde::Serialize + ?Sized>(value: &T, pretty: bool) -> Result<String> {
    let mut bytes = Vec::new();
    write(&mut bytes, value, pretty)?;
    String::from_utf8(bytes).map_err(|_| Error::InvalidInput("JSON is not UTF-8".into()))
}
