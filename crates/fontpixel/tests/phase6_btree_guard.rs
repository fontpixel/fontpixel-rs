//! Unknown compilers and targets retain the pre-existing accounting policy.
#[path = "../build_support.rs"]
mod build_support;
const OFFICIAL: &str = "rustc 1.99.0 (b940084d7 2026-09-28)\nbinary: rustc\ncommit-hash: b940084d7eb6a299eb4bfeb8e34901bc051e7ac4\ncommit-date: 2026-09-28\nhost: x86_64-unknown-linux-gnu\nrelease: 1.99.0\nLLVM version: 23.1.1\n";
#[test]
fn exact_compiler_and_target_enable_verified_policy() {
    assert!(build_support::verified_btree_layout(
        OFFICIAL.as_bytes(),
        true,
        "x86_64-unknown-linux-gnu",
        false
    ));
}
#[test]
fn all_unverified_metadata_falls_back() {
    let target = "x86_64-unknown-linux-gnu";
    for bad in [
        OFFICIAL.replace("release: 1.99.0", "release: 1.100.0"),
        OFFICIAL.replace("release: 1.99.0", "release: 1.99.0-nightly"),
        OFFICIAL.replace(
            "b940084d7eb6a299eb4bfeb8e34901bc051e7ac4",
            "0000000000000000000000000000000000000000",
        ),
        OFFICIAL.replace(
            "host: x86_64-unknown-linux-gnu",
            "host: aarch64-apple-darwin",
        ),
        OFFICIAL.replace("release: 1.99.0\n", ""),
        format!("{OFFICIAL}release: 1.99.0\n"),
        OFFICIAL.replace("binary: rustc", "binary rustc"),
        String::new(),
    ] {
        assert!(!build_support::verified_btree_layout(
            bad.as_bytes(),
            true,
            target,
            false
        ));
    }
    assert!(!build_support::verified_btree_layout(
        OFFICIAL.as_bytes(),
        false,
        target,
        false
    ));
    assert!(!build_support::verified_btree_layout(
        OFFICIAL.as_bytes(),
        true,
        target,
        true
    ));
    assert!(!build_support::verified_btree_layout(
        OFFICIAL.as_bytes(),
        true,
        "wasm32-unknown-unknown",
        false
    ));
    assert!(!build_support::verified_btree_layout(
        &[0xff],
        true,
        target,
        false
    ));
}
