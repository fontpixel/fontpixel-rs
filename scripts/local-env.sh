#!/usr/bin/env bash
# Source this file from each restored worktree before local builds or validation.
FONTPIXEL_REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
FONTPIXEL_WORK_ROOT="${FONTPIXEL_WORK_ROOT:-$(dirname -- "$FONTPIXEL_REPO_ROOT")/local-work}"
export FONTPIXEL_WORK_ROOT
export PYTHONDONTWRITEBYTECODE=1
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-6}"
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_TARGET_DIR="${FONTPIXEL_TARGET_DIR:-$FONTPIXEL_WORK_ROOT/target-$(basename -- "$FONTPIXEL_REPO_ROOT")}"
if [[ -d "$FONTPIXEL_WORK_ROOT/oracles/bin" ]]; then
    export PATH="$FONTPIXEL_WORK_ROOT/oracles/bin:$PATH"
fi
unset FONTPIXEL_REPO_ROOT
