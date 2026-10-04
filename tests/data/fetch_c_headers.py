#!/usr/bin/env python3
"""Download pinned, public-domain C headers outside the repository, verifying SHA-256."""
import argparse
import hashlib
import json
from pathlib import Path
from urllib.request import urlopen

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("output", type=Path)
args = parser.parse_args()
manifest = json.loads(Path(__file__).with_name("c-upstream-sources.json").read_text())
args.output.mkdir(parents=True, exist_ok=True)
for entry in manifest["files"]:
    data = urlopen(entry["url"], timeout=30).read()
    assert hashlib.sha256(data).hexdigest() == entry["sha256"], entry["name"]
    destination = args.output / entry["name"]
    if destination.exists():
        assert destination.read_bytes() == data, destination
    else:
        destination.write_bytes(data)
print(f'Verified {len(manifest["files"])} {manifest["license"]} headers')
