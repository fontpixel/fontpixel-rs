"""Run many operations as one batch, in memory and on the filesystem.

    python batch_jobs.py [INPUT] [OUTPUT_DIR]

Three ways to run the same jobs as `fontpixel batch`:

1. `ops.batch(request, files=...)` with typed task dicts, all in memory;
2. `ops.batch_toml(text, files=...)` with the command line's TOML job format;
3. `ops.batch_file(path)` on the filesystem (outputs written transactionally).

Writes the in-memory outputs under OUTPUT_DIR/memory and runs the TOML file
in OUTPUT_DIR/disk.
"""
from __future__ import annotations

import shutil
import sys

from _common import arguments
from fontpixel import ops
from fontpixel.types import BatchRequest

JOB = """
version = 1

[[tasks]]
id = "pcf"
kind = "convert"
inputs = ["font.bdf"]
output = "out/font.pcf"

[[tasks]]
id = "outline"
kind = "vectorize"
inputs = ["font.bdf"]
output = "out/font.ttf"

[[tasks]]
id = "check"
kind = "validate"
inputs = ["font.bdf"]
"""


def main(argv: list[str] | None = None) -> int:
    source, out = arguments(__doc__ or "", argv)
    files = {"font.bdf": source.read_bytes()}

    # 1. Typed tasks: each is the operation's request dict plus `id`/`kind`.
    request: BatchRequest = {
        "tasks": [
            {"id": "pcf", "kind": "convert", "inputs": ["font.bdf"], "output": "out/font.pcf"},
            {"id": "psf", "kind": "convert", "inputs": ["font.bdf"], "output": "out/font.psf",
             "to": "psf", "psf_version": "2"},
            {"id": "check", "kind": "validate", "input": "font.bdf"},
            # ".psf" is also a PrintShop extension: without `to` this task
            # fails on its own (exit code 1); the other tasks still commit.
            {"id": "ambiguous", "kind": "convert", "inputs": ["font.bdf"], "output": "out/x.psf"},
        ],
        "jobs": 2,
    }
    result = ops.batch(request, files=files)
    for task in result.tasks:
        error = f" ({task['error']})" if task.get("error") else ""
        print(f"{task['id']:>9}: {task['kind']:<9} exit {task['exit_code']}{error}")
    memory = out / "memory"
    for key, data in sorted(result.outputs.items()):
        (memory / key).parent.mkdir(parents=True, exist_ok=True)
        (memory / key).write_bytes(data)
    print("batch exit code:", result.exit_code, "| outputs:", sorted(result.outputs))
    assert result.exit_code == 1 and "out/font.psf" in result.outputs

    # 2. The TOML job format, still in memory.
    toml = ops.batch_toml(JOB, files=files)
    print("toml tasks:", [t["id"] for t in toml.document["tasks"]])
    parsed = ops.parse_batch_toml(JOB)
    print("parsed:", [(t["id"], t.get("weight")) for t in parsed["tasks"]],
          "| working budget:", parsed["max_working_bytes"])

    # 3. On disk: paths are relative to the job file's directory.
    disk = out / "disk"
    if disk.exists():
        shutil.rmtree(disk)
    (disk / "out").mkdir(parents=True)
    shutil.copyfile(source, disk / "font.bdf")
    (disk / "jobs.toml").write_text(JOB)
    on_disk = ops.batch_file(disk / "jobs.toml")
    print("on disk:", sorted(p.name for p in (disk / "out").iterdir()), "exit", on_disk.exit_code)
    # The JSON document is exactly what `fontpixel batch` prints.
    assert on_disk.json().startswith("{")
    return 0 if on_disk.exit_code == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
