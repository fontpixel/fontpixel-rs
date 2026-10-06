#!/usr/bin/env python3
"""Check that every public Rust item is mapped to the Python and JS bindings.

Rules live in bindings/coverage/*.toml:

    [[rule]]
    pattern = "fontpixel::model::Font::*"     # `*` = one path segment, `**` = any
    status = "bound"          # bound | adapted | unavailable | internal
    python = "fontpixel.Font.{name}"          # entry; {name} = last path segment
    js = "Font.{name}"
    python_test = "crates/fontpixel-py/tests/test_model.py::test_font_api"
    js_test = "crates/fontpixel-wasm/tests/model.test.mjs::font api"
    reason = "..."            # required for adapted/unavailable/internal
    evidence = "..."          # required for unavailable

Every inventory item except fields and enum variants must be matched by exactly
one most-specific rule (the rule with the longest literal pattern prefix wins;
ties are an error). A struct field or enum variant is covered by its parent's
rule; when that rule is `adapted` through serde, the parent must implement
Deserialize. `--require python` (phase 7) or `--require python,js` (phase 8)
demands an entry and an existing test for every bound/adapted rule in those
languages. A rule that matches nothing is stale and fails the check.

Usage: check.py INVENTORY.json [--require python[,js]] [--report OUT.md]
"""
import argparse
import fnmatch
import json
import re
import sys
try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    import tomli as tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
STATUSES = {"bound", "adapted", "unavailable", "internal"}


def compile_pattern(pattern):
    parts = []
    for seg in pattern.split("::"):
        if seg == "**":
            parts.append(r"(?:[^:]+(?:::[^:]+)*)")
        else:
            parts.append(re.escape(seg).replace(r"\*", r"[^:]*"))
    return re.compile("^" + "::".join(parts) + "$")


def literal_prefix(pattern):
    return len(pattern.split("*")[0])


def test_exists(spec):
    """`path::name` or `path::name[param]`: path exists and mentions name/param."""
    if "::" not in spec:
        return False
    path, _, name = spec.partition("::")
    file = ROOT / path
    if not file.is_file():
        return False
    text = file.read_text(errors="replace")
    param = None
    if name.endswith("]") and "[" in name:
        name, _, param = name[:-1].partition("[")
    return name in text and (param is None or param in text)


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("inventory")
    p.add_argument("--require", default="")
    p.add_argument("--report")
    a = p.parse_args()
    require = [x for x in a.require.split(",") if x]
    items = json.load(open(a.inventory))["items"]
    by_path = {i["path"]: i for i in items}
    rules = []
    for file in sorted((ROOT / "bindings/coverage").glob("*.toml")):
        for n, rule in enumerate(tomllib.load(open(file, "rb")).get("rule", [])):
            rule["_where"] = f"{file.relative_to(ROOT)}#{n}"
            rule["_re"] = compile_pattern(rule["pattern"])
            rule["_hits"] = 0
            rules.append(rule)
    errors = []
    for r in rules:
        if r.get("status") not in STATUSES:
            errors.append(f"{r['_where']}: bad status {r.get('status')!r}")
        if r.get("status") in ("adapted", "unavailable", "internal") and not r.get("reason"):
            errors.append(f"{r['_where']}: status {r['status']} needs a reason")
        if r.get("status") == "unavailable" and not r.get("evidence"):
            errors.append(f"{r['_where']}: unavailable needs evidence")
    covered = {}
    unmapped = []
    for item in items:
        if item["kind"] in ("field", "variant"):
            continue
        matches = [r for r in rules if r["_re"].match(item["path"])]
        if not matches:
            unmapped.append(item["path"])
            continue
        best = max(literal_prefix(r["pattern"]) for r in matches)
        top = [r for r in matches if literal_prefix(r["pattern"]) == best]
        if len(top) > 1:
            errors.append(f"{item['path']}: ambiguous rules {[r['_where'] for r in top]}")
        rule = top[0]
        rule["_hits"] += 1
        covered[item["path"]] = rule
    for item in items:
        if item["kind"] not in ("field", "variant"):
            continue
        parent_rule = covered.get(item["parent"])
        if parent_rule is None:
            unmapped.append(item["path"])
            continue
        if item["kind"] == "field" and parent_rule["status"] == "adapted" and "serde" in parent_rule.get("reason", ""):
            parent = by_path.get(item["parent"], {})
            if "Deserialize" not in parent.get("serde", []):
                errors.append(f"{item['path']}: parent adapted via serde but lacks Deserialize")
    for r in rules:
        if r["_hits"] == 0:
            errors.append(f"{r['_where']}: stale rule matches nothing ({r['pattern']})")
        if r.get("status") in ("bound", "adapted"):
            for lang in require:
                if not r.get(lang):
                    errors.append(f"{r['_where']}: no {lang} entry for {r['pattern']}")
                test = r.get(f"{lang}_test")
                if not test or not test_exists(test):
                    errors.append(f"{r['_where']}: {lang}_test missing or not found: {test!r}")
    total = sum(1 for i in items if i["kind"] not in ("field", "variant"))
    status = {}
    for rule in covered.values():
        status[rule["status"]] = status.get(rule["status"], 0) + 1
    summary = {"items": total, "fields_and_variants": len(items) - total, "mapped": len(covered),
               "unmapped": len(unmapped), "by_status": status, "errors": len(errors)}
    if a.report:
        lines = ["# Binding coverage", "", "```json", json.dumps(summary, indent=1), "```", ""]
        lines += ["| Rust item | Status | Python | JS |", "|---|---|---|---|"]
        for path, rule in sorted(covered.items()):
            name = path.split("::")[-1]
            lines.append(f"| `{path}` | {rule['status']} | `{rule.get('python', '').replace('{name}', name)}` "
                         f"| `{rule.get('js', '').replace('{name}', name)}` |")
        Path(a.report).write_text("\n".join(lines) + "\n")
    for e in errors[:200]:
        print("ERROR", e)
    for u in unmapped[:50]:
        print("UNMAPPED", u)
    print(json.dumps(summary))
    sys.exit(1 if errors or unmapped else 0)


if __name__ == "__main__":
    main()
