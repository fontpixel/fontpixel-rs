//! Native editor codec timing helper; formal repetitions are coordinated externally.
use fontpixel::formats::{bdf, bmfont, kbits, sfd};
use fontpixel::resources::{Input, OutputBundle, ReadContext, ResourcePath, ResourceResolver};
use fontpixel::{FontSet, ResourceLimits, Result};
use std::{hint::black_box, time::Instant};
struct Bundle<'a>(&'a OutputBundle);
impl ResourceResolver for Bundle<'_> {
    fn resolve(&self, path: &ResourcePath, max: usize) -> Result<Vec<u8>> {
        let a = self
            .0
            .assets()
            .iter()
            .find(|a| &a.path == path)
            .ok_or_else(|| fontpixel::Error::MissingResource(path.to_string()))?;
        if a.bytes.len() > max {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "timing resource",
                limit: max,
            });
        }
        Ok(a.bytes.clone())
    }
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let path = args.get(1).ok_or_else(|| {
        fontpixel::Error::InvalidInput("usage: w44_timing INPUT.bdf [LOOPS]".into())
    })?;
    let loops = args
        .get(2)
        .map_or(Ok(1usize), |n| n.parse())
        .map_err(|e| fontpixel::Error::InvalidInput(format!("loops: {e}")))?;
    if loops == 0 {
        return Err(fontpixel::Error::InvalidInput(
            "loops must be positive".into(),
        ));
    }
    let bytes = std::fs::read(path)?;
    let f = bdf::read(&bytes, &Default::default())?.0;
    let set = FontSet::from(f.clone());
    let l = ResourceLimits::default();
    let mut output = Vec::new();
    for format in ["bmfont", "sfd", "kbits"] {
        let begin = Instant::now();
        let mut size = 0;
        for _ in 0..loops {
            size = match format {
                "bmfont" => black_box(bmfont::write(
                    &f,
                    &ResourcePath::new("timing.fnt")?,
                    &Default::default(),
                )?)
                .output
                .assets()
                .iter()
                .map(|a| a.bytes.len())
                .sum(),
                "sfd" => black_box(sfd::write(&set, &Default::default())?).0.len(),
                _ => black_box(kbits::write(&f, &Default::default())?).0.len(),
            };
        }
        let write_ns = begin.elapsed().as_nanos() / loops as u128;
        let begin;
        let read_ns = match format {
            "bmfont" => {
                let w = bmfont::write(&f, &ResourcePath::new("timing.fnt")?, &Default::default())?;
                let r = Bundle(&w.output);
                let primary = w
                    .output
                    .assets()
                    .iter()
                    .find(|a| &a.path == w.output.primary())
                    .unwrap();
                let input = Input::new(&primary.bytes).with_resources(&r);
                begin = Instant::now();
                for _ in 0..loops {
                    black_box(bmfont::read(
                        &mut ReadContext::new(&input, &l)?,
                        &Default::default(),
                    )?);
                }
                begin.elapsed().as_nanos() / loops as u128
            }
            "sfd" => {
                let (b, _) = sfd::write(&set, &Default::default())?;
                begin = Instant::now();
                for _ in 0..loops {
                    black_box(sfd::read(&b, &l)?);
                }
                begin.elapsed().as_nanos() / loops as u128
            }
            _ => {
                let (b, _) = kbits::write(&f, &Default::default())?;
                begin = Instant::now();
                for _ in 0..loops {
                    black_box(kbits::read(&b, &l)?);
                }
                begin.elapsed().as_nanos() / loops as u128
            }
        };
        output.push(serde_json::json!({"format":format,"glyphs":f.len(),"bytes":size,"loops":loops,"read_ns":read_ns,"write_ns":write_ns}));
    }
    println!("{}", serde_json::to_string(&output).unwrap());
    Ok(())
}
