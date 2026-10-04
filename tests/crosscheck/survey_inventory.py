#!/usr/bin/env python3
"""Validate W5.1's complete, version-pinned format inventory without building Rust.

The default is offline. --monobit also verifies the installed independent oracle;
--upstream fetches the pinned upstream README and verifies every catalogue row.
This checks survey coverage, not production codec implementation or acceptance.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.metadata
import json
from pathlib import Path
import re
import urllib.request


ROOT = Path(__file__).resolve().parents[2]


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"survey inventory: {message}")


def catalogue(text: str) -> list[tuple[str, str]]:
    lines = text.splitlines()
    section = ""
    result = []
    for i, line in enumerate(lines):
        if i + 1 < len(lines) and re.fullmatch(r"[-=]{3,}", lines[i + 1]):
            section = line
        if line.startswith("|") and "`" in line:
            columns = [c.strip() for c in line.strip("|").split("|")]
            if len(columns) > 2:
                match = re.search(r"`([^`]+)`", columns[1])
                if match:
                    result.append((section, match.group(1)))
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inventory", type=Path, default=ROOT / "reports/w5.1-survey.json")
    parser.add_argument("--monobit", action="store_true")
    parser.add_argument("--upstream", action="store_true")
    args = parser.parse_args()
    data = json.loads(args.inventory.read_text())
    require(data["schema_version"] == 1, "unsupported schema")
    original = set(data["original_ids"])
    formats = data["new_formats"]
    added = {f["id"] for f in formats}
    require(len(added) == len(formats), "duplicate canonical ids")
    require(not original & added, "new ids duplicate original ids")
    aliases = {}
    for item in formats:
        require(item["feasible"] is True, f"unexplained exclusion: {item['id']}")
        for key in ("read_write_scope", "verification_risks", "primary_sources", "independent_oracles", "sample"):
            require(bool(item[key]), f"{item['id']} lacks {key}")
        require(item["complexity"] in data["complexity_scale"], "unknown complexity")
        require(all(url.startswith(("https://", "http://")) for url in item["primary_sources"]), "non-URL source")
        for name in item["monobit_names"]:
            require(name not in aliases, f"duplicate upstream alias: {name}")
            aliases[name] = item["id"]
    grouped = [name for group in data["implementation_groups"] for name in group["new_ids"]]
    require(len(grouped) == len(set(grouped)) and set(grouped) == added, "groups do not partition new ids")
    bitmap = data["monobit_bitmap_inventory"]
    require(len({row["id"] for row in bitmap}) == len(bitmap), "duplicate bitmap rows")
    for row in bitmap:
        require(row["canonical_id"] in original | added, f"unmapped bitmap: {row['id']}")
        if row["id"] in aliases:
            require(row["canonical_id"] == aliases[row["id"]], "inconsistent canonical alias")
    for row in data["monobit_wrappers"]:
        require(bool(row["reason"]), f"wrapper lacks classification reason: {row['id']}")
    for row in data["existing_expansions"]:
        require(row["id"] in original, "expansion targets an unknown original id")
    require(len(data["original_status_snapshot"]) == len(original), "incomplete accepted-base snapshot")
    result = {
        "schema_version": 1,
        "inventory_sha256": hashlib.sha256(args.inventory.read_bytes()).hexdigest(),
        "scope": "survey coverage only; not codec acceptance",
        "original_ids": len(original),
        "new_ids": len(added),
        "monobit_bitmap_rows": len(bitmap),
        "monobit_wrapper_rows": len(data["monobit_wrappers"]),
        "monobit_stroke_rows": len(data["monobit_stroke_inventory"]),
        "native_sample_hash_records": len(data["native_sample_fetches"]),
        "offline_validation": "pass",
    }
    if args.monobit:
        import monobit
        import monobit.storage.fontformats  # noqa: F401: register the oracle formats
        from monobit.storage.base import container_loaders, container_savers

        require(importlib.metadata.version("monobit") == data["monobit"]["pypi_version"], "oracle version changed")
        expected_read = {row["id"] for row in bitmap} | {"borland", "gimms", "hurt", "svg"}
        actual_read = set(monobit.loaders.get_formats()) | set(container_loaders.get_formats())
        require(expected_read == actual_read, f"oracle reader difference: {sorted(expected_read ^ actual_read)}")
        expected_write = {row["id"] for row in bitmap if row["write"]} | {"borland", "svg"}
        actual_write = set(monobit.savers.get_formats()) | set(container_savers.get_formats())
        require(expected_write == actual_write, f"oracle writer difference: {sorted(expected_write ^ actual_write)}")
        package = Path(monobit.__file__).resolve().parent.parent
        for item in data["monobit"]["source_hashes"]:
            path = package / item["path"]
            require(path.is_file(), f"missing oracle source: {item['path']}")
            require(hashlib.sha256(path.read_bytes()).hexdigest() == item["sha256"], f"modified oracle source: {item['path']}")
        result["installed_oracle_validation"] = "pass"
        result["oracle_source_files_hashed"] = len(data["monobit"]["source_hashes"])
    if args.upstream:
        commit = data["monobit"]["github_commit"]
        url = f"https://raw.githubusercontent.com/robhagemans/monobit/{commit}/README.md"
        with urllib.request.urlopen(url, timeout=30) as response:
            payload = response.read(1_000_001)
        require(len(payload) <= 1_000_000, "unexpected oversized upstream README")
        require(hashlib.sha256(payload).hexdigest() == data["monobit"]["readme_sha256"], "upstream README hash changed")
        expected = [(row["section"], row["id"]) for name in ("monobit_bitmap_inventory", "monobit_stroke_inventory", "monobit_wrappers") for row in data[name]]
        require(catalogue(payload.decode()) == expected, "upstream catalogue has unmapped/reordered rows")
        result["pinned_upstream_catalogue_validation"] = "pass"
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
