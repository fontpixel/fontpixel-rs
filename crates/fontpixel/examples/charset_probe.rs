//! Binary, ordered (input u32 LE, output u32 LE) pairs for independent parity.
//! Used by tools/check_charsets.py; no font codec is exercised by this program.
use fontpixel::charset::CharsetCodec;
use std::io::{self, BufWriter, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("usage: charset_probe KEY GL(true|false) decode|encode|gb4".into());
    }
    let codec = CharsetCodec::from_key(&args[1], Some(args[2].parse()?))?;
    let mut output = BufWriter::new(io::stdout().lock());
    let mut emit = |key: u32, value: Option<u32>| -> io::Result<()> {
        if let Some(value) = value {
            output.write_all(&key.to_le_bytes())?;
            output.write_all(&value.to_le_bytes())?;
        }
        Ok(())
    };
    match args[3].as_str() {
        "decode" => {
            for code in 0..=0xffff {
                emit(code, codec.cp2unicode(code))?;
            }
        }
        "encode" => {
            for value in 0..=0x10ffff {
                emit(value, codec.unicode2cp(value))?;
            }
        }
        "gb4" => {
            for index in 0u32..126 * 10 * 126 * 10 {
                let d = index % 10 + 0x30;
                let c = (index / 10) % 126 + 0x81;
                let b = (index / (10 * 126)) % 10 + 0x30;
                let a = index / (10 * 126 * 10) + 0x81;
                let code = (a << 24) | (b << 16) | (c << 8) | d;
                emit(code, codec.cp2unicode(code))?;
            }
        }
        _ => return Err("unknown probe operation".into()),
    }
    output.flush()?;
    Ok(())
}
