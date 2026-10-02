#!/usr/bin/env bash
# Run from any directory. Do not skip any required check if a tool is missing.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo clippy --locked -p fontpixel --all-targets --no-default-features -- -D warnings
cargo test --locked --workspace --all-features
cargo test --locked --workspace
cargo test --locked -p fontpixel --no-default-features
cargo test --locked -p fontpixel --no-default-features --features bdf,pcf
cargo deny --locked --all-features check
