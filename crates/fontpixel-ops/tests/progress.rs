//! Progress observers and cooperative cancellation for `merge` and `convert`.
#![cfg(all(feature = "bdf", feature = "pcf"))]
use fontpixel::Error;
use fontpixel_ops::{
    Completed, ConvertRequest, MemoryFiles, MergeInput, MergeOutput, MergeRequest, Progress,
    ProgressStep, ReportStyle, Storage,
};
use std::ops::ControlFlow;
use std::path::PathBuf;

const SPLEEN: &[u8] = include_bytes!("../../../tests/data/bdf/spleen-5x8.bdf");
const BLOCK: &[u8] = include_bytes!("../../../tests/data/bdf/3x5-block.bdf");

fn storage() -> Storage {
    Storage::memory(
        MemoryFiles::new()
            .with("fonts/spleen.bdf", SPLEEN)
            .with("fonts/block.bdf", BLOCK),
    )
}
fn merge_request() -> MergeRequest {
    let part = |input: &str, take: &str, dy| MergeInput {
        input: input.into(),
        take: take.parse().unwrap(),
        dy,
        from: None,
        font_index: None,
    };
    let output = |destination: &str| MergeOutput {
        destination: destination.into(),
        format: None,
    };
    let mut request = MergeRequest {
        parts: vec![
            part("fonts/spleen.bdf", "western", 0),
            part("fonts/block.bdf", "all", -1),
        ],
        outputs: vec![output("out/mix.bdf"), output("out/mix.pcf")],
        ..Default::default()
    };
    request.settings.family = "Mix".into();
    request
}
fn convert_each_request() -> ConvertRequest {
    ConvertRequest {
        inputs: vec!["fonts/spleen.bdf".into(), "fonts/block.bdf".into()],
        output: "out".into(),
        to: Some("pcf".into()),
        each: true,
        ..Default::default()
    }
}
/// Outputs as (destination, every asset path and bytes) for byte comparison.
type Assets = Vec<(PathBuf, Vec<(String, Vec<u8>)>)>;
fn assets<T>(done: &Completed<T>) -> Assets {
    done.outputs
        .iter()
        .map(|output| {
            let assets = output
                .bundle
                .assets()
                .iter()
                .map(|asset| (asset.path.to_string(), asset.bytes.clone()))
                .collect();
            (output.destination.clone(), assets)
        })
        .collect()
}
fn event(
    step: ProgressStep,
    done: usize,
    total: usize,
    item: Option<&str>,
    glyphs: Option<usize>,
    bytes: Option<usize>,
) -> Progress {
    Progress {
        step,
        done,
        total,
        item: item.map(str::to_owned),
        glyphs,
        bytes,
    }
}

#[test]
fn merge_progress_reports_every_step_and_equals_merge() {
    let plain = fontpixel_ops::merge(merge_request(), &storage()).unwrap();
    let mut events = Vec::new();
    let observed = fontpixel_ops::merge_with_progress(merge_request(), &storage(), &mut |p| {
        events.push(p.clone());
        ControlFlow::Continue(())
    })
    .unwrap();
    assert_eq!(assets(&observed), assets(&plain));
    let document = |done: &Completed<fontpixel_ops::MergeResponse>| {
        let mut out = Vec::new();
        done.response
            .write(ReportStyle::Json, &mut out, true)
            .unwrap();
        out
    };
    assert_eq!(document(&observed), document(&plain));
    let summaries = &plain.response.summaries;
    let part_glyphs: Vec<usize> = ["spleen", "block"]
        .iter()
        .map(|name| {
            let bytes = if *name == "spleen" { SPLEEN } else { BLOCK };
            fontpixel::registry()
                .read(
                    &fontpixel::Input::new(bytes).with_name("x.bdf"),
                    &fontpixel::ReadOptions::default(),
                )
                .unwrap()
                .fonts
                .fonts[0]
                .len()
        })
        .collect();
    assert_eq!(
        events,
        [
            event(
                ProgressStep::Read,
                1,
                2,
                Some("fonts/spleen.bdf"),
                Some(part_glyphs[0]),
                None
            ),
            event(
                ProgressStep::Read,
                2,
                2,
                Some("fonts/block.bdf"),
                Some(part_glyphs[1]),
                None
            ),
            event(
                ProgressStep::Merge,
                1,
                1,
                None,
                Some(plain.response.glyphs),
                None
            ),
            event(
                ProgressStep::Encode,
                1,
                2,
                Some("out/mix.bdf"),
                None,
                Some(summaries[0].bytes)
            ),
            event(
                ProgressStep::Encode,
                2,
                2,
                Some("out/mix.pcf"),
                None,
                Some(summaries[1].bytes)
            ),
        ]
    );
}

#[test]
fn convert_progress_alternates_reads_and_encodes_and_equals_convert() {
    let plain = fontpixel_ops::convert(convert_each_request(), &storage()).unwrap();
    let mut events = Vec::new();
    let observed =
        fontpixel_ops::convert_with_progress(convert_each_request(), &storage(), &mut |p| {
            events.push(p.clone());
            ControlFlow::Continue(())
        })
        .unwrap();
    assert_eq!(assets(&observed), assets(&plain));
    assert_eq!(observed.response, plain.response);
    let steps: Vec<_> = events.iter().map(|e| (e.step, e.done, e.total)).collect();
    assert_eq!(
        steps,
        [
            (ProgressStep::Read, 1, 2),
            (ProgressStep::Encode, 1, 2),
            (ProgressStep::Read, 2, 2),
            (ProgressStep::Encode, 2, 2),
        ]
    );
    assert_eq!(events[0].item.as_deref(), Some("fonts/spleen.bdf"));
    assert_eq!(events[1].item.as_deref(), Some("out/spleen.pcf"));
    assert_eq!(events[1].bytes, Some(plain.response.outputs[0].bytes));
    assert_eq!(events[3].bytes, Some(plain.response.outputs[1].bytes));

    // Several inputs into one output: every read, then the single encode.
    let request = || ConvertRequest {
        inputs: vec!["fonts/spleen.bdf".into(), "fonts/block.bdf".into()],
        output: "out/both.yaff".into(),
        ..Default::default()
    };
    let mut steps = Vec::new();
    let observed = fontpixel_ops::convert_with_progress(request(), &storage(), &mut |p| {
        steps.push((p.step, p.done, p.total));
        ControlFlow::Continue(())
    });
    let plain = fontpixel_ops::convert(request(), &storage());
    match (observed, plain) {
        (Ok(observed), Ok(plain)) => {
            assert_eq!(assets(&observed), assets(&plain));
            assert_eq!(
                steps,
                [
                    (ProgressStep::Read, 1, 2),
                    (ProgressStep::Read, 2, 2),
                    (ProgressStep::Encode, 1, 1),
                ]
            );
        }
        (Err(observed), Err(plain)) => assert_eq!(observed.to_string(), plain.to_string()),
        (observed, plain) => panic!("observer changed the outcome: {observed:?} vs {plain:?}"),
    }
}

#[test]
fn breaking_at_any_step_cancels_without_outputs() {
    // Merge: five boundaries (two reads, the merge, two encodes).
    for stop in 0..5 {
        let mut seen = 0;
        let result = fontpixel_ops::merge_with_progress(merge_request(), &storage(), &mut |_| {
            seen += 1;
            if seen > stop {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        });
        assert!(
            matches!(result, Err(Error::Cancelled)),
            "merge stop {stop}: {result:?}"
        );
        assert_eq!(seen, stop + 1, "merge stops at the first Break");
    }
    // Convert --each: four boundaries.
    for stop in 0..4 {
        let mut seen = 0;
        let result =
            fontpixel_ops::convert_with_progress(convert_each_request(), &storage(), &mut |_| {
                seen += 1;
                if seen > stop {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            });
        assert!(
            matches!(result, Err(Error::Cancelled)),
            "convert stop {stop}: {result:?}"
        );
        assert_eq!(seen, stop + 1);
    }
    assert_eq!(Error::Cancelled.exit_code(), 1);
    assert!(Error::Cancelled.report().is_none());
}

#[cfg(feature = "fs")]
#[test]
fn cancelling_after_the_last_encode_commits_nothing_on_disk() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../local-work/ops-tests")
        .join(format!("progress-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("fonts")).unwrap();
    std::fs::write(root.join("fonts/spleen.bdf"), SPLEEN).unwrap();
    std::fs::write(root.join("fonts/block.bdf"), BLOCK).unwrap();
    std::fs::write(root.join("mix.bdf"), b"previous").unwrap();
    let mut request = merge_request();
    for (part, name) in request.parts.iter_mut().zip(["spleen", "block"]) {
        part.input = root.join(format!("fonts/{name}.bdf"));
    }
    request.outputs[0].destination = root.join("mix.bdf");
    request.outputs[1].destination = root.join("mix.pcf");
    let result = fontpixel_ops::merge_with_progress(request, &Storage::Filesystem, &mut |p| {
        if p.step == ProgressStep::Encode && p.done == p.total {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    });
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert_eq!(std::fs::read(root.join("mix.bdf")).unwrap(), b"previous");
    assert!(!root.join("mix.pcf").exists());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn progress_is_strict_plain_data() {
    let progress = event(
        ProgressStep::Encode,
        2,
        3,
        Some("out/a.pcf"),
        None,
        Some(42),
    );
    let value = serde_json::to_value(&progress).unwrap();
    assert_eq!(
        value,
        serde_json::json!({
            "step": "encode", "done": 2, "total": 3, "item": "out/a.pcf",
            "glyphs": null, "bytes": 42
        })
    );
    assert_eq!(
        serde_json::from_value::<Progress>(value.clone()).unwrap(),
        progress
    );
    let mut unknown = value;
    unknown["extra"] = 1.into();
    assert!(serde_json::from_value::<Progress>(unknown).is_err());
    for (step, name) in [
        (ProgressStep::Read, "read"),
        (ProgressStep::Merge, "merge"),
        (ProgressStep::Encode, "encode"),
    ] {
        assert_eq!(serde_json::to_value(step).unwrap(), name);
    }
}
