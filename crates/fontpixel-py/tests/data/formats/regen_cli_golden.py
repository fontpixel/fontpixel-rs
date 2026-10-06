#!/usr/bin/env python3
"""Regenerate cli_golden.json from the frozen fontpixel CLI.

Usage: FONTPIXEL_CLI=/path/to/fontpixel regen_cli_golden.py

For every catalogue format (from `fontpixel formats --json`) and the case in
`tests/formats_support.py` it records, as SHA-256 hashes:

- `write`: every asset of `fontpixel convert SOURCE -o out/<primary> --to ID`
  with the case's CLI flags (or the CLI's error line);
- `readback`: the YAFF written by `fontpixel convert out/<primary> --from ID`
  with the case's read flags;
- `source`: for byte-authored native sources, the YAFF of the source itself;
- `plain`: the assets of a flag-free conversion of the Unicode 3x5 font, or the
  error line when the CLI rejects it.

`fixtures` holds the YAFF of real fixture files read by the CLI. Only the
frozen CLI binary is used here; the Python bindings are not imported.
"""
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(os.path.dirname(HERE)))
from formats_support import CASES, FIXTURES, NATIVE, REPO, SOURCES, Case  # noqa: E402

CLI = os.environ.get("FONTPIXEL_CLI", "fontpixel")

def sha(data):
    return hashlib.sha256(data).hexdigest()


def tree(root):
    out = {}
    for dp, _, names in os.walk(root):
        for n in names:
            p = os.path.join(dp, n)
            with open(p, "rb") as f:
                out[os.path.relpath(p, root).replace(os.sep, "/")] = sha(f.read())
    return dict(sorted(out.items()))


def run(*args):
    r = subprocess.run([CLI, *args], capture_output=True)
    err = r.stderr.decode(errors="replace").strip().splitlines()
    return r.returncode == 0, (err[-1] if err else "")


def convert(work, src, to, primary, flags, src_flags=()):
    out = os.path.join(work, "out")
    shutil.rmtree(out, ignore_errors=True)
    os.makedirs(out)
    ok, err = run("convert", src, *src_flags, "-o", os.path.join(out, primary), "--to", to, *flags)
    return ({"assets": tree(out)} if ok else {"error": err}), out


def to_yaff(work, src, flags):
    dst = os.path.join(work, "back.yaff")
    if os.path.exists(dst):
        os.remove(dst)
    ok, err = run("convert", src, *flags, "-o", dst)
    if not ok:
        return {"error": err}
    with open(dst, "rb") as f:
        return {"yaff": sha(f.read())}


def main():
    ok, _ = run("--version")
    if not ok:
        sys.exit(f"cannot run the CLI {CLI!r}; set FONTPIXEL_CLI")
    listing = json.loads(subprocess.run([CLI, "formats", "--json"], capture_output=True, check=True).stdout)
    golden = {"cli_version": subprocess.run([CLI, "--version"], capture_output=True).stdout.decode().strip(),
              "listing": listing["formats"], "formats": {}, "fixtures": {}}
    with tempfile.TemporaryDirectory() as work:
        for status in listing["formats"]:
            fid = status["format"]["id"]
            exts = status["format"]["extensions"]
            primary = f"font.{exts[0]}" if exts else "font"
            c = CASES.get(fid, Case("uni3"))
            entry = {}
            src_flags = []
            if c.source == "native":
                src = os.path.join(work, "src", primary)
                os.makedirs(os.path.dirname(src), exist_ok=True)
                with open(src, "wb") as f:
                    f.write(NATIVE[fid]())
                src_flags = ["--from", fid, *c.cli_source_read]
                entry["source"] = to_yaff(work, src, src_flags)
            else:
                src = SOURCES[c.source]
            entry["write"], out = convert(work, src, fid, primary, c.cli, src_flags)
            if "assets" in entry["write"]:
                entry["readback"] = to_yaff(work, os.path.join(out, primary), ["--from", fid, *c.cli_read])
            entry["plain"], _ = convert(work, SOURCES["uni3"], fid, primary, [])
            golden["formats"][fid] = entry
            print(fid, json.dumps(entry)[:160], file=sys.stderr)
        for key, (rel, _fmt, _opts, flags) in FIXTURES.items():
            golden["fixtures"][key] = {"path": rel, "flags": flags, **to_yaff(work, os.path.join(REPO, rel), flags)}
    with open(os.path.join(HERE, "cli_golden.json"), "w") as f:
        json.dump(golden, f, indent=1, sort_keys=True)
        f.write("\n")


if __name__ == "__main__":
    main()
