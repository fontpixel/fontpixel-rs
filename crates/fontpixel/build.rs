//! Select a tighter proved allocation bound only for an audited std implementation.
mod build_support;
fn main() {
    println!("cargo:rustc-check-cfg=cfg(fontpixel_verified_btree_1_99)");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build_support.rs");
    for name in [
        "RUSTC",
        "TARGET",
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_UNSTABLE_BUILD_STD",
        "RUSTC_BOOTSTRAP",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    let custom_std = ["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"]
        .iter()
        .any(|key| std::env::var(key).is_ok_and(|v| v.contains("--sysroot") || v.contains("-Z")))
        || std::env::var_os("CARGO_UNSTABLE_BUILD_STD").is_some()
        || std::env::var_os("RUSTC_BOOTSTRAP").is_some();
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let Ok(output) = std::process::Command::new(rustc).arg("-vV").output() else {
        return;
    };
    if build_support::verified_btree_layout(
        &output.stdout,
        output.status.success(),
        &std::env::var("TARGET").unwrap_or_default(),
        custom_std,
    ) {
        println!("cargo:rustc-cfg=fontpixel_verified_btree_1_99");
    }
}
