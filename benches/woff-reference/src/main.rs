// Isolates the exact WOFF2 call used by Tong export-rs; reference-only, not a
// production fontpixel dependency. Input read/output write are outside timing.
use std::time::Instant;
fn main() -> Result<(),Box<dyn std::error::Error>> {
    let a=std::env::args().collect::<Vec<_>>();
    if a.len()!=4 { return Err("usage: fontpixel-tong-woff-reference INPUT OUTPUT QUALITY".into()); }
    if a[1]=="decode" {let input=std::fs::read(&a[2])?;let output=woff::version2::decompress(&input).ok_or("WOFF2 decoding failed")?;std::fs::write(&a[3],output)?;return Ok(());}
    let input=std::fs::read(&a[1])?;let quality=a[3].parse()?;let mut times=Vec::new();let mut data=Vec::new();
    for _ in 0..5 {let start=Instant::now();data=woff::version2::compress(&input,"",quality,true).ok_or("WOFF2 compression failed")?;times.push(start.elapsed().as_secs_f64());}
    println!("{{\"seconds\":{:?},\"bytes\":{},\"quality\":{}}}",times,data.len(),quality);
    std::fs::write(&a[2],data)?;Ok(())
}
