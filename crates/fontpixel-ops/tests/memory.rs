//! Every operation in memory storage, checked against the filesystem mode
//! where that mode exists.
#![cfg(all(feature = "bdf", feature = "pcf"))]
use fontpixel::Error;
use fontpixel_ops::{
    BatchRequest, BatchTask, CharsetRequest, ConvertRequest, DiffRequest, FormatsRequest,
    InfoRequest, MemoryFiles, MergeInput, MergeOutput, MergeRequest, PreparedOutput,
    PreviewRequest, RenderRequest, ReportStyle, Storage, TaskRequest, ValidateRequest,
    ValidationKind,
};
use std::path::{Path, PathBuf};

const SPLEEN: &[u8] = include_bytes!("../../../tests/data/bdf/spleen-5x8.bdf");
const BLOCK: &[u8] = include_bytes!("../../../tests/data/bdf/3x5-block.bdf");
#[cfg(feature = "bdat")]
const EMBEDDED_TTF: &[u8] = include_bytes!("../../../tests/data/embedded-bitmap/bdat-i1-f1-d1.ttf");

fn files() -> MemoryFiles {
    MemoryFiles::new()
        .with("fonts/spleen.bdf", SPLEEN)
        .with("fonts/block.bdf", BLOCK)
}
fn storage() -> Storage {
    Storage::memory(files())
}
fn output<'a>(outputs: &'a [PreparedOutput], destination: &str) -> &'a PreparedOutput {
    outputs
        .iter()
        .find(|o| o.destination == Path::new(destination))
        .unwrap_or_else(|| panic!("missing output {destination}"))
}
fn primary(output: &PreparedOutput) -> Vec<u8> {
    let name = output.bundle.primary();
    output
        .bundle
        .assets()
        .iter()
        .find(|asset| asset.path == *name)
        .expect("primary asset")
        .bytes
        .clone()
}
fn convert_request(inputs: &[&str], output: &str) -> ConvertRequest {
    ConvertRequest {
        inputs: inputs.iter().map(PathBuf::from).collect(),
        output: output.into(),
        ..Default::default()
    }
}
/// A scratch directory under the shared local-work tree, removed on drop.
#[cfg(feature = "fs")]
struct Scratch(PathBuf);
#[cfg(feature = "fs")]
impl Scratch {
    fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../local-work/ops-tests")
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("fonts")).unwrap();
        std::fs::write(path.join("fonts/spleen.bdf"), SPLEEN).unwrap();
        std::fs::write(path.join("fonts/block.bdf"), BLOCK).unwrap();
        Self(path)
    }
}
#[cfg(feature = "fs")]
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn formats_lists_the_catalogue_with_compiled_status() {
    let response = fontpixel_ops::formats(&FormatsRequest::default());
    assert!(
        response
            .formats
            .iter()
            .any(|f| f.format.id == "bdf" && f.read && f.write)
    );
    let mut json = Vec::new();
    response.write_json(&mut json, true).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(
        value["formats"].as_array().unwrap().len(),
        response.formats.len()
    );
    let mut text = Vec::new();
    response.write_text(&mut text).unwrap();
    assert!(text.starts_with(b"ID\tREAD\tWRITE"));
}

#[test]
fn info_reads_named_buffers_and_reports_missing_keys() {
    let request = InfoRequest {
        input: "fonts/spleen.bdf".into(),
        ..Default::default()
    };
    let response = fontpixel_ops::info(request, &storage()).unwrap();
    assert_eq!(response.format, "bdf");
    assert_eq!(response.fonts.fonts.len(), 1);
    let mut json = Vec::new();
    response.write_json(&mut json, false).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(value["font_count"], 1);
    assert_eq!(value["format"], "bdf");
    let missing = fontpixel_ops::info(
        InfoRequest {
            input: "fonts/nope.bdf".into(),
            ..Default::default()
        },
        &storage(),
    );
    assert!(matches!(missing, Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound));
}

#[test]
fn convert_returns_outputs_and_summaries_without_touching_storage() {
    let done = fontpixel_ops::convert(
        convert_request(&["fonts/spleen.bdf"], "out/spleen.pcf"),
        &storage(),
    )
    .unwrap();
    assert_eq!(done.outputs.len(), 1);
    let pcf = output(&done.outputs, "out/spleen.pcf");
    let summary = &done.response.outputs[0];
    assert_eq!(summary.format, "pcf");
    assert_eq!(summary.bytes, primary(pcf).len());
    assert!(primary(pcf).starts_with(b"\x01fcp"));
    // The converted output decodes back to the same glyphs.
    let back = fontpixel_ops::diff(
        DiffRequest {
            a: "fonts/spleen.bdf".into(),
            b: "out/spleen.pcf".into(),
            ..Default::default()
        },
        &Storage::memory(files().with("out/spleen.pcf", primary(pcf))),
    )
    .unwrap();
    // PCF stores charset metadata differently; glyphs and metrics are equal.
    assert!(
        back.differences.iter().all(|d| d.kind == "charset"),
        "{:?}",
        back.differences
    );
}

#[cfg(feature = "yaff")]
#[test]
fn convert_each_and_naming_overrides_apply_per_input() {
    let mut request = convert_request(&["fonts/spleen.bdf", "fonts/block.bdf"], "out");
    request.each = true;
    request.to = Some("yaff".into());
    request.names.family = Some("Renamed".into());
    let done = fontpixel_ops::convert(request, &storage()).unwrap();
    assert_eq!(done.outputs.len(), 2);
    assert_eq!(done.response.outputs.len(), 2);
    let yaff = output(&done.outputs, "out/spleen.yaff");
    let text = String::from_utf8(primary(yaff)).unwrap();
    assert!(text.contains("Renamed"), "{text}");
    output(&done.outputs, "out/block.yaff");
    let mut json = Vec::new();
    done.response.write_json(&mut json, true).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(value["outputs"].as_array().unwrap().len(), 2);
}

#[test]
fn request_resource_limits_apply_in_memory_mode() {
    let mut request = convert_request(&["fonts/spleen.bdf"], "out/spleen.pcf");
    request.raster.max_total_bytes = Some(4096);
    let result = fontpixel_ops::convert(request, &storage());
    assert!(
        matches!(result, Err(Error::ResourceLimit { .. })),
        "{result:?}"
    );
    let mut request = convert_request(&["fonts/spleen.bdf"], "out/spleen.pcf");
    request.raster.max_input_bytes = Some(SPLEEN.len() - 1);
    let result = fontpixel_ops::convert(request, &storage());
    assert!(
        matches!(result, Err(Error::ResourceLimit { .. })),
        "{result:?}"
    );
}

#[cfg(feature = "psf")]
#[test]
fn strict_loss_is_an_error_with_its_report() {
    let mut request = convert_request(&["fonts/spleen.bdf"], "out/spleen.psf");
    request.to = Some("psf".into());
    request.strict = true;
    let result = fontpixel_ops::convert(request, &storage());
    assert!(
        matches!(&result, Err(Error::StrictLoss(report)) if report.has_loss()),
        "{result:?}"
    );
}

#[cfg(all(feature = "image", feature = "png"))]
#[test]
fn sidecar_and_directory_outputs_round_trip_through_the_resolver() {
    // Atlas plus mapping sidecar.
    let mut request = convert_request(&["fonts/spleen.bdf"], "out/atlas.png");
    request.to = Some("image".into());
    request.image.image_mapping_format = Some("toml".into());
    let done = fontpixel_ops::convert(request, &storage()).unwrap();
    let atlas = output(&done.outputs, "out/atlas.png");
    assert_eq!(atlas.bundle.assets().len(), 2);
    let mut set = files();
    for asset in atlas.bundle.assets() {
        set.insert(
            Path::new("out").join(asset.path.as_str()),
            asset.bytes.clone(),
        );
    }
    let mut back = convert_request(&["out/atlas.png"], "back.bdf");
    back.from = Some("image".into());
    let back = fontpixel_ops::convert(back, &Storage::memory(set)).unwrap();
    let bdf = output(&back.outputs, "back.bdf");
    let same = fontpixel_ops::diff(
        DiffRequest {
            a: "fonts/spleen.bdf".into(),
            b: "back.bdf".into(),
            ..Default::default()
        },
        &Storage::memory(files().with("back.bdf", primary(bdf))),
    )
    .unwrap();
    assert!(same.equal(), "{:?}", same.differences);
    // A directory bundle read back as a directory input.
    let mut request = convert_request(&["fonts/spleen.bdf"], "out/glyphs");
    request.to = Some("image".into());
    request.image.image_layout = Some("imageset".into());
    request.image.image_domain = Some("unicode".into());
    request.image.image_prefix = Some("u".into());
    let done = fontpixel_ops::convert(request, &storage()).unwrap();
    let directory = output(&done.outputs, "out/glyphs");
    assert!(directory.bundle.is_directory());
    let mut set = files();
    for asset in directory.bundle.assets() {
        set.insert(
            Path::new("out").join(asset.path.as_str()),
            asset.bytes.clone(),
        );
    }
    let info = fontpixel_ops::info(
        InfoRequest {
            input: "out/glyphs".into(),
            from: Some("image".into()),
            raster: fontpixel_ops::options::RasterOptions {
                image: fontpixel_ops::image_formats::ReadOptions {
                    image_read_domain: Some("unicode".into()),
                    image_read_prefix: Some("u".into()),
                    ..Default::default()
                },
                ..Default::default()
            },
            json: false,
        },
        &Storage::memory(set),
    )
    .unwrap();
    assert_eq!(
        info.fonts.fonts[0].len(),
        fontpixel::read(SPLEEN, &Default::default())
            .unwrap()
            .fonts
            .fonts[0]
            .len()
    );
}

#[test]
fn charset_validate_and_diff_in_memory() {
    let done = fontpixel_ops::charset(
        CharsetRequest {
            input: "fonts/spleen.bdf".into(),
            output: "out/latin.bdf".into(),
            to: "cp1252".into(),
            ..Default::default()
        },
        &storage(),
    )
    .unwrap();
    assert_eq!(done.response.charset, "cp1252");
    assert_eq!(done.response.font_count, 1);
    output(&done.outputs, "out/latin.bdf");
    let unknown = fontpixel_ops::charset(
        CharsetRequest {
            input: "fonts/spleen.bdf".into(),
            output: "out/latin.bdf".into(),
            to: "no-such-charset".into(),
            ..Default::default()
        },
        &storage(),
    );
    assert!(matches!(unknown, Err(Error::InvalidInput(_))));
    let valid = fontpixel_ops::validate(
        ValidateRequest {
            input: "fonts/spleen.bdf".into(),
            ..Default::default()
        },
        &storage(),
    )
    .unwrap();
    assert_eq!(valid.kind, ValidationKind::Native);
    assert_eq!(valid.faces.len(), 1);
    let differs = fontpixel_ops::diff(
        DiffRequest {
            a: "fonts/spleen.bdf".into(),
            b: "fonts/block.bdf".into(),
            max_differences: 2,
            ..Default::default()
        },
        &storage(),
    )
    .unwrap();
    assert!(!differs.equal());
    assert!(differs.differences.len() <= 2);
    assert!(differs.status().is_err());
    let mut json = Vec::new();
    differs.write_json(&mut json, false).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(value["equal"], false);
}

#[cfg(feature = "bdat")]
#[test]
fn sfnt_validation_uses_the_structural_validator() {
    let set = files().with("embedded.ttf", EMBEDDED_TTF);
    let valid = fontpixel_ops::validate(
        ValidateRequest {
            input: "embedded.ttf".into(),
            ..Default::default()
        },
        &Storage::memory(set),
    )
    .unwrap();
    assert_eq!(valid.kind, ValidationKind::Sfnt);
}

#[cfg(feature = "opentype")]
#[test]
fn merge_writes_every_output_in_one_group() {
    let mut request = MergeRequest {
        parts: vec![
            MergeInput {
                input: "fonts/spleen.bdf".into(),
                take: "western".parse().unwrap(),
                dy: 0,
                from: None,
                font_index: None,
            },
            MergeInput {
                input: "fonts/block.bdf".into(),
                take: "all".parse().unwrap(),
                dy: -1,
                from: None,
                font_index: None,
            },
        ],
        outputs: vec![
            MergeOutput {
                destination: "out/mix.bdf".into(),
                format: None,
            },
            MergeOutput {
                destination: "out/mix.ttf".into(),
                format: None,
            },
        ],
        ..Default::default()
    };
    request.settings.family = "Mix".into();
    let done = fontpixel_ops::merge(request, &storage()).unwrap();
    assert_eq!(done.outputs.len(), 2);
    assert_eq!(done.response.formats, ["bdf", "ttf"]);
    assert!(done.response.glyphs > 0);
    let mut json = Vec::new();
    done.response
        .write(ReportStyle::Json, &mut json, true)
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(value["family"], "Mix");
    assert_eq!(value["outputs"].as_array().unwrap().len(), 2);
}

#[cfg(feature = "opentype")]
#[test]
fn variants_and_subset_in_memory() {
    use fontpixel_ops::{SfntOutputOptions, SubsetRequest, VariantsRequest};
    let done = fontpixel_ops::variants(
        VariantsRequest {
            base: "fonts/spleen.bdf".into(),
            languages: vec!["ja=fonts/spleen-ja.bdf".into()],
            output: SfntOutputOptions {
                output: "out/regional.ttf".into(),
                ..Default::default()
            },
            ..Default::default()
        },
        &Storage::memory(files().with("fonts/spleen-ja.bdf", SPLEEN)),
    )
    .unwrap();
    let ttf = output(&done.outputs, "out/regional.ttf");
    assert!(
        done.response
            .report
            .entries
            .iter()
            .all(|e| !e.code.is_empty())
    );
    assert_eq!(done.response.bytes, primary(ttf).len());
    let set = files().with("regional.ttf", primary(ttf));
    let done = fontpixel_ops::subset(
        SubsetRequest {
            input: "regional.ttf".into(),
            text: Some("AB".into()),
            output: SfntOutputOptions {
                output: "out/ab.woff2".into(),
                ..Default::default()
            },
            ..Default::default()
        },
        &Storage::memory(set),
    )
    .unwrap();
    let woff2 = output(&done.outputs, "out/ab.woff2");
    assert!(primary(woff2).starts_with(b"wOF2"));
}

#[test]
fn preview_and_render_in_memory() {
    let mut request = PreviewRequest::default();
    request.layout.input = "fonts/spleen.bdf".into();
    request.layout.text = Some("Hi".into());
    let preview = fontpixel_ops::preview(request, &storage()).unwrap();
    assert!(preview.text.contains('#'));
    let mut request = RenderRequest::default();
    request.layout.input = "fonts/spleen.bdf".into();
    request.layout.text = Some("Hi".into());
    request.output = "out/hi.svg".into();
    let done = fontpixel_ops::render(request, &storage()).unwrap();
    let svg = output(&done.outputs, "out/hi.svg");
    assert!(primary(svg).starts_with(b"<svg"));
    assert_eq!(done.response.format, "svg");
}

#[test]
fn memory_outputs_reject_duplicate_destinations() {
    let mut request = MergeRequest {
        parts: vec![MergeInput {
            input: "fonts/spleen.bdf".into(),
            take: "all".parse().unwrap(),
            dy: 0,
            from: None,
            font_index: None,
        }],
        outputs: vec![
            MergeOutput {
                destination: "out/same.bdf".into(),
                format: None,
            },
            MergeOutput {
                destination: "out/./same.bdf".into(),
                format: None,
            },
        ],
        ..Default::default()
    };
    request.settings.family = "Same".into();
    let result = fontpixel_ops::merge(request, &storage());
    assert!(matches!(result, Err(Error::InvalidInput(_))), "{result:?}");
}

#[test]
fn typed_batch_runs_in_memory_and_returns_committed_outputs() {
    let request = BatchRequest {
        jobs: Some(1),
        tasks: vec![
            BatchTask::new(
                "pcf",
                TaskRequest::Convert(Box::new(convert_request(
                    &["fonts/spleen.bdf"],
                    "out/spleen.pcf",
                ))),
            ),
            BatchTask::new(
                "validate",
                TaskRequest::Validate(Box::new(ValidateRequest {
                    input: "fonts/block.bdf".into(),
                    ..Default::default()
                })),
            ),
            BatchTask::new(
                "aliases-input",
                TaskRequest::Convert(Box::new(convert_request(
                    &["fonts/block.bdf"],
                    "fonts/spleen.bdf",
                ))),
            ),
        ],
        ..Default::default()
    };
    let mut document = Vec::new();
    let response = fontpixel_ops::batch::run(request, &storage(), &mut document).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&document).unwrap();
    assert_eq!(value["tasks"].as_array().unwrap().len(), 3);
    // Writing over a declared input fails the whole output transaction.
    assert_eq!(response.exit_code, 1);
    assert!(response.outputs.is_empty());
    assert!(response.tasks[1].success);
    let request = BatchRequest {
        jobs: Some(1),
        tasks: vec![BatchTask::new(
            "pcf",
            TaskRequest::Convert(Box::new(convert_request(
                &["fonts/spleen.bdf"],
                "out/spleen.pcf",
            ))),
        )],
        ..Default::default()
    };
    let response = fontpixel_ops::batch::run(request, &storage(), &mut Vec::new()).unwrap();
    assert_eq!(response.exit_code, 0);
    assert!(response.tasks[0].committed);
    output(&response.outputs, "out/spleen.pcf");
}

#[cfg(feature = "toml")]
#[test]
fn toml_batch_runs_in_memory_with_command_line_option_errors() {
    let text = r#"
version = 1
[[tasks]]
id = "ok"
kind = "convert"
inputs = ["fonts/spleen.bdf"]
output = "out/spleen.psf"
to = "psf"
[tasks.options]
psf_version = "1"
[[tasks]]
id = "bad"
kind = "convert"
inputs = ["fonts/spleen.bdf"]
output = "out/bad.psf"
to = "psf"
[tasks.options]
psf_version = "3"
"#;
    let mut document = Vec::new();
    let response = fontpixel_ops::batch::toml::run_text(
        text,
        Path::new(""),
        Some(1),
        &storage(),
        &mut document,
    )
    .unwrap();
    assert_eq!(response.exit_code, 2);
    assert!(
        response.tasks[0].success && response.tasks[0].committed,
        "{response:?} {}",
        String::from_utf8_lossy(&document)
    );
    assert_eq!(response.tasks[1].exit_code, 2);
    assert!(
        response.tasks[1]
            .error
            .as_deref()
            .unwrap()
            .contains("invalid value '3'")
    );
    output(&response.outputs, "out/spleen.psf");
}

#[cfg(feature = "fs")]
#[test]
fn memory_and_filesystem_outputs_are_byte_identical() {
    let scratch = Scratch::new("identical");
    let fs_request = convert_request(
        &[scratch.0.join("fonts/spleen.bdf").to_str().unwrap()],
        scratch.0.join("spleen.pcf").to_str().unwrap(),
    );
    let fs = fontpixel_ops::convert(fs_request, &Storage::Filesystem).unwrap();
    assert!(fs.outputs.is_empty());
    let memory = fontpixel_ops::convert(
        convert_request(&["fonts/spleen.bdf"], "spleen.pcf"),
        &storage(),
    )
    .unwrap();
    assert_eq!(
        std::fs::read(scratch.0.join("spleen.pcf")).unwrap(),
        primary(output(&memory.outputs, "spleen.pcf"))
    );
    let mut fs_text = Vec::new();
    fs.response.write_text(&mut fs_text).unwrap();
    assert!(String::from_utf8(fs_text).unwrap().contains("Wrote "));
}

#[cfg(all(feature = "fs", feature = "opentype"))]
#[test]
fn filesystem_merge_transaction_rolls_back_every_output() {
    let scratch = Scratch::new("transaction");
    let first = scratch.0.join("first.bdf");
    std::fs::write(&first, b"previous").unwrap();
    // The second destination's parent is a regular file, so the group fails
    // before any destination is replaced.
    std::fs::write(scratch.0.join("blocker"), b"file").unwrap();
    let mut request = MergeRequest {
        parts: vec![MergeInput {
            input: scratch.0.join("fonts/spleen.bdf"),
            take: "all".parse().unwrap(),
            dy: 0,
            from: None,
            font_index: None,
        }],
        outputs: vec![
            MergeOutput {
                destination: first.clone(),
                format: None,
            },
            MergeOutput {
                destination: scratch.0.join("blocker/second.ttf"),
                format: None,
            },
        ],
        ..Default::default()
    };
    request.settings.family = "Rollback".into();
    let result = fontpixel_ops::merge(request, &Storage::Filesystem);
    assert!(result.is_err());
    assert_eq!(std::fs::read(&first).unwrap(), b"previous");
    // Outputs may not overwrite a source either.
    let result = fontpixel_ops::convert(
        convert_request(
            &[scratch.0.join("fonts/block.bdf").to_str().unwrap()],
            scratch.0.join("fonts/spleen.bdf").to_str().unwrap(),
        ),
        &Storage::Filesystem,
    );
    assert!(
        result.is_ok(),
        "a single distinct source may be replaced: {result:?}"
    );
    let result = fontpixel_ops::convert(
        convert_request(
            &[
                scratch.0.join("fonts/block.bdf").to_str().unwrap(),
                scratch.0.join("first.bdf").to_str().unwrap(),
            ],
            scratch.0.join("fonts/block.bdf").to_str().unwrap(),
        ),
        &Storage::Filesystem,
    );
    assert!(result.is_err());
    assert_eq!(
        std::fs::read(scratch.0.join("fonts/block.bdf")).unwrap(),
        BLOCK
    );
}
