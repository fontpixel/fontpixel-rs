//! Typed request defaults equal the command-line defaults, and requests
//! round-trip through serde as flat option maps.
#![cfg(all(feature = "clap", feature = "opentype"))]
use clap::Parser;
use fontpixel_ops::cli::{Cli, Command};
use fontpixel_ops::{
    CharsetRequest, ConvertRequest, DiffRequest, FormatsRequest, InfoRequest, MergeInput,
    MergeOutput, MergeRequest, PreviewRequest, RenderRequest, SfntOutputOptions, SubsetRequest,
    ValidateRequest, VariantsRequest,
};
use std::path::PathBuf;

fn parse(args: &[&str]) -> Command {
    let mut argv = vec!["fontpixel"];
    argv.extend_from_slice(args);
    Cli::try_parse_from(argv).unwrap().command
}

#[test]
fn defaults_match_the_command_line() {
    let Command::Convert(convert) = parse(&["convert", "a.bdf", "-o", "b.pcf"]) else {
        panic!("convert")
    };
    assert_eq!(
        *convert,
        ConvertRequest {
            inputs: vec!["a.bdf".into()],
            output: "b.pcf".into(),
            ..Default::default()
        }
    );
    let Command::Info(info) = parse(&["info", "a.bdf"]) else {
        panic!("info")
    };
    assert_eq!(
        *info,
        InfoRequest {
            input: "a.bdf".into(),
            ..Default::default()
        }
    );
    let Command::Formats(formats) = parse(&["formats"]) else {
        panic!("formats")
    };
    assert_eq!(formats, FormatsRequest::default());
    let Command::Preview(preview) = parse(&["preview", "a.bdf", "Hi"]) else {
        panic!("preview")
    };
    let mut expected = PreviewRequest::default();
    expected.layout.input = "a.bdf".into();
    expected.layout.text = Some("Hi".into());
    assert_eq!(*preview, expected);
    let Command::Render(render) = parse(&["render", "a.bdf", "Hi", "-o", "x.svg"]) else {
        panic!("render")
    };
    let mut expected = RenderRequest::default();
    expected.layout.input = "a.bdf".into();
    expected.layout.text = Some("Hi".into());
    expected.output = "x.svg".into();
    assert_eq!(*render, expected);
    let Command::Diff(diff) = parse(&["diff", "a.bdf", "b.bdf"]) else {
        panic!("diff")
    };
    assert_eq!(
        *diff,
        DiffRequest {
            a: "a.bdf".into(),
            b: "b.bdf".into(),
            ..Default::default()
        }
    );
    let Command::Validate(validate) = parse(&["validate", "a.bdf"]) else {
        panic!("validate")
    };
    assert_eq!(
        *validate,
        ValidateRequest {
            input: "a.bdf".into(),
            ..Default::default()
        }
    );
    let Command::Charset(charset) = parse(&["charset", "a.bdf", "-o", "b.bdf", "--to", "cp1252"])
    else {
        panic!("charset")
    };
    assert_eq!(
        *charset,
        CharsetRequest {
            input: "a.bdf".into(),
            output: "b.bdf".into(),
            to: "cp1252".into(),
            ..Default::default()
        }
    );
    let Command::Variants(variants) = parse(&["variants", "--base", "a.bdf", "-o", "v.ttf"]) else {
        panic!("variants")
    };
    assert_eq!(
        *variants,
        VariantsRequest {
            base: "a.bdf".into(),
            output: SfntOutputOptions {
                output: "v.ttf".into(),
                ..Default::default()
            },
            ..Default::default()
        }
    );
    let Command::Subset(subset) = parse(&["subset", "a.ttf", "--text", "A", "-o", "s.ttf"]) else {
        panic!("subset")
    };
    assert_eq!(
        *subset,
        SubsetRequest {
            input: "a.ttf".into(),
            text: Some("A".into()),
            output: SfntOutputOptions {
                output: "s.ttf".into(),
                ..Default::default()
            },
            ..Default::default()
        }
    );
    let Command::Merge(merge) = parse(&[
        "merge", "--family", "Mix", "--part", "a.bdf", "--take", "cjk", "--dy=-1", "--part",
        "b.bdf", "--take", "all", "-o", "m.bdf", "-o", "m.dat", "--to", "pcf",
    ]) else {
        panic!("merge")
    };
    let mut expected = MergeRequest {
        parts: vec![
            MergeInput {
                input: "a.bdf".into(),
                take: "cjk".parse().unwrap(),
                dy: -1,
                from: None,
                font_index: None,
            },
            MergeInput {
                input: "b.bdf".into(),
                take: "all".parse().unwrap(),
                dy: 0,
                from: None,
                font_index: None,
            },
        ],
        outputs: vec![
            MergeOutput {
                destination: "m.bdf".into(),
                format: None,
            },
            MergeOutput {
                destination: PathBuf::from("m.dat"),
                format: Some("pcf".into()),
            },
        ],
        ..Default::default()
    };
    expected.settings.family = "Mix".into();
    assert_eq!(*merge, expected);
}

#[cfg(feature = "serde")]
#[test]
fn requests_round_trip_as_flat_option_maps() {
    let Command::Convert(convert) = parse(&[
        "convert",
        "a.bdf",
        "b.bdf",
        "-o",
        "out.otb",
        "--otb-upm",
        "1000",
        "--otb-reference",
        "largest",
        "--family",
        "Fam",
        "--psf-version",
        "1",
        "--max-total-bytes",
        "1234567",
        "--shape",
        "dot",
    ]) else {
        panic!("convert")
    };
    let value = serde_json::to_value(&*convert).unwrap();
    // Every group is flattened: options use their command-line names.
    assert_eq!(value["otb_upm"], 1000);
    assert_eq!(value["otb_reference"], "largest");
    assert_eq!(value["family"], "Fam");
    assert_eq!(value["psf_version"], "1");
    assert_eq!(value["max_total_bytes"], 1234567);
    let back: ConvertRequest = serde_json::from_value(value).unwrap();
    assert_eq!(back, *convert);
    // Missing keys take the command-line defaults.
    let sparse: ConvertRequest =
        serde_json::from_value(serde_json::json!({"inputs": ["a.bdf"], "output": "b.pcf"}))
            .unwrap();
    assert_eq!(
        sparse,
        ConvertRequest {
            inputs: vec!["a.bdf".into()],
            output: "b.pcf".into(),
            ..Default::default()
        }
    );
    let Command::Render(render) = parse(&[
        "render",
        "a.bdf",
        "Hi",
        "-o",
        "x.png",
        "--color",
        "#123",
        "--direction",
        "tbrl",
    ]) else {
        panic!("render")
    };
    let value = serde_json::to_value(&*render).unwrap();
    // Colors and directions use the core serde shapes.
    assert_eq!(
        value["color"],
        serde_json::json!({"red": 17, "green": 34, "blue": 51, "alpha": 255})
    );
    assert_eq!(
        value["direction"],
        serde_json::json!({"glyphs": "down", "lines": "left"})
    );
    let back: RenderRequest = serde_json::from_value(value).unwrap();
    assert_eq!(back, *render);
    let mut merge = MergeRequest {
        parts: vec![MergeInput {
            input: "a.bdf".into(),
            take: "western,geometric".parse().unwrap(),
            dy: 2,
            from: None,
            font_index: Some(1),
        }],
        outputs: vec![MergeOutput {
            destination: "m.bdf".into(),
            format: None,
        }],
        ..Default::default()
    };
    merge.settings.family = "Mix".into();
    let value = serde_json::to_value(&merge).unwrap();
    assert_eq!(value["parts"][0]["take"], "western,geometric");
    let back: MergeRequest = serde_json::from_value(value).unwrap();
    assert_eq!(back, merge);
    let task: fontpixel_ops::TaskRequest = serde_json::from_value(serde_json::json!({
        "kind": "validate",
        "input": "a.bdf",
        "json": true
    }))
    .unwrap();
    assert_eq!(task.kind(), fontpixel_ops::TaskKind::Validate);
}
