# Sourced by scripts/check_wasm.sh and scripts/test_npm_package.sh: which
# node:test suites (crates/fontpixel-wasm/tests/*.test.mjs) apply to a wasm
# build profile.
#
# The bdfparser build backs bdfparser 4.x: it carries the compat API, BDF/PCF
# and charsets but no ops codecs, so the general format and ops suites (whose
# expectations are per profile: full, minimal, merge, web, convert) do not
# apply to it.
WASM_GENERAL_ONLY_SUITES='ops.test.mjs ops-progress.test.mjs formats_extra.test.mjs formats_options.test.mjs foundation.test.mjs'

# wasm_suites REPO PROFILE: print the suite paths for PROFILE, one per line.
wasm_suites() {
  local t
  for t in "$1"/crates/fontpixel-wasm/tests/*.test.mjs; do
    if [ "$2" = bdfparser ] && [[ " $WASM_GENERAL_ONLY_SUITES " == *" $(basename "$t") "* ]]; then continue; fi
    printf '%s\n' "$t"
  done
}
