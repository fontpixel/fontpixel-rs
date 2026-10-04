//! Native Consoleet directory, resource operations and filename validation.
#![cfg(feature = "consoleet")]
use fontpixel::formats::{WriteOptions, consoleet::*};
use fontpixel::resources::{ReadContext, ResourceResolver};
use fontpixel::{Input, ResourceLimits, ResourcePath, Result};
struct Files(Vec<(String, Vec<u8>)>);
impl ResourceResolver for Files {
    fn resolve(&self, p: &ResourcePath, max: usize) -> Result<Vec<u8>> {
        let bytes = &self
            .0
            .iter()
            .find(|(name, _)| name == p.as_str())
            .ok_or_else(|| fontpixel::Error::MissingResource(p.to_string()))?
            .1;
        if bytes.len() > max {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "test bytes",
                limit: max,
            });
        }
        Ok(bytes.clone())
    }
    fn list(
        &self,
        _: Option<&ResourcePath>,
        max: usize,
        bytes: usize,
    ) -> Result<Vec<ResourcePath>> {
        if self.0.len() > max
            || self
                .0
                .iter()
                .map(|(p, _)| (size_of::<ResourcePath>() + p.len()) * 2)
                .sum::<usize>()
                > bytes
        {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "test listing",
                limit: bytes,
            });
        }
        self.0.iter().map(|(p, _)| ResourcePath::new(p)).collect()
    }
}
fn fixture() -> Files {
    Files(vec![
        ("0041.txt".into(), b"PCLT\n3 2\n##..##\n..##..\n".to_vec()),
        ("0042.txt".into(), b"PCLT\n2 1\n####\n".to_vec()),
    ])
}
fn parse(f: &Files, l: &ResourceLimits) -> Result<(fontpixel::FontSet, fontpixel::Report)> {
    read(
        &mut ReadContext::new(&Input::directory("font", f), l)?,
        &Default::default(),
    )
}
#[test]
fn native_directory_roundtrip() {
    let (s, _) = parse(&fixture(), &Default::default()).unwrap();
    assert_eq!(s.fonts[0].len(), 2);
    let w = write(
        &s,
        &WriteOptions::new(ResourcePath::new("font").unwrap()),
        &Default::default(),
    )
    .unwrap();
    assert!(!w.report.has_loss(), "{}", w.report);
    assert!(w.output.is_directory());
    let f = Files(
        w.output
            .into_assets()
            .into_iter()
            .map(|a| {
                (
                    a.path.as_str().strip_prefix("font/").unwrap().into(),
                    a.bytes,
                )
            })
            .collect(),
    );
    assert_eq!(parse(&f, &Default::default()).unwrap().0, s);
}
#[test]
fn malformed_directory_and_resource_limits() {
    let mut f = fixture();
    f.0.push(("41.txt".into(), f.0[0].1.clone()));
    assert!(parse(&f, &Default::default()).is_err());
    f = fixture();
    f.0[0].0 = "../../0041.txt".into();
    assert!(parse(&f, &Default::default()).is_err());
    for max_resources in 0..3 {
        assert!(
            parse(
                &fixture(),
                &ResourceLimits {
                    max_resources,
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
    f = fixture();
    f.0[0].1 = b"PCLT\n4294967295 4294967295\n".to_vec();
    assert!(parse(&f, &Default::default()).is_err());
    f = fixture();
    f.0[0].1 = b"PCLT\n1 1\n#.\n".to_vec();
    assert!(parse(&f, &Default::default()).is_err());
}
#[test]
fn same_limit_reopen() {
    let (s, _) = parse(&fixture(), &Default::default()).unwrap();
    for cap in [1024, 16384, 1048576] {
        let limits = ResourceLimits {
            max_total_bytes: cap,
            ..Default::default()
        };
        if let Ok(w) = write(
            &s,
            &WriteOptions::new(ResourcePath::new("font").unwrap()),
            &ConsoleetWriteOptions {
                limits: limits.clone(),
                ..Default::default()
            },
        ) {
            let f = Files(
                w.output
                    .into_assets()
                    .into_iter()
                    .map(|a| (a.path.as_str()[5..].into(), a.bytes))
                    .collect(),
            );
            assert!(parse(&f, &limits).is_ok());
        }
    }
}
#[test]
fn high_caller_limits_still_validate_physical_text_before_allocation() {
    let files = Files(vec![("0041.txt".into(), b"PCLT\n100000 100000\n".to_vec())]);
    let limits = ResourceLimits {
        max_total_bytes: usize::MAX,
        max_table_bytes: usize::MAX,
        max_pixels_per_glyph: usize::MAX,
        ..Default::default()
    };
    assert!(parse(&files, &limits).is_err());
}
