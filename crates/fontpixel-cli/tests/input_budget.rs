//! Regression coverage for retained allocations across multiple CLI inputs.
#[path = "../src/input_budget.rs"]
mod input_budget;
use fontpixel::formats::Loaded;
use fontpixel::model::{Codepoint, Property, PropertyValue};
use fontpixel::report::{Impact, Level, Location};
use fontpixel::{Font, FontSet, Glyph, Report, ReportEntry, ResourceLimits};
use input_budget::InputBudget;

fn loaded() -> Loaded<FontSet> {
    let mut glyph = Glyph::default();
    glyph.codepoints.push(Codepoint::Unicode(65));
    Loaded {
        fonts: FontSet::from(Font::from_glyphs([glyph]).unwrap()),
        report: Report::default(),
        format: "test",
    }
}

#[test]
fn subsequent_decoders_get_only_the_remaining_shared_allowance() {
    let limits = ResourceLimits {
        max_total_bytes: 100_000,
        max_glyphs: 7,
        ..Default::default()
    };
    let mut budget = InputBudget::new(limits.clone());
    assert_eq!(budget.remaining_limits().unwrap().max_glyphs, 7);
    budget.admit(&loaded()).unwrap();
    let remaining = budget.remaining_limits().unwrap();
    assert!(remaining.max_total_bytes < limits.max_total_bytes);
    assert_eq!(remaining.max_glyphs, 6);
    for amount in [
        remaining.max_input_bytes,
        remaining.max_resource_bytes,
        remaining.max_decompressed_bytes,
        remaining.max_table_bytes,
    ] {
        assert!(amount <= remaining.max_total_bytes);
    }
    assert_eq!(remaining.max_resources, limits.max_resources);
    assert_eq!(remaining.max_pixels_per_glyph, limits.max_pixels_per_glyph);
}

#[test]
fn individually_valid_models_cannot_accumulate_past_the_batch_budget() {
    let mut budget = InputBudget::new(ResourceLimits {
        max_total_bytes: 50_000,
        ..Default::default()
    });
    let input = loaded();
    let mut count = 0;
    loop {
        let previous = budget.remaining_limits().unwrap();
        if budget.admit(&input).is_err() {
            assert_eq!(budget.remaining_limits().unwrap(), previous);
            break;
        }
        count += 1;
        assert!(count < 100);
    }
    assert!(count >= 2);
}

#[test]
fn sparse_slots_and_duplicate_aliases_are_counted_as_glyph_ids() {
    let mut input = loaded();
    let font = &mut input.fonts.fonts[0];
    let removed = font.add_glyph(Glyph::default()).unwrap();
    font.remove_glyph(removed).unwrap();
    let first = font.glyph_id(Codepoint::Unicode(65)).unwrap();
    font.edit_glyph(first, |g| {
        g.codepoints
            .extend([Codepoint::Legacy(65), Codepoint::Unicode(66)]);
        Ok(())
    })
    .unwrap();
    let mut budget = InputBudget::new(ResourceLimits {
        max_glyphs: 2,
        ..Default::default()
    });
    budget.admit(&input).unwrap();
    assert_eq!(budget.remaining_limits().unwrap().max_glyphs, 0);
    let previous = budget.remaining_limits().unwrap();
    assert!(budget.admit(&loaded()).is_err());
    assert_eq!(budget.remaining_limits().unwrap(), previous);
}

#[test]
fn diagnostic_strings_and_unused_capacity_cannot_bypass_the_limit() {
    let mut input = loaded();
    input.report.entries = Vec::with_capacity(8);
    let mut asset = String::with_capacity(40_000);
    asset.push('x');
    input.report.push(ReportEntry {
        level: Level::Warning,
        impact: Impact::Loss,
        code: "test.loss".into(),
        location: Location {
            asset: Some(asset),
            ..Default::default()
        },
        message: "loss".into(),
    });
    let mut budget = InputBudget::new(ResourceLimits {
        max_total_bytes: 30_000,
        ..Default::default()
    });
    let before = budget.remaining_limits().unwrap();
    assert!(budget.admit(&input).is_err());
    assert_eq!(budget.remaining_limits().unwrap(), before);
}

#[test]
fn container_properties_and_merge_arrays_are_budgeted_before_moving() {
    let mut input = loaded();
    for _ in 0..20 {
        input
            .fonts
            .properties
            .push(Property::new("opaque", PropertyValue::Bytes(vec![1; 256])));
    }
    input.report.push(ReportEntry {
        level: Level::Info,
        impact: Impact::None,
        code: "test.note".into(),
        location: Location::default(),
        message: "retained".into(),
    });
    let model_bytes = input.fonts.retained_usage(usize::MAX).unwrap().bytes;
    // The complete model alone fits. Retaining diagnostics and extending the
    // receiving font/property/report arrays additionally requires space.
    let mut budget = InputBudget::new(ResourceLimits {
        max_total_bytes: model_bytes,
        ..Default::default()
    });
    assert!(budget.admit(&input).is_err());
    assert_eq!(
        budget.remaining_limits().unwrap().max_total_bytes,
        model_bytes
    );
}

#[test]
fn retained_font_arrays_are_charged_even_after_all_fonts_are_removed() {
    let mut input = loaded();
    input.fonts.fonts = Vec::with_capacity(100);
    let mut budget = InputBudget::new(ResourceLimits {
        max_total_bytes: 1000,
        ..Default::default()
    });
    assert!(budget.admit(&input).is_err());
}

#[cfg(all(feature = "raw", feature = "yaff"))]
#[test]
fn cli_rejects_cumulative_inputs_before_replacing_an_existing_output() {
    use fontpixel::formats::raw::{RawLayout, RawReadOptions};
    use std::process::Command;
    // Seventeen real cells provide enough retained model storage for the
    // standalone identity slice, without making a large bitmap dominate the
    // decoder's transient budget. The same cap must still complete one CLI write.
    let input = vec![0x80; 17];
    let (fonts, report) = fontpixel::formats::raw::read(
        &input,
        &RawReadOptions {
            layout: Some(RawLayout::new(1, 1)),
        },
        &ResourceLimits::default(),
    )
    .unwrap();
    assert_eq!(fonts.fonts.len(), 1);
    assert_eq!(fonts.fonts[0].len(), input.len());
    let loaded = Loaded {
        fonts,
        report,
        format: "raw",
    };
    let mut probe = InputBudget::new(ResourceLimits::default());
    probe.admit(&loaded).unwrap();
    let one = ResourceLimits::default().max_total_bytes
        - probe.remaining_limits().unwrap().max_total_bytes;
    let working = one.checked_mul(2).unwrap() - 1;
    // Below the 1 MiB identity ceiling, solve L - floor(L / 256) = working.
    // This is the smallest such L, including the plateau at multiples of 255.
    assert!(working < 255 * 1024 * 1024);
    let total = working.checked_add((working - 1) / 255).unwrap();
    let identity = (total / 256).min(1024 * 1024);
    assert_eq!(total - identity, working);
    let mut cumulative = InputBudget::new(ResourceLimits {
        max_total_bytes: working,
        ..Default::default()
    });
    cumulative.admit(&loaded).unwrap();
    assert!(matches!(
        cumulative.admit(&loaded),
        Err(fontpixel::Error::ResourceLimit {
            resource: "retained CLI input bytes",
            ..
        })
    ));
    let directory =
        std::env::temp_dir().join(format!("fontpixel-input-budget-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(directory.clone());
    let source = directory.join("one.raw");
    let target = directory.join("out.yaff");
    std::fs::write(&source, &input).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_fontpixel"));
    command
        .args([
            "convert",
            "--from",
            "raw",
            "--raw-width",
            "1",
            "--raw-height",
            "1",
            "--to",
            "yaff",
            "-o",
        ])
        .arg(&target)
        .arg(&source)
        .arg("--max-total-bytes")
        .arg(total.to_string());
    let good = command.output().unwrap();
    assert!(
        good.status.success(),
        "{}",
        String::from_utf8_lossy(&good.stderr)
    );
    std::fs::write(&target, b"original output").unwrap();
    command.arg(&source);
    let failed = command.output().unwrap();
    assert!(!failed.status.success());
    assert!(
        String::from_utf8_lossy(&failed.stderr).contains("retained CLI input bytes"),
        "{}",
        String::from_utf8_lossy(&failed.stderr)
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"original output");
}

#[test]
#[cfg(feature = "opentype")]
fn language_route_clones_reserve_before_allocation_and_fail_atomically() {
    let input = loaded();
    let mut budget = InputBudget::new(ResourceLimits {
        max_glyphs: 2,
        max_total_bytes: 100_000,
        ..Default::default()
    });
    budget.admit(&input).unwrap();
    let before = budget.remaining_limits().unwrap();
    budget.reserve_font_clone(&input.fonts.fonts[0]).unwrap();
    let after = budget.remaining_limits().unwrap();
    assert_eq!(after.max_glyphs, 0);
    assert!(after.max_total_bytes < before.max_total_bytes);
    assert!(budget.reserve_font_clone(&input.fonts.fonts[0]).is_err());
    assert_eq!(budget.remaining_limits().unwrap(), after);
}
