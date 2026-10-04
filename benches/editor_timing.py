#!/usr/bin/env python3
"""Five complete large-font editor-codec runs, retaining source/binary identity."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--font", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--environment-note", required=True)
    args = parser.parse_args()
    if args.output.exists():
        raise SystemExit("Refusing to overwrite timing evidence")
    binary, font = args.binary.resolve(), args.font.resolve()
    command = [str(binary), str(font), "1"]
    records, peak_rss = [], 0
    for _ in range(5):
        with subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE) as proc:
            _, status, usage = os.wait4(proc.pid, 0)
            proc.returncode = os.waitstatus_to_exitcode(status)
            output, error = proc.communicate()
        if proc.returncode:
            raise SystemExit(error.decode(errors="replace"))
        rows = json.loads(output)
        if len(rows) != 3 or {row["format"] for row in rows} != {"bmfont", "sfd", "kbits"}:
            raise SystemExit("Unexpected timing schema")
        records.append({row["format"]: row for row in rows})
        peak_rss = max(peak_rss, usage.ru_maxrss)
    summary = {}
    for fmt in ("bmfont", "sfd", "kbits"):
        rows = [record[fmt] for record in records]
        for field in ("glyphs", "bytes", "loops"):
            if len({row[field] for row in rows}) != 1:
                raise SystemExit(f"Unstable {fmt} {field}")
        summary[fmt] = {field: rows[0][field] for field in ("glyphs", "bytes", "loops")}
        for operation in ("read", "write"):
            values = [row[f"{operation}_ns"] / 1e6 for row in rows]
            summary[fmt][f"{operation}_ms"] = {
                "median": statistics.median(values), "min": min(values), "max": max(values)
            }
    result = {
        "platform": platform.platform(),
        "command": command,
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "source_sha256": hashlib.sha256(font.read_bytes()).hexdigest(),
        "runner_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "runs": 5,
        "method": "One in-memory whole-font read/write per process; no timed BDF parse or warmups",
        "peak_rss_kib": peak_rss,
        "summary": summary,
        "records": records,
        "environment_note": args.environment_note,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({key: value for key, value in result.items() if key != "records"}, indent=2))


if __name__ == "__main__":
    main()
