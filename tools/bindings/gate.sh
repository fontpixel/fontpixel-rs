#!/usr/bin/env bash
# Binding coverage gate: rustdoc JSON -> inventory -> mapping check.
# Usage: tools/bindings/gate.sh [python|python,js] [report.md]
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.."
require="${1:-}"
report="${2:-}"
target="${FONTPIXEL_RUSTDOC_TARGET:-${CARGO_TARGET_DIR:-target}/rustdoc-json}"
docs=()
for crate in fontpixel fontpixel-ops; do
  if cargo metadata --no-deps --format-version 1 | grep -q "\"name\":\"$crate\""; then
    CARGO_TARGET_DIR="$target" RUSTDOCFLAGS="-Z unstable-options --output-format json" \
      cargo +nightly rustdoc -q --locked -p "$crate" --all-features --lib
    docs+=("$target/doc/${crate//-/_}.json")
  fi
done
"${PYTHON:-python3}" tools/bindings/inventory.py "$target/inventory.json" "${docs[@]}"
# Generated format tables must match the inventory (a new serde type or
# format function must not be absorbed silently by a module catch-all).
"${PYTHON:-python3}" tools/bindings/gen_formats_types.py "$target/inventory.json" --check
"${PYTHON:-python3}" tools/bindings/gen_formats_coverage.py "$target/inventory.json" --check
# Phase 8: the generated TypeScript plain-data types must match too.
case ",$require," in *,js,*) "${PYTHON:-python3}" tools/bindings/gen_dts.py --rustdoc "$target/doc" --check ;; esac
args=("$target/inventory.json")
[ -n "$require" ] && args+=(--require "$require")
[ -n "$report" ] && args+=(--report "$report")
"${PYTHON:-python3}" tools/bindings/check.py "${args[@]}"
