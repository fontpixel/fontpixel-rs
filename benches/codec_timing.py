#!/usr/bin/env python3
"""Run a JSON codec timing example five times, retaining binary identity and RSS."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--environment-note", required=True)
    args = parser.parse_args()
    if args.output.exists():
        raise SystemExit("Refusing to overwrite timing evidence")
    binary = args.binary.resolve()
    records, peak_rss = [], 0
    started = time.monotonic()
    for _ in range(5):
        with subprocess.Popen([str(binary)], stdout=subprocess.PIPE, stderr=subprocess.PIPE) as proc:
            # Each example emits one short JSON record; neither pipe can fill.
            _, status, usage = os.wait4(proc.pid, 0)
            proc.returncode = os.waitstatus_to_exitcode(status)
            output, error = proc.communicate()
            if proc.returncode:
                raise SystemExit(error.decode(errors="replace"))
        records.append(json.loads(output))
        peak_rss = max(peak_rss, usage.ru_maxrss)
    timings = {}
    for key, value in records[0]["timings"].items():
        if isinstance(value, dict):
            timings[key] = {}
            for metric in value:
                numbers = [record["timings"][key][metric] for record in records]
                timings[key][metric] = {
                    "median": statistics.median(numbers), "min": min(numbers), "max": max(numbers)
                }
        else:
            timings[key] = statistics.median(record["timings"][key] for record in records)
    result = {
        "platform": platform.platform(),
        "binary": str(binary),
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "runs": 5,
        "elapsed_seconds": time.monotonic() - started,
        "peak_rss_kib": peak_rss,
        "timings": timings,
        "records": records,
        "environment_note": args.environment_note,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({k: v for k, v in result.items() if k != "records"}, indent=2))


if __name__ == "__main__":
    main()
