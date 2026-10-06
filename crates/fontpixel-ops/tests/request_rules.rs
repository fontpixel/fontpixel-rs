//! Requests built without the command-line parser obey the same rules.
//!
//! Every case pairs a command line with the equivalent flat request map. The
//! parser and strict deserialization (which runs `validate()`) must agree: both
//! accept (and produce equal requests) or both reject with the same message.
#![cfg(all(feature = "clap", feature = "serde", feature = "opentype"))]
use clap::{CommandFactory, Parser};
use fontpixel_ops::cli::{Cli, Command};
use fontpixel_ops::{
    CharsetRequest, ConvertRequest, DiffRequest, FormatsRequest, InfoRequest, MergeRequest,
    PreviewRequest, RenderRequest, SubsetRequest, TaskKind, TaskRequest, ValidateRequest,
    VariantsRequest,
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::fmt::Debug;

/// The parser's message without the `error: ` prefix and usage footer.
fn clap_message(error: &clap::Error) -> String {
    let text = error.to_string();
    let text = text.strip_prefix("error: ").unwrap_or(&text);
    let end = ["\n\nUsage:", "\n\nFor more information"]
        .iter()
        .filter_map(|marker| text.find(marker))
        .min()
        .unwrap_or(text.len());
    text[..end].trim_end().to_owned()
}

fn check<T: DeserializeOwned + PartialEq + Debug>(
    parsed: Result<T, String>,
    request: Value,
    args: &[&str],
) {
    let built: Result<T, String> = serde_json::from_value(request).map_err(|e| e.to_string());
    match (&parsed, &built) {
        (Ok(a), Ok(b)) => assert_eq!(a, b, "{args:?}"),
        (Err(a), Err(b)) => assert_eq!(a, b, "{args:?}"),
        _ => panic!("parser and request disagree for {args:?}: {parsed:?} vs {built:?}"),
    }
}

/// Parse `args` and compare with `request` deserialized as the command's type.
fn case(args: &[&str], request: Value) {
    let mut argv = vec!["fontpixel"];
    argv.extend_from_slice(args);
    let parsed = Cli::try_parse_from(&argv).map(|cli| cli.command);
    let err = |e: &clap::Error| clap_message(e);
    macro_rules! compare {
        ($variant:ident, $ty:ty) => {
            check::<$ty>(
                match parsed {
                    Ok(Command::$variant(r)) => Ok(*r),
                    Ok(other) => panic!("unexpected command {other:?}"),
                    Err(ref e) => Err(err(e)),
                },
                request,
                args,
            )
        };
    }
    match args[0] {
        "convert" => compare!(Convert, ConvertRequest),
        "preview" => compare!(Preview, PreviewRequest),
        "render" => compare!(Render, RenderRequest),
        "info" => compare!(Info, InfoRequest),
        "diff" => compare!(Diff, DiffRequest),
        "validate" => compare!(Validate, ValidateRequest),
        "charset" => compare!(Charset, CharsetRequest),
        "merge" => compare!(Merge, MergeRequest),
        "variants" => compare!(Variants, VariantsRequest),
        "subset" => compare!(Subset, SubsetRequest),
        other => panic!("no case for {other}"),
    }
}

#[test]
fn required_arguments_match_the_parser() {
    case(&["convert", "-o", "x.pcf"], json!({"output": "x.pcf"}));
    case(&["convert", "a.bdf"], json!({"inputs": ["a.bdf"]}));
    case(
        &["convert", "a.bdf", "-o", "x.pcf"],
        json!({"inputs": ["a.bdf"], "output": "x.pcf"}),
    );
    case(&["info"], json!({}));
    case(&["info", "a.bdf"], json!({"input": "a.bdf"}));
    case(&["validate"], json!({}));
    case(&["diff", "a.bdf"], json!({"a": "a.bdf"}));
    case(
        &["diff", "a.bdf", "b.bdf"],
        json!({"a": "a.bdf", "b": "b.bdf"}),
    );
    case(
        &["charset", "a.bdf", "--to", "cp437"],
        json!({"input": "a.bdf", "to": "cp437"}),
    );
    case(
        &["render", "a.bdf", "Hi"],
        json!({"input": "a.bdf", "text": "Hi"}),
    );
    case(&["variants", "-o", "v.ttf"], json!({"output": "v.ttf"}));
    case(&["variants", "--base", "a.bdf"], json!({"base": "a.bdf"}));
    case(
        &["subset", "--text", "A", "-o", "s.ttf"],
        json!({"text": "A", "output": "s.ttf"}),
    );
    case(
        &["merge", "--part", "a.bdf", "--take", "all", "-o", "m.bdf"],
        json!({"parts": [{"input": "a.bdf", "take": "all"}], "outputs": [{"destination": "m.bdf"}]}),
    );
    case(
        &[
            "merge", "--family", "F", "--part", "a.bdf", "--take", "all", "-o", "m.bdf",
        ],
        json!({"family": "F", "parts": [{"input": "a.bdf", "take": "all"}], "outputs": [{"destination": "m.bdf"}]}),
    );
}

#[test]
fn layout_rules_match_the_parser() {
    case(&["preview", "a.bdf"], json!({"input": "a.bdf"}));
    case(
        &["preview", "a.bdf", "Hi", "--all"],
        json!({"input": "a.bdf", "text": "Hi", "all": true}),
    );
    case(
        &[
            "preview", "a.bdf", "--all", "--range", "U+20-7E", "--order", "file",
        ],
        json!({"input": "a.bdf", "all": true, "range": "U+20-7E", "order": "file"}),
    );
    case(
        &["preview", "a.bdf", "--range", "U+20-7E"],
        json!({"input": "a.bdf", "range": "U+20-7E"}),
    );
    case(
        &["preview", "a.bdf", "Hi", "--range", "U+20-7E"],
        json!({"input": "a.bdf", "text": "Hi", "range": "U+20-7E"}),
    );
    case(
        &["preview", "a.bdf", "Hi", "--order", "descending"],
        json!({"input": "a.bdf", "text": "Hi", "order": "descending"}),
    );
    case(
        &["preview", "a.bdf", "Hi", "--missing", "?", "--fail-missing"],
        json!({"input": "a.bdf", "text": "Hi", "missing": "?", "fail_missing": true}),
    );
    case(
        &["preview", "a.bdf", "Hi", "--missing", "?"],
        json!({"input": "a.bdf", "text": "Hi", "missing": "?"}),
    );
}

#[test]
fn input_option_rules_match_the_parser() {
    let base = || json!({"inputs": ["a.h"], "output": "x.bdf"});
    let with = |extra: Value| {
        let mut value = base();
        value
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        value
    };
    let convert = |extra: &[&str], fields: Value| {
        let mut args = vec!["convert", "a.h", "-o", "x.bdf"];
        args.extend_from_slice(extra);
        case(&args, with(fields));
    };
    convert(&["--c-width", "8"], json!({"c_width": 8}));
    convert(
        &["--c-width", "8", "--c-height", "8"],
        json!({"c_width": 8, "c_height": 8}),
    );
    convert(
        &[
            "--c-width",
            "8",
            "--c-height",
            "8",
            "--c-first",
            "32",
            "--c-charset",
            "unicode",
        ],
        json!({"c_width": 8, "c_height": 8, "c_first": "32", "c_charset": "unicode"}),
    );
    convert(&["--c-height", "8"], json!({"c_height": 8}));
    convert(&["--c-first", "32"], json!({"c_first": "32"}));
    convert(&["--c-charset", "unicode"], json!({"c_charset": "unicode"}));
    convert(&["--c-baseline=-1"], json!({"c_baseline": -1}));
    convert(&["--c-lsb-first"], json!({"c_lsb_first": true}));
    convert(&["--grid-units", "64"], json!({"grid_units": 64}));
    convert(
        &["--grid-units", "64", "--ppem", "16"],
        json!({"grid_units": 64, "ppem": 16}),
    );
    convert(
        &["--playdate-read-cell-width", "8"],
        json!({"playdate_read_cell_width": 8}),
    );
    convert(
        &["--playdate-read-cell-height", "8"],
        json!({"playdate_read_cell_height": 8}),
    );
    convert(
        &[
            "--fbit-apple-double-resource",
            "r",
            "--fbit-read-data-fork",
            "d",
        ],
        json!({"fbit_apple_double_resource": "r", "fbit_read_data_fork": "d"}),
    );
}

#[test]
fn output_option_rules_match_the_parser() {
    let convert = |extra: &[&str], fields: Value| {
        let mut args = vec!["convert", "a.bdf", "-o", "x.bin"];
        args.extend_from_slice(extra);
        let mut value = json!({"inputs": ["a.bdf"], "output": "x.bin"});
        value
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        case(&args, value);
    };
    convert(&["--each"], json!({"each": true}));
    convert(
        &["--each", "--to", "pcf"],
        json!({"each": true, "to": "pcf"}),
    );
    convert(&["--swidth-units", "1000"], json!({"swidth_units": 1000}));
    convert(
        &["--swidth-units", "1000", "--swidth-ppem", "16"],
        json!({"swidth_units": 1000, "swidth_ppem": 16}),
    );
    convert(&["--point-size", "12"], json!({"point_size": 12}));
    convert(&["--resolution", "72,72"], json!({"resolution": [72, 72]}));
    convert(
        &["--point-size", "12", "--resolution", "72,72"],
        json!({"point_size": 12, "resolution": [72, 72]}),
    );
    convert(&["--otb-created", "5"], json!({"otb_created": 5}));
    convert(&["--otb-modified", "5"], json!({"otb_modified": 5}));
    convert(
        &["--otb-created", "5", "--otb-modified", "6"],
        json!({"otb_created": 5, "otb_modified": 6}),
    );
    convert(
        &["--otb-index-min-run", "3"],
        json!({"otb_index_min_run": 3}),
    );
    convert(
        &[
            "--otb-index-min-run",
            "3",
            "--otb-index-layout",
            "shared-runs",
        ],
        json!({"otb_index_min_run": 3, "otb_index_layout": "shared-runs"}),
    );
    convert(
        &["--playdate-cell-width", "8"],
        json!({"playdate_cell_width": 8}),
    );
    convert(
        &["--playdate-cell-height", "8"],
        json!({"playdate_cell_height": 8}),
    );
    convert(
        &["--hppcl-x-resolution", "300"],
        json!({"hppcl_x_resolution": 300}),
    );
    convert(
        &["--hppcl-y-resolution", "300"],
        json!({"hppcl_y_resolution": 300}),
    );
    convert(
        &["--vtfont-cell-width", "8"],
        json!({"vtfont_cell_width": 8}),
    );
    convert(
        &["--vtfont-cell-height", "8"],
        json!({"vtfont_cell_height": 8}),
    );
    convert(
        &["--symbos-writing-system", "3", "--symbos-layout", "screen"],
        json!({"symbos_writing_system": 3, "symbos_layout": "screen"}),
    );
    convert(
        &["--riscos-metrics-version", "2"],
        json!({"riscos_metrics_version": "2"}),
    );
    convert(
        &["--native-cell-width", "8"],
        json!({"native_cell_width": 8}),
    );
    convert(
        &["--native-cell-height", "8"],
        json!({"native_cell_height": 8}),
    );
    convert(&["--drcs-cell-width", "8"], json!({"drcs_cell_width": 8}));
    convert(&["--drcs-cell-height", "8"], json!({"drcs_cell_height": 8}));
    convert(&["--drcs-cell-x", "1"], json!({"drcs_cell_x": 1}));
    convert(&["--drcs-cell-y=-1"], json!({"drcs_cell_y": -1}));
    convert(
        &[
            "--drcs-cell-width",
            "8",
            "--drcs-cell-height",
            "8",
            "--drcs-cell-x",
            "1",
        ],
        json!({"drcs_cell_width": 8, "drcs_cell_height": 8, "drcs_cell_x": 1}),
    );
    // Ranges and choice lists are checked on deserialization as well.
    convert(&["--swidth-ppem", "0"], json!({"swidth_ppem": 0}));
    convert(&["--shape", "dot"], json!({"shape": "dot"}));
}

#[test]
fn subset_selection_group_matches_the_parser() {
    let subset = |extra: &[&str], fields: Value| {
        let mut args = vec!["subset", "a.ttf", "-o", "s.ttf"];
        args.extend_from_slice(extra);
        let mut value = json!({"input": "a.ttf", "output": "s.ttf"});
        value
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        case(&args, value);
    };
    subset(&[], json!({}));
    subset(&["--text", "A"], json!({"text": "A"}));
    subset(&["--unicodes", "U+41"], json!({"unicodes": "U+41"}));
    subset(&["--text-file", "t.txt"], json!({"text_file": "t.txt"}));
    subset(
        &["--text", "A", "--unicodes", "U+41"],
        json!({"text": "A", "unicodes": "U+41"}),
    );
    subset(
        &["--text", "A", "--text-file", "t.txt"],
        json!({"text": "A", "text_file": "t.txt"}),
    );
    subset(
        &["--unicodes", "U+41", "--text-file", "t.txt"],
        json!({"unicodes": "U+41", "text_file": "t.txt"}),
    );
}

#[test]
fn variants_metadata_rules_match_the_parser() {
    let variants = |extra: &[&str], fields: Value| {
        let mut args = vec!["variants", "--base", "a.bdf", "-o", "v.ttf"];
        args.extend_from_slice(extra);
        let mut value = json!({"base": "a.bdf", "output": "v.ttf"});
        value
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        case(&args, value);
    };
    variants(&["--created", "1"], json!({"created": 1}));
    variants(&["--modified", "1"], json!({"modified": 1}));
    variants(
        &[
            "--created",
            "1",
            "--modified",
            "2",
            "--shape",
            "square-separate",
        ],
        json!({"created": 1, "modified": 2, "shape": "square-separate"}),
    );
}

/// Keys of the serialized default request.
fn keys<T: serde::Serialize + Default>() -> Vec<String> {
    let mut keys: Vec<String> = serde_json::to_value(T::default())
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

#[test]
fn request_keys_are_the_command_line_argument_ids() {
    let command = Cli::command();
    let ids = |name: &str| {
        let mut ids: Vec<String> = command
            .find_subcommand(name)
            .unwrap()
            .get_arguments()
            .map(|arg| arg.get_id().to_string())
            .filter(|id| id != "help")
            .collect();
        ids.sort();
        ids
    };
    assert_eq!(keys::<ConvertRequest>(), ids("convert"));
    assert_eq!(keys::<InfoRequest>(), ids("info"));
    assert_eq!(keys::<FormatsRequest>(), ids("formats"));
    assert_eq!(keys::<PreviewRequest>(), ids("preview"));
    assert_eq!(keys::<RenderRequest>(), ids("render"));
    assert_eq!(keys::<DiffRequest>(), ids("diff"));
    assert_eq!(keys::<ValidateRequest>(), ids("validate"));
    assert_eq!(keys::<CharsetRequest>(), ids("charset"));
    assert_eq!(keys::<VariantsRequest>(), ids("variants"));
    assert_eq!(keys::<SubsetRequest>(), ids("subset"));
    // Merge pairs its repeated part/output options into typed lists.
    let mut merge = ids("merge");
    merge.retain(|id| {
        !["parts", "take", "dy", "from", "font_index", "outputs", "to"].contains(&id.as_str())
    });
    merge.extend(["outputs".to_owned(), "parts".to_owned()]);
    merge.sort();
    assert_eq!(keys::<MergeRequest>(), merge);
}

#[test]
fn unknown_keys_are_rejected_at_every_level() {
    let err = |value: Value| {
        serde_json::from_value::<ConvertRequest>(value)
            .unwrap_err()
            .to_string()
    };
    // A top-level typo and a typo of a key from a nested flattened group.
    assert_eq!(
        err(json!({"inputs": ["a.bdf"], "output": "b.pcf", "stirct": true})),
        "unknown field `stirct`"
    );
    assert_eq!(
        err(json!({"inputs": ["a.bdf"], "output": "b.pcf", "max_total_byte": 1})),
        "unknown field `max_total_byte`"
    );
    // Keys of every flattened group (raster → p3 → otb metadata) are known.
    let ok: ConvertRequest = serde_json::from_value(json!({
        "inputs": ["a.bdf"], "output": "b.otb", "max_total_bytes": 1, "otb_upm": 1000,
        "playdate_read_cell_width": 8, "playdate_read_cell_height": 8, "psf_version": "1"
    }))
    .unwrap();
    assert_eq!(ok.raster.max_total_bytes, Some(1));
    // Groups deserialized on their own are strict too.
    assert!(
        serde_json::from_value::<fontpixel_ops::options::RasterOptions>(json!({"output": "x"}))
            .is_err()
    );
    // Merge parts and outputs reject unknown keys.
    let merge = |part: Value, output: Value| {
        serde_json::from_value::<MergeRequest>(
            json!({"family": "F", "parts": [part], "outputs": [output]}),
        )
    };
    assert!(
        merge(
            json!({"input": "a.bdf", "take": "all"}),
            json!({"destination": "m.bdf"})
        )
        .is_ok()
    );
    assert!(
        merge(
            json!({"input": "a.bdf", "take": "all", "dx": 1}),
            json!({"destination": "m.bdf"})
        )
        .unwrap_err()
        .to_string()
        .contains("unknown field `dx`")
    );
    assert!(
        merge(
            json!({"input": "a.bdf", "take": "all"}),
            json!({"destination": "m.bdf", "fmt": "pcf"})
        )
        .unwrap_err()
        .to_string()
        .contains("unknown field `fmt`")
    );
    // Tasks reject unknown keys of the wrapped request.
    assert!(
        serde_json::from_value::<TaskRequest>(
            json!({"kind": "validate", "input": "a.bdf", "jsn": true})
        )
        .unwrap_err()
        .to_string()
        .contains("unknown field `jsn`")
    );
    // Non-map input is rejected with a clear message.
    assert_eq!(
        serde_json::from_value::<InfoRequest>(json!(["a.bdf"]))
            .unwrap_err()
            .to_string(),
        "expected a map of option names to values"
    );
}

#[test]
fn choice_options_are_typed_or_checked_on_deserialization() {
    // Typed enums share the command-line spellings.
    let merge: MergeRequest = serde_json::from_value(json!({
        "family": "F", "spacing": "prop", "geometric_width": "full",
        "parts": [{"input": "a.bdf", "take": "cjk"}], "outputs": [{"destination": "m.bdf"}]
    }))
    .unwrap();
    assert_eq!(merge.settings.spacing, fontpixel::merge::MergeSpacing::Prop);
    assert_eq!(
        merge.settings.geometric_width,
        fontpixel::merge::GeometricWidth::Full
    );
    let value = serde_json::to_value(&merge).unwrap();
    assert_eq!(value["spacing"], "prop");
    assert_eq!(value["geometric_width"], "full");
    for (spelling, shape) in [
        ("square", fontpixel::outline::Shape::Square),
        ("square-separate", fontpixel::outline::Shape::SquareSeparate),
        ("dot", fontpixel::outline::Shape::Dot),
    ] {
        let Ok(Command::Render(render)) = Cli::try_parse_from([
            "fontpixel",
            "render",
            "a.bdf",
            "Hi",
            "-o",
            "x.svg",
            "--shape",
            spelling,
        ])
        .map(|cli| cli.command) else {
            panic!("render")
        };
        assert_eq!(render.shape, Some(shape));
        assert_eq!(serde_json::to_value(shape).unwrap(), spelling);
    }
    // String choices are checked against the shared lists.
    assert_eq!(
        serde_json::from_value::<ConvertRequest>(
            json!({"inputs": ["a.bdf"], "output": "b.pcf", "glyph_pad": "3"})
        )
        .unwrap_err()
        .to_string(),
        "invalid value '3' for glyph_pad [possible values: 1, 2, 4, 8]"
    );
    assert!(
        serde_json::from_value::<MergeRequest>(json!({
            "family": "F", "spacing": "wide",
            "parts": [{"input": "a.bdf", "take": "cjk"}], "outputs": [{"destination": "m.bdf"}]
        }))
        .is_err()
    );
}

#[test]
fn tasks_accept_every_batch_kind_including_vectorize() {
    let task = |kind: &str| {
        serde_json::from_value::<TaskRequest>(json!({
            "kind": kind, "inputs": ["a.bdf"], "output": "a.ttf"
        }))
        .unwrap()
    };
    let vectorize = task("vectorize");
    assert_eq!(vectorize.kind(), TaskKind::Vectorize);
    assert_eq!(vectorize.kind().name(), "vectorize");
    assert_eq!(task("convert").kind(), TaskKind::Convert);
    let TaskRequest::Vectorize(request) = &vectorize else {
        panic!("vectorize")
    };
    assert_eq!(request.inputs, vec![std::path::PathBuf::from("a.bdf")]);
    assert_eq!(
        serde_json::to_value(&vectorize).unwrap()["kind"],
        "vectorize"
    );
    assert_eq!(
        vectorize.declared_inputs(),
        vec![std::path::PathBuf::from("a.bdf")]
    );
}
