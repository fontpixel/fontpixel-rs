#!/usr/bin/env python3
"""Standard-library-only documentation/source checks; never execute project code.

This is not a Rust type checker, native test, performance check or acceptance gate.
It reads source fragments, Cargo feature names, Markdown links, documented
contracts, illustrative TOML syntax and rustdoc inclusion.
No project import, subprocess, Cargo command, CLI or Rust example is invoked.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_fragment_present(text: str, fragment: str) -> bool:
    """Permit formatting whitespace without joining identifiers or string words.

    This remains a lexical source sketch, not a Rust parser. Exact fragments
    (including diagnostic prose inside a string) retain the original check.
    """
    if fragment in text:
        return True
    token = r'"(?:\\.|[^"\\])*"|[A-Za-z_][A-Za-z_0-9]*|[0-9]+|[^\s]'
    needle = "\0" + "\0".join(re.findall(token, fragment)) + "\0"
    haystack = "\0" + "\0".join(re.findall(token, text)) + "\0"
    return needle in haystack


def local_links(path: Path):
    for target in re.findall(r"\[[^\]]+\]\(([^)]+)\)", path.read_text()):
        if "://" in target or target.startswith("#"):
            continue
        if re.match(r"^(mod|fn|struct|enum|trait)@", target):
            continue  # A rustdoc-disambiguated symbol is not a filesystem path.
        name = target.split("#", 1)[0]
        if name:
            yield path.parent / name


def check(require_rustdoc_link: bool):
    contract_path = ROOT / "docs/api-example-contracts.json"
    contract = json.loads(contract_path.read_text())
    source_hashes = {}
    declarations = 0
    for reference in contract["references"]:
        path = ROOT / reference["source"]
        text = path.read_text()
        for declaration in reference["declarations"]:
            if not source_fragment_present(text, declaration):
                raise ValueError(f"missing source reference: {reference['capability']}: {declaration}")
            declarations += 1
        source_hashes[reference["source"]] = digest(path)

    documented_fragments = 0
    stale_claims = 0
    for requirement in contract.get("documentation_contracts", []):
        path = ROOT / requirement["source"]
        normalized = " ".join(path.read_text().split())
        for phrase in requirement.get("required", []):
            if " ".join(phrase.split()) not in normalized:
                raise ValueError(f"missing documented contract in {requirement['source']}: {phrase}")
            documented_fragments += 1
        for phrase in requirement.get("forbidden", []):
            if " ".join(phrase.split()) in normalized:
                raise ValueError(f"stale documentation in {requirement['source']}: {phrase}")
            stale_claims += 1
        source_hashes[requirement["source"]] = digest(path)

    toml_examples = 0
    for expected in contract.get("toml_examples", []):
        path = ROOT / expected["source"]
        matches = []
        for block in re.findall(r"^```toml\s*\n(.*?)^```\s*$", path.read_text(),
                                re.MULTILINE | re.DOTALL):
            job = tomllib.loads(block)
            for task in job.get("tasks", []):
                if task.get("id") == expected["task_id"]:
                    matches.append((job, task))
        if len(matches) != 1:
            raise ValueError(f"expected one TOML example for {expected['task_id']}")
        job, task = matches[0]
        if (job.get("version") != expected["version"]
                or task.get("kind") != expected["kind"]
                or task.get("to") != expected["format"]
                or task.get("options") != expected["options"]):
            raise ValueError(f"TOML example contract changed: {expected['task_id']}")
        toml_examples += 1

    example = ROOT / contract["example"]
    text = example.read_text()
    if "uncompiled" not in text or "untested" not in text:
        raise ValueError("example must retain its current uncompiled/untested status")
    for forbidden in ["std::fs", "std::process", "Command::new", "include_bytes!"]:
        if forbidden in text:
            raise ValueError(f"filesystem-free example has an unexpected dependency: {forbidden}")
    cargo_path = ROOT / "crates/fontpixel/Cargo.toml"
    cargo = tomllib.loads(cargo_path.read_text())
    for feature in contract["features"]:
        if feature not in cargo["features"]:
            raise ValueError(f"unknown feature: {feature}")
        if f'#[cfg(feature = "{feature}")]' not in text:
            raise ValueError(f"example lacks its documented feature gate: {feature}")
    if cargo["package"].get("autoexamples", True) is False:
        raise ValueError("automatic example discovery is disabled; review registration")
    source_hashes[contract["example"]] = digest(example)
    source_hashes["crates/fontpixel/Cargo.toml"] = digest(cargo_path)
    source_hashes["docs/api-example-contracts.json"] = digest(contract_path)
    source_hashes["examples/check_api_documentation.py"] = digest(Path(__file__))

    doc_path = ROOT / contract["rustdoc"]
    rustdoc = doc_path.read_text()
    blocks = len(re.findall(r"^```(?:rust)?\s*$", rustdoc, re.MULTILINE)) // 2
    if blocks < contract["minimum_rustdoc_examples"]:
        raise ValueError("crate-root code examples are missing")
    lib_path = ROOT / "crates/fontpixel/src/lib.rs"
    linked = contract["rustdoc_include"] in lib_path.read_text()
    if require_rustdoc_link and not linked:
        raise ValueError("lib.rs does not include docs/api-rustdoc.md; root patch is still pending")
    source_hashes[contract["rustdoc"]] = digest(doc_path)
    source_hashes["crates/fontpixel/src/lib.rs"] = digest(lib_path)

    guides = ["README.md", "docs/api.md", "docs/api-rustdoc.md", "docs/bitmap-model.md",
              "docs/migration-fontpixel.md", "docs/migration-tong.md", "docs/w6.1-cli.md",
              "docs/recovery-status.md", "docs/development.md", "examples/README.md",
              "docs/w6.1-writer-budget.md"]
    links = 0
    for name in guides:
        path = ROOT / name
        for target in local_links(path):
            if not target.exists():
                raise ValueError(f"missing local link in {name}: {target}")
            links += 1
        source_hashes[name] = digest(path)
    batch = (ROOT / "docs/w6.1-cli.md").read_text()
    for phrase in ["shared input-identity table", "100,000", "disk-backed", "RSS", "sticky"]:
        if phrase not in batch:
            raise ValueError(f"batch budget documentation omits: {phrase}")

    return {"status": "static_checks_passed" if linked else "static_checks_passed_with_pending_rustdoc_link",
            "native_compilation": "not performed", "native_runtime": "not performed",
            "rust_type_checking": "not performed", "rustdoc_link_present": linked,
            "lexical_declarations_checked": declarations, "source_reference_groups": len(contract["references"]),
            "source_match": "exact text or formatting-insensitive tokens; not Rust parsing/type checking",
            "documented_fragments_checked": documented_fragments,
            "stale_claims_excluded": stale_claims, "toml_examples_parsed": toml_examples,
            "feature_gates": contract["features"], "rustdoc_code_blocks": blocks,
            "local_links_checked": links, "source_sha256": source_hashes,
            "limits": "Lexical source presence and TOML parsing do not prove Rust signatures type-check, CLI parsing accepts a job, or operations succeed."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--require-rustdoc-link", action="store_true")
    parser.add_argument("--output", type=Path, help="optional new JSON evidence file; refuses replacement")
    args = parser.parse_args()
    result = check(args.require_rustdoc_link)
    encoded = json.dumps(result, indent=2) + "\n"
    if args.output:
        with args.output.open("x") as output:
            output.write(encoded)
    print(encoded, end="")


if __name__ == "__main__":
    main()
