//! Exact supported-toolchain check for conservative B-tree allocation accounting.

/// Recognize only the audited official compiler and bundled x86-64 Linux std.
/// Unsuccessful, malformed, duplicate or unrecognized metadata selects fallback.
pub fn verified_btree_layout(output: &[u8], success: bool, target: &str, custom_std: bool) -> bool {
    if !success || custom_std || target != "x86_64-unknown-linux-gnu" {
        return false;
    }
    let Ok(text) = std::str::from_utf8(output) else {
        return false;
    };
    if text.lines().next() != Some("rustc 1.99.0 (b940084d7 2026-09-28)") {
        return false;
    }
    let mut fields = std::collections::BTreeMap::new();
    for line in text.lines().skip(1) {
        let Some((key, value)) = line.split_once(": ") else {
            return false;
        };
        if fields.insert(key, value).is_some() {
            return false;
        }
    }
    fields.get("binary") == Some(&"rustc")
        && fields.get("release") == Some(&"1.99.0")
        && fields.get("host") == Some(&"x86_64-unknown-linux-gnu")
        && fields.get("commit-hash") == Some(&"b940084d7eb6a299eb4bfeb8e34901bc051e7ac4")
}
