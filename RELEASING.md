# Releasing

One version number covers everything: the Rust crates `fontpixel`,
`fontpixel-ops` and `fontpixel-cli`, the PyPI package `fontpixel` and the npm
package `fontpixel`. `.github/workflows/release.yml` builds, tests and
publishes them; nothing is uploaded from a developer machine.

## One-time setup

Repository: `https://github.com/fontpixel/fontpixel-rs`.

GitHub → Settings → Environments: create four environments. Add required
reviewers to each if every upload should wait for a manual approval (dry runs
then wait too).

| environment | used by | secrets / settings |
|---|---|---|
| `crates-io` | `publish-crates` | secret `CARGO_REGISTRY_TOKEN`: a crates.io API token with the `publish-new` and `publish-update` scopes, restricted to the crates `fontpixel`, `fontpixel-ops`, `fontpixel-cli` once they exist |
| `pypi` | `publish-pypi` (to PyPI) | none: trusted publishing |
| `testpypi` | `publish-pypi` (to TestPyPI) | none: trusted publishing |
| `npm` | `publish-npm` | secret `NPM_TOKEN`: an npm granular access token with read and write access to the package `fontpixel` (for the first publish: to all packages, then narrow it), allowed to bypass 2FA for automation |

Trusted publishers (no tokens; OpenID Connect from the workflow):

- PyPI → Your account → Publishing → "Add a new pending publisher": project
  `fontpixel`, owner `fontpixel`, repository `fontpixel-rs`, workflow
  `release.yml`, environment `pypi`.
- TestPyPI (test.pypi.org, a separate account) → the same with environment
  `testpypi`.

npm uses a token (`NODE_AUTH_TOKEN` from `NPM_TOKEN`) and publishes with
`--provenance` (the job has `id-token: write`). npm trusted publishing is an
alternative once the package exists: on npmjs.com → package `fontpixel` →
Settings → Trusted publisher, add GitHub Actions, organization `fontpixel`,
repository `fontpixel-rs`, workflow `release.yml`, environment `npm`; then
delete `NODE_AUTH_TOKEN` from the job and the `NPM_TOKEN` secret (npm 11.5.1
or newer, which Node 24's npm is).

crates.io: the first publish of each crate name needs a token from the
account that will own it; add co-owners later with `cargo owner --add`.
(crates.io trusted publishing, `rust-lang/crates-io-auth-action`, can replace
the token after the first release.)

The `reference-python` and `reference-js` jobs of check.yml check out
`tomchen/bdfparser` and `tomchen/bdfparser-js` at branch `v3`; they fail until
those branches exist.

## Release steps

1. Update the version in `Cargo.toml` (`[workspace.package]` and the
   `fontpixel` entry of `[workspace.dependencies]`), the `fontpixel-ops`
   requirement in `crates/fontpixel-cli/Cargo.toml` and
   `crates/fontpixel-py/Cargo.toml`, `crates/fontpixel-wasm/Cargo.toml`, and
   `crates/fontpixel-wasm/js/package.json`; run `cargo update -w`; move the
   CHANGELOG entry from "unreleased" to the date. Commit.
2. Local checks: `scripts/check.sh`, `scripts/check_python.sh`,
   `scripts/check_wasm.sh`, `cargo publish --workspace --dry-run`,
   `scripts/build_wheels.sh` and `scripts/build_npm.sh` +
   `scripts/test_npm_package.sh` (usage in each script's header).
3. Push the commit; wait for check.yml to pass.
4. Dry run: Actions → Release → Run workflow on the commit's branch with
   `dry_run` checked. It runs every check and test, `cargo publish
   --dry-run`, `twine check` and `npm publish --dry-run`, and uploads nothing.
5. Optional TestPyPI rehearsal: run it again with `dry_run` unchecked and
   `pypi` = `testpypi` (branch runs may only upload to TestPyPI; crates.io
   and npm are skipped), then `pip install -i https://test.pypi.org/simple/
   fontpixel` in a fresh environment.
6. Tag and push: `git tag -a v0.1.0 -m "fontpixel 0.1.0" && git push origin
   v0.1.0`. The tag must equal `v` + the workspace version. The workflow
   publishes in this order, each step only after every test passed and the
   previous step succeeded: crates.io (`cargo publish --workspace`: fontpixel,
   fontpixel-ops, fontpixel-cli), PyPI (all wheels and the sdist), npm.
7. Check the published packages (`cargo install fontpixel-cli`, `pip install
   fontpixel`, `npm install fontpixel`) and create the GitHub release from the
   tag with the CHANGELOG entry.

A failed publish job can be re-run from the Actions page; crates.io and PyPI
refuse to overwrite a version, so a partially published release continues
with the remaining registries (re-run the failed job), never by re-uploading.

## Afterwards: bdfparser 4.x

The `bdfparser` 4.0.0 packages are thin wrappers over this release and are
released only after `fontpixel` 0.1.0 is live on PyPI and npm:

- PyPI `bdfparser` 4.0.0 depends on `fontpixel>=0.1,<0.2` and re-exports
  `fontpixel.compat.bdfparser` (and its submodules `bitmap`, `cli`,
  `encoding`, `errors`, `font`, `glyph`, `outline`, `pcf`, `ttf`; see
  `crates/fontpixel-py/tests/compat_bdfparser/shim/` for a working example).
- npm `bdfparser` 4.0.0 depends on `fontpixel@^0.1.0` and re-exports
  `fontpixel/compat/bdfparser`, `fontpixel/compat/bdfparser/cjk` and
  `fontpixel/compat/bdfparser/cli` (or the smaller
  `fontpixel/bdfparser/compat/bdfparser...` build, see the npm README); see
  `crates/fontpixel-wasm/tests/compat_bdfparser/shim/`. Publish it under the
  dist-tag `rust` (`npm publish --tag rust`); `latest` stays on 3.x.
- Their changelogs take the list in
  [docs/bdfparser-compat-differences.md](docs/bdfparser-compat-differences.md).
