#!/usr/bin/env python3
"""Generate crates/fontpixel-wasm/src/normalize_types.rs (JS `normalize`).

One `normalize_table!` row per serde type of `fontpixel` and `fontpixel-ops`
that implements both Serialize and Deserialize (and has no lifetime or
default-less type parameter), keyed exactly as `SerdeTypes` in the generated
TypeScript (tools/bindings/gen_dts.py `serde_index`): the Rust path without
the `fontpixel::` prefix, `fontpixel_ops::` kept, re-export paths included.

Each row names the type by a public Rust path (the canonical one when every
module on it is public, else the shortest public re-export) and carries the
`#[cfg(...)]` under which that path exists in the binding crate. rustdoc JSON
is built with every feature and records no `cfg`, so the condition is read
from the sources: the `#[cfg]` attributes of every `mod` declaration on the
canonical and the public path, of the item itself and of the `pub use` that
re-exports it. Each core/ops feature condition is evaluated for every
combination of the binding crate's profile features (`all-formats`,
`compat-bdfparser`, `cjk-charsets`, `toml`, `minimal`, `merge`, `web`, `convert`; feature lists
read from the three Cargo.toml files, unified like Cargo does) and written as
the minimal `any(all(...))` over those features. Types that no combination
provides (for example behind fontpixel-ops `fs`) get no row.

Usage:
    tools/bindings/gen_normalize.py [--rustdoc DIR] [--check]

DIR as for gen_dts.py (default $FONTPIXEL_RUSTDOC_TARGET/doc, else
${CARGO_TARGET_DIR:-target}/rustdoc-json/doc). `--check` writes nothing and
exits 1 when the committed file differs.
"""
from __future__ import annotations

import argparse
import itertools
import re
import sys
import tomllib
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(HERE))

import gen_dts  # noqa: E402

OUT = ROOT / "crates/fontpixel-wasm/src/normalize_types.rs"
CRATES = {"fontpixel": ROOT / "crates/fontpixel", "fontpixel_ops": ROOT / "crates/fontpixel-ops"}
# Binding-crate features a build combines (`full` = the first four).
ATOMS = ["all-formats", "compat-bdfparser", "cjk-charsets", "toml", "minimal", "merge", "web", "convert"]


# ------------------------------------------------------------ cargo features


def cargo_features(path):
    return tomllib.loads(path.read_text()).get("features", {})


WASM_FEATURES = cargo_features(ROOT / "crates/fontpixel-wasm/Cargo.toml")
OPS_FEATURES = cargo_features(CRATES["fontpixel_ops"] / "Cargo.toml")
CORE_FEATURES = cargo_features(CRATES["fontpixel"] / "Cargo.toml")


def enabled(atoms):
    """Unified (fontpixel, fontpixel_ops) feature sets of a binding build."""
    # Dependency specs: fontpixel {features = ["serde"]} (default features
    # off workspace-wide); fontpixel-ops {default-features = false,
    # features = ["serde"]}, which itself asks fontpixel for "serde".
    core, ops, wasm = {"serde"}, {"serde"}, set()
    todo = list(atoms)
    while todo:
        f = todo.pop()
        if f in wasm:
            continue
        wasm.add(f)
        for entry in WASM_FEATURES.get(f, []):
            if entry.startswith("fontpixel/"):
                core.add(entry.split("/", 1)[1])
            elif entry.startswith("fontpixel-ops/"):
                ops.add(entry.split("/", 1)[1])
            elif not entry.startswith("dep:"):
                todo.append(entry)

    def close(seed, table, other=None):
        out, todo = set(), list(seed)
        while todo:
            f = todo.pop()
            if f in out:
                continue
            out.add(f)
            for entry in table.get(f, []):
                if entry.startswith("dep:") or "?/" in entry:
                    continue
                if "/" in entry:
                    crate, feat = entry.split("/", 1)
                    if other is not None and crate == "fontpixel":
                        other.add(feat)
                    continue
                todo.append(entry)
        return out

    ops = close(ops, OPS_FEATURES, core)
    core = close(core, CORE_FEATURES)
    return {"fontpixel": core, "fontpixel_ops": ops}


COMBOS = [frozenset(c) for n in range(len(ATOMS) + 1) for c in itertools.combinations(ATOMS, n)]
FEATURES = {c: enabled(c) for c in COMBOS}


# ------------------------------------------------------------ cfg expressions


def parse_cfg(text):
    tokens = re.findall(r'"[^"]*"|[A-Za-z_][A-Za-z0-9_]*|[(),=]', text)
    pos = 0

    def take():
        nonlocal pos
        pos += 1
        return tokens[pos - 1]

    def expr():
        name = take()
        if pos < len(tokens) and tokens[pos] == "(":
            take()
            args = []
            while tokens[pos] != ")":
                args.append(expr())
                if tokens[pos] == ",":
                    take()
            take()
            if name not in ("any", "all", "not"):
                raise SystemExit(f"gen_normalize: unknown cfg predicate {name} in {text!r}")
            return (name, args)
        if pos < len(tokens) and tokens[pos] == "=":
            take()
            return ("kv", name, take().strip('"'))
        return ("flag", name)

    out = expr()
    if pos != len(tokens):
        raise SystemExit(f"gen_normalize: cannot parse cfg {text!r}")
    return out


def evaluate(node, features):
    kind = node[0]
    if kind == "any":
        return any(evaluate(n, features) for n in node[1])
    if kind == "all":
        return all(evaluate(n, features) for n in node[1])
    if kind == "not":
        return not evaluate(node[1][0], features)
    if kind == "kv" and node[1] == "feature":
        return node[2] in features
    if kind == "flag" and node[1] in ("test", "doc"):
        return False
    raise SystemExit(f"gen_normalize: cannot evaluate cfg {node} for every target")


# ------------------------------------------------------------ source scanning


def matching_open(text, close):
    """Index of the `[` matching the `]` at `close` (strings respected)."""
    depth, i = 0, close
    while i >= 0:
        ch = text[i]
        if ch == '"':
            i -= 1
            while i >= 0 and not (text[i] == '"' and text[i - 1] != "\\"):
                i -= 1
        elif ch == "]":
            depth += 1
        elif ch == "[":
            depth -= 1
            if depth == 0:
                return i
        i -= 1
    raise SystemExit("gen_normalize: unbalanced attribute")


def attrs_before(text, start):
    """`#[cfg(...)]` texts of the attributes directly before offset `start`."""
    cfgs = []
    end = start
    while True:
        head = text[:end].rstrip()
        if head.endswith("]"):
            open_ = matching_open(head, len(head) - 1)
            if open_ > 0 and head[open_ - 1] == "#":
                attr = head[open_ + 1:-1].strip()
                if attr.startswith("cfg(") and attr.endswith(")"):
                    cfgs.append(attr[4:-1])
                end = open_ - 1
                continue
        line_start = head.rfind("\n") + 1
        line = head[line_start:].strip()
        if line.startswith("//"):
            end = line_start
            continue
        return cfgs


class Sources:
    def __init__(self):
        self.text = {}

    def read(self, path):
        if path not in self.text:
            self.text[path] = path.read_text()
        return self.text[path]

    def module_file(self, crate, segments):
        """(file, cfgs) of the module `crate::segments` (inline or out-of-line)."""
        file = CRATES[crate] / "src/lib.rs"
        cfgs = []
        for seg in segments:
            text = self.read(file)
            m = re.search(rf"^[ \t]*(?:pub(?:\([^)]*\))?\s+)?mod\s+{seg}\s*([;{{])", text, re.M)
            if not m:
                raise SystemExit(f"gen_normalize: no `mod {seg}` in {file}")
            cfgs += attrs_before(text, m.start())
            if m.group(1) == "{":
                raise SystemExit(f"gen_normalize: inline module {seg} in {file} is not supported")
            base = file.parent if file.name in ("lib.rs", "mod.rs") else file.with_suffix("")
            file = base / f"{seg}.rs"
            if not file.exists():
                file = base / seg / "mod.rs"
        return file, cfgs

    def item_cfgs(self, span):
        path = ROOT / span["filename"]
        text = self.read(path)
        lines = text.split("\n")
        offset = sum(len(line) + 1 for line in lines[:span["begin"][0] - 1]) + span["begin"][1] - 1
        return attrs_before(text, offset)

    def use_cfgs(self, file, name):
        """cfgs of the `pub use` statement in `file` that exports `name`."""
        text = self.read(file)
        for m in re.finditer(r"^[ \t]*pub\s+use\s+[^;]*;", text, re.M | re.S):
            if re.search(rf"\b{re.escape(name)}\b", m.group(0)):
                return attrs_before(text, m.start())
        raise SystemExit(f"gen_normalize: no `pub use` of {name} in {file}")


# ------------------------------------------------------------ table


def item_traits(model, key):
    it = model.items[key]
    inner = it["inner"]
    kind = "struct" if "struct" in inner else "enum"
    traits = set()
    for impl_id in inner[kind].get("impls", []):
        impl = model.items.get((key[0], str(impl_id)))
        if impl and impl["inner"]["impl"].get("trait"):
            traits.add(impl["inner"]["impl"]["trait"]["path"].split("::")[-1])
    nameable = True
    for g in inner[kind].get("generics", {}).get("params", []):
        kind_g = g["kind"]
        if "lifetime" in kind_g or ("type" in kind_g and kind_g["type"].get("default") is None):
            nameable = False
    return traits, nameable, it.get("span")


def condition(cfgs_by_crate):
    """Minimal DNF over ATOMS for a list of (crate, cfg text) conditions."""
    parsed = [(crate, parse_cfg(c)) for crate, c in cfgs_by_crate]
    truth = {c: all(evaluate(node, FEATURES[c][crate]) for crate, node in parsed) for c in COMBOS}
    minimal = [c for c in COMBOS if truth[c] and not any(truth[d] for d in COMBOS if d < c)]
    for c in COMBOS:
        if truth[c] != any(m <= c for m in minimal):
            raise SystemExit(f"gen_normalize: non-monotone cfg {cfgs_by_crate}")
    if not minimal:
        return None
    if minimal == [frozenset()]:
        return ""
    terms = []
    for m in sorted(minimal, key=lambda s: sorted(ATOMS.index(a) for a in s)):
        feats = [f'feature = "{a}"' for a in sorted(m, key=ATOMS.index)]
        terms.append(feats[0] if len(feats) == 1 else f"all({', '.join(feats)})")
    return terms[0] if len(terms) == 1 else f"any({', '.join(terms)})"


def rows(model):
    src = Sources()
    publics = {}
    for path, (n, k, owner) in model.public.items():
        if owner is None:
            publics.setdefault(model.path_of.get((n, k)), []).append(path)
    out = []
    for short, _name in gen_dts.serde_index(model):
        full = short if short.startswith("fontpixel_ops::") else "fontpixel::" + short
        if full in model.serde:
            canon = full
        else:
            hit = model.public.get(full)
            canon = model.path_of.get((hit[0], hit[1])) if hit else None
        key = model.serde.get(canon)
        if key is None:
            continue
        traits, nameable, span = item_traits(model, key)
        if not nameable or not {"Serialize", "Deserialize"} <= traits:
            continue
        candidates = publics.get(canon, [])
        rust = canon if canon in candidates else min(candidates, key=lambda p: (p.count("::"), len(p), p))
        cfgs = []
        canon_crate, *canon_mods = canon.split("::")[:-1]
        _, mod_cfgs = src.module_file(canon_crate, canon_mods)
        cfgs += [(canon_crate, c) for c in mod_cfgs]
        cfgs += [(canon_crate, c) for c in src.item_cfgs(span)]
        if rust != canon:
            rust_crate, *rust_mods = rust.split("::")[:-1]
            file, mod_cfgs = src.module_file(rust_crate, rust_mods)
            cfgs += [(rust_crate, c) for c in mod_cfgs]
            cfgs += [(rust_crate, c) for c in src.use_cfgs(file, canon.split("::")[-1])]
        cond = condition(cfgs)
        if cond is None:
            continue
        out.append((short, rust, cond))
    return out


def render(table):
    lines = [
        "// Generated by tools/bindings/gen_normalize.py from the rustdoc JSON and the",
        "// crate sources; do not edit by hand. One row per serde type (Serialize and",
        "// Deserialize) of fontpixel and fontpixel-ops, keyed as `SerdeTypes`, under the",
        "// binding features that provide it.",
        "normalize_table! {",
    ]
    for short, rust, cond in table:
        if cond:
            lines.append(f"    #[cfg({cond})]")
        lines.append(f'    "{short}" => {rust},')
    lines.append("}")
    return "\n".join(lines) + "\n"


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--rustdoc", type=Path, default=gen_dts.default_rustdoc())
    ap.add_argument("--check", action="store_true")
    a = ap.parse_args(argv)
    model = gen_dts.TsModel(a.rustdoc, gen_dts.GLOBALS | gen_dts.alias_names(), gen_dts.class_names())
    table = rows(model)
    text = render(table)
    if a.check:
        if (OUT.read_text() if OUT.exists() else "") != text:
            print(f"{OUT.relative_to(ROOT)} is out of date; run tools/bindings/gen_normalize.py", file=sys.stderr)
            return 1
        print(f"{OUT.relative_to(ROOT)} up to date ({len(table)} types)")
        return 0
    OUT.write_text(text)
    print(f"wrote {OUT.relative_to(ROOT)} ({len(table)} types)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
