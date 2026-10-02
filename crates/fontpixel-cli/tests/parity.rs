//! Offline golden operation scripts; Python is never used by the Rust runner.
#![cfg(feature = "parity")]
use serde_json::Value;
use std::{fs, process::Command};
#[test]
fn synthetic_reference_scripts_match_every_result_and_error_state() {
    let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/data/compat");
    let out = std::env::temp_dir().join(format!(
        "fontpixel-parity-golden-{}.jsonl",
        std::process::id()
    ));
    let status = Command::new(env!("CARGO_BIN_EXE_fontpixel-parity"))
        .arg(data.join("cases.jsonl"))
        .arg(&out)
        .status()
        .unwrap();
    assert!(status.success());
    let actual = fs::read_to_string(&out).unwrap();
    fs::remove_file(out).unwrap();
    let expected = fs::read_to_string(data.join("expected.jsonl")).unwrap();
    let a = actual
        .lines()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .collect::<Vec<_>>();
    let e = expected
        .lines()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(a.len(), e.len());
    for (a, e) in a.iter().zip(&e) {
        assert_eq!(a["id"], e["id"]);
        let ar = a["res"].as_array().unwrap();
        let er = e["res"].as_array().unwrap();
        assert_eq!(ar.len(), er.len(), "{}", e["id"]);
        for (a, e) in ar.iter().zip(er) {
            assert_eq!(a[0], e[0]);
            assert_eq!(a[1], e[1]);
            if !e[3].is_null() && !a[3].is_null() {
                assert_eq!(a.get(4), e.get(4), "{} op {} error state", a[0], a[1]);
            } else {
                assert_eq!(a, e, "{} op {}", a[0], a[1]);
            }
        }
    }
}
