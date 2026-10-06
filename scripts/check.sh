#!/usr/bin/env bash
# Run from any directory. Do not skip any required check if a tool is missing.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
PYTHONDONTWRITEBYTECODE=1 python3 tools/gen_charsets.py --check
PYTHONDONTWRITEBYTECODE=1 python3 tools/gen_bdf_printable.py --check
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo clippy --locked -p fontpixel --all-targets --no-default-features -- -D warnings
cargo clippy --locked -p fontpixel-cli --all-targets --no-default-features --features bdf -- -D warnings
cargo clippy --locked -p fontpixel-ops --all-targets --no-default-features -- -D warnings
cargo clippy --locked -p fontpixel-ops --all-targets --no-default-features --features bdf,pcf,serde -- -D warnings
cargo clippy --locked -p fontpixel-ops --target wasm32-unknown-unknown --no-default-features --features serde,toml,all-formats -- -D warnings
cargo test --locked --workspace --all-features
cargo test --locked --workspace
cargo test --locked -p fontpixel --no-default-features
cargo test --locked -p fontpixel --no-default-features --features bdf,pcf
cargo test --locked -p fontpixel --no-default-features --features cjk-charsets
cargo test --locked -p fontpixel-ops --no-default-features --features bdf,pcf,serde
cargo run --locked -q -p fontpixel --all-features --example api_quickstart > /dev/null
cargo deny --locked --all-features check
