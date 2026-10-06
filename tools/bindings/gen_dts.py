#!/usr/bin/env python3
"""Generate the TypeScript declarations of the plain-data types (JS binding).

Writes crates/fontpixel-wasm/ts/types.d.ts: one declaration per public serde
type of `fontpixel` and `fontpixel-ops`, from nightly rustdoc JSON through
tools/bindings/serde_shapes.py (the same shapes gen_pyi.py renders for
Python, so both languages use the same type names):

* unit-only enums -> string-literal unions of the serde names (aliases too);
* structs -> interfaces with the Rust field names (`key?:` when the key may
  be omitted on input; `T | null` for `Option`);
* data-carrying enums -> unions of single-key interfaces (externally tagged)
  or tagged interfaces;
* newtypes -> their inner type;
* `SerdeTypes`: every type by its public Rust path (crate prefix
  `fontpixel::` dropped, `fontpixel_ops::` kept), as `normalize()` and the
  Python `fontpixel.types.SERDE_TYPES` spell it.

Number policy (docs/bindings-design.md, "JS/WASM"): `u64`/`i64`/`u128`/
`i128` are `WideInt` (`number | bigint`, decided per value); every other
integer (including `usize`, 32-bit on wasm32) and float is `number`. Maps are
`Record<string, V>` (integer keys as decimal strings). Byte lists are
`SerdeBytes` (`number[]`). The aliases live in ts/aliases.d.ts.

The plain-data shape of a binding class `X` (`#[wasm_bindgen(js_name = X)]`
on a struct in crates/fontpixel-wasm/src) is named `XDict`, as in Python.

Usage:
    tools/bindings/gen_dts.py [--rustdoc DIR] [--check]

DIR holds fontpixel.json / fontpixel_ops.json (default
$FONTPIXEL_RUSTDOC_TARGET/doc, else ${CARGO_TARGET_DIR:-target}/rustdoc-json/doc;
build them with tools/bindings/gate.sh). `--check` writes nothing and exits 1
when the committed file differs.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(HERE))

import serde_shapes  # noqa: E402

OUT = ROOT / "crates/fontpixel-wasm/ts/types.d.ts"
SRC = ROOT / "crates/fontpixel-wasm/src"
ALIASES = ROOT / "crates/fontpixel-wasm/ts/aliases.d.ts"
WIDE = {"u64", "i64", "u128", "i128"}
# TS-specific corrections of serde_shapes.MANUAL fields (hand-written there as
# Python `int`, but 64-bit in Rust).
MANUAL_WIDE = {("fontpixel::model::Rational", "numerator")}
# Global names a generated declaration must not shadow.
GLOBALS = {"Array", "ArrayBuffer", "BigInt", "Boolean", "DataView", "Date", "Error", "Function",
           "Map", "Number", "Object", "Promise", "Record", "Response", "Request", "Set", "String",
           "Symbol", "URL", "Uint8Array", "WeakMap", "WeakSet", "WebAssembly"}
SIMPLE = {"int": "number", "float": "number", "bool": "boolean", "str": "string", "None": "null",
          "Any": "unknown", "StrPath": "string", "SerdeBytes": "SerdeBytes", "WideInt": "WideInt",
          "bytes": "SerdeBytes"}


def default_rustdoc():
    target = os.environ.get("FONTPIXEL_RUSTDOC_TARGET") or os.path.join(
        os.environ.get("CARGO_TARGET_DIR", str(ROOT / "target")), "rustdoc-json")
    return Path(target) / "doc"


def class_names():
    """JS class names declared on structs of the binding crate."""
    names = set()
    for file in sorted(SRC.glob("*.rs")):
        text = file.read_text()
        names |= set(re.findall(r"#\[wasm_bindgen\(js_name\s*=\s*\"?(\w+)\"?\)\]\s*pub struct", text))
    return names


def alias_names():
    return set(re.findall(r"^export (?:type|interface) (\w+)", ALIASES.read_text(), re.M))


class TsModel(serde_shapes.SerdeModel):
    """serde_shapes with 64-bit integers kept apart as `WideInt`."""

    def resolve(self, doc_no, ty, generics=()):
        if next(iter(ty)) == "primitive" and ty["primitive"] in WIDE:
            return "WideInt"
        return super().resolve(doc_no, ty, generics)


# ------------------------------------------------------------ expressions


def tokenize(expr):
    return re.findall(r'"(?:[^"\\]|\\.)*"|[A-Za-z_][\w.]*|[\[\],|]', expr)


def ts_type(expr):
    """TypeScript type of a serde_shapes (Python-style) type expression."""
    tokens = tokenize(expr)
    pos = 0

    def peek():
        return tokens[pos] if pos < len(tokens) else None

    def take(expected=None):
        nonlocal pos
        tok = tokens[pos]
        if expected and tok != expected:
            raise SystemExit(f"gen_dts: cannot parse {expr!r}")
        pos += 1
        return tok

    def union():
        parts = [atom()]
        while peek() == "|":
            take("|")
            parts.append(atom())
        out = []
        for p in parts:
            for q in p.split(" | ") if not p.startswith("(") else [p]:
                if q not in out:
                    out.append(q)
        return " | ".join(out)

    def args():
        take("[")
        items = [union()]
        while peek() == ",":
            take(",")
            items.append(union())
        take("]")
        return items

    def wrap(t):
        return f"({t})" if " | " in t else t

    def atom():
        tok = take()
        if tok.startswith('"'):
            return tok
        if peek() == "[":
            items = args()
            if tok in ("list", "List", "Sequence"):
                return wrap(items[0]) + "[]"
            if tok in ("dict", "Dict", "Mapping"):
                return f"Record<string, {items[1]}>"
            if tok == "tuple":
                return "[" + ", ".join(items) + "]"
            if tok == "Literal":
                return " | ".join(items)
            if tok == "Union":
                return " | ".join(items)
            raise SystemExit(f"gen_dts: unknown generic {tok} in {expr!r}")
        return SIMPLE.get(tok, tok)

    result = union()
    if pos != len(tokens):
        raise SystemExit(f"gen_dts: trailing tokens in {expr!r}")
    return result


# ------------------------------------------------------------ rendering


def doc_comment(text, indent=""):
    text = text.strip()
    if not text:
        return []
    text = text.replace("*/", "*∕")
    lines = text.splitlines()
    if len(lines) == 1:
        return [f"{indent}/** {lines[0]} */"]
    return [f"{indent}/**"] + [f"{indent} * {line}".rstrip() for line in lines] + [f"{indent} */"]


def key(name):
    return name if re.fullmatch(r"[A-Za-z_$][\w$]*", name) else json.dumps(name)


def rust_doc(shape):
    path = shape.path
    doc = shape.doc or ""
    return f"`{path}`" + (f": {doc}" if doc else "")


def render_struct(shape, wide_fields=()):
    out = doc_comment(rust_doc(shape))
    fields = shape.fields
    if not fields:
        out.append(f"export interface {shape.name} {{}}")
        return out
    out.append(f"export interface {shape.name} {{")
    for f in fields:
        if f.key.startswith("**"):
            out += doc_comment(f"Keys of the flattened `{f.key[2:]}` group.", "  ")
            out.append("  [key: string]: unknown;")
            continue
        t = "WideInt" if f.key in wide_fields else ts_type(f.type)
        out += doc_comment(f.doc, "  ")
        opt = "" if f.required else "?"
        out.append(f"  {key(f.key)}{opt}: {t};")
    out.append("}")
    return out


def render(model):
    lines = [
        "// Generated by tools/bindings/gen_dts.py from the rustdoc JSON of fontpixel and",
        "// fontpixel-ops; do not edit by hand. Plain-data shapes of every serde type:",
        "// keys are the Rust field names; WideInt = number | bigint (per value);",
        "// SerdeBytes = number[]; Record keys of integer maps are decimal strings.",
        "",
    ]
    shapes = sorted(model.shapes.values(), key=lambda s: s.name)
    decls = {}
    for s in shapes:
        if s.kind == "literal":
            decls[s.name] = doc_comment(rust_doc(s)) + [
                f"export type {s.name} = {' | '.join(json.dumps(v) for v in s.values)};"]
        elif s.kind == "alias":
            decls[s.name] = doc_comment(rust_doc(s)) + [f"export type {s.name} = {ts_type(s.alias)};"]
        elif s.kind == "struct":
            wide = {f for (p, f) in MANUAL_WIDE if p == s.path}
            decls[s.name] = render_struct(s, wide)
        else:
            members = [ts_type(m) for m in s.members]
            decls[s.name] = doc_comment(rust_doc(s)) + [f"export type {s.name} = {' | '.join(members)};"]
            for extra in s.extra:
                decls[extra.name] = render_struct(extra)
    for name in sorted(decls):
        lines += decls[name]
        lines.append("")
    lines += doc_comment(
        "Every serde type by its public Rust path (crate prefix `fontpixel::` dropped, "
        "`fontpixel_ops::` kept), as `normalize(typeName, value)` and the Python "
        "`fontpixel.types.SERDE_TYPES` spell it.")
    lines.append("export interface SerdeTypes {")
    for path, name in serde_index(model):
        lines.append(f"  {json.dumps(path)}: {name};")
    lines.append("}")
    return "\n".join(lines) + "\n"


def serde_index(model):
    rows = dict(model.names)
    for public, (n, k, owner) in model.public.items():
        if owner is None:
            canon = model.path_of.get((n, k))
            if canon in model.names:
                rows.setdefault(public, model.names[canon])
    out = []
    for path, name in sorted(rows.items()):
        short = path[len("fontpixel::"):] if path.startswith("fontpixel::") else path
        out.append((short, name))
    return out


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--rustdoc", type=Path, default=default_rustdoc())
    ap.add_argument("--check", action="store_true")
    a = ap.parse_args(argv)
    reserved = GLOBALS | alias_names()
    model = TsModel(a.rustdoc, reserved, class_names())
    text = render(model)
    if a.check:
        if (OUT.read_text() if OUT.exists() else "") != text:
            print(f"{OUT.relative_to(ROOT)} is out of date; run tools/bindings/gen_dts.py", file=sys.stderr)
            return 1
        print(f"{OUT.relative_to(ROOT)} up to date ({len(model.shapes)} types)")
        return 0
    OUT.write_text(text)
    print(f"wrote {OUT.relative_to(ROOT)} ({len(model.shapes)} types)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
