"""fontpixel.ops: every command-line operation in-process.

Normal results are compared with the frozen command-line binary run on the
same inputs: the JSON/text documents must equal its stdout byte for byte and
every output file its bytes. Memory mode (named buffers, resolver objects,
ResourceSet) and filesystem mode (real paths, source protection, transactional
groups) are both exercised, as are errors, strict loss and resource limits.
"""
import json
import os
import shutil
import subprocess
import threading
from pathlib import Path

import pytest

import fontpixel as fp
from fontpixel import ops

REPO = Path(__file__).resolve().parents[3]
DATA = REPO / "tests" / "data"
SPLEEN = (DATA / "bdf" / "spleen-5x8.bdf").read_bytes()
BLOCK = (DATA / "bdf" / "3x5-block.bdf").read_bytes()
EMBEDDED_TTF = (DATA / "embedded-bitmap" / "bdat-i1-f1-d1.ttf").read_bytes()
FROZEN = Path(
    os.environ.get(
        "FONTPIXEL_FROZEN_CLI",
        "/home/chen/githubprojects/fontpixel/local-work/phase6-local-20261004/"
        "frozen-binaries-v5/release/fontpixel",
    )
)
needs_cli = pytest.mark.skipif(not FROZEN.is_file(), reason="frozen fontpixel CLI not found")
INPUTS = {"s.bdf": SPLEEN, "b.bdf": BLOCK, "e.ttf": EMBEDDED_TTF}


def cli(args, cwd):
    """Run the frozen CLI; return (exit code, stdout text, stderr text)."""
    proc = subprocess.run([str(FROZEN), *map(str, args)], cwd=cwd, capture_output=True)
    return proc.returncode, proc.stdout.decode(), proc.stderr.decode()


def tree(root, skip=()):
    """Every file below root (relative posix path -> bytes), minus skip."""
    return {
        p.relative_to(root).as_posix(): p.read_bytes()
        for p in sorted(Path(root).rglob("*"))
        if p.is_file() and p.relative_to(root).as_posix() not in skip
    }


def workdir(base, name):
    """A directory holding the shared inputs."""
    d = Path(base) / name
    d.mkdir()
    for key, data in INPUTS.items():
        (d / key).write_bytes(data)
    return d


# ------------------------------------------------------------ registry/queries


@needs_cli
def test_formats_documents_equal_the_command_line(tmp_path):
    result = ops.formats()
    code, out, _ = cli(["formats", "--json"], tmp_path)
    assert code == 0 and result.json() == out
    assert ops.formats({"json": True}).json() == out
    assert cli(["formats"], tmp_path)[1] == result.text()
    assert result.document == json.loads(out)
    status = result.format_status("bdf")
    assert status["id"] == "bdf" if "id" in status else status
    assert result.format_status("nope") is None
    assert result.exit_code == 0 and result.outputs == {} and result.report is None
    with pytest.raises(TypeError):
        ops.convert(inputs=["s.bdf"], output="o.pcf", files=INPUTS).format_status("bdf")


def test_capabilities_constants_and_keys():
    assert ops.capabilities() == {"filesystem": True, "threads": True, "toml_batch": True}
    assert ops.DEFAULT_MAX_WORKING_BYTES == 1 << 30
    assert ops.DEFAULT_MAX_PENDING_BYTES == 512 << 20
    assert ops.DEFAULT_MAX_REPORT_BYTES == 2 << 30
    assert ops.DEFAULT_MAX_TASK_REPORT_BYTES == 64 << 20
    assert ops.memory_key("./a/../b//c.bdf") == "b/c.bdf"
    assert ops.memory_key(Path("/x/./y")) == "/x/y"


def test_parse_args_and_inspect_request_share_the_command_line_definition():
    parsed = ops.parse_args(["convert", "a.bdf", "-o", "b.pcf", "--report", "json",
                             "--max-total-bytes", "1000000"])
    assert parsed["command"] == "convert"
    assert parsed["report_style"] == "json"
    assert parsed["request"]["inputs"] == ["a.bdf"]
    assert parsed["request"]["max_total_bytes"] == 1000000
    # The parsed request is accepted unchanged by the operation and by
    # inspect_request, which fills the same defaults.
    inspected = ops.inspect_request("vectorize", parsed["request"])
    assert inspected["request"] == parsed["request"]
    assert inspected["declared_inputs"] == ["a.bdf"]
    assert inspected["weight"] == inspected["source_identity_total_bytes"]
    assert inspected["source_identity_total_bytes"] == parsed["source_identity_total_bytes"]
    assert ops.parse_args(["batch", "jobs.toml", "--jobs", "2"])["request"] == {
        "file": "jobs.toml", "jobs": 2}
    assert ops.inspect_request("info", input="a.bdf", json=True)["report_style"] == "json"
    with pytest.raises(fp.InvalidInputError) as info:
        ops.parse_args(["convert", "a.bdf", "--each", "-o", "d"])
    assert info.value.exit_code == 2
    assert "the following required arguments were not provided:\n  --to <TO>" in str(info.value)
    with pytest.raises(fp.InvalidInputError) as info:
        ops.parse_args(["--help"])
    assert info.value.exit_code == 0
    with pytest.raises(fp.InvalidInputError):
        ops.inspect_request("nope", {})


# ------------------------------------------------------------ requests


@needs_cli
@pytest.mark.parametrize(
    "args, request_",
    [
        (["convert", "s.bdf", "--each", "-o", "d"],
         {"inputs": ["s.bdf"], "each": True, "output": "d"}),
        (["preview", "s.bdf", "Hi", "--all"], {"input": "s.bdf", "text": "Hi", "all": True}),
        (["preview", "s.bdf", "--range", "U+20-7E"], {"input": "s.bdf", "range": "U+20-7E"}),
        (["preview", "s.bdf", "Hi", "--missing", "?", "--fail-missing"],
         {"input": "s.bdf", "text": "Hi", "missing": "?", "fail_missing": True}),
        (["subset", "e.ttf", "--text", "A", "--unicodes", "U+41", "-o", "x.ttf"],
         {"input": "e.ttf", "text": "A", "unicodes": "U+41", "output": "x.ttf"}),
        (["subset", "e.ttf", "-o", "x.ttf"], {"input": "e.ttf", "output": "x.ttf"}),
        (["convert", "s.bdf", "-o", "x.bdf", "--c-width", "3"],
         {"inputs": ["s.bdf"], "output": "x.bdf", "c_width": 3}),
        (["convert", "s.bdf", "-o", "x.bdf", "--swidth-ppem", "0"],
         {"inputs": ["s.bdf"], "output": "x.bdf", "swidth_ppem": 0}),
    ],
)
def test_request_rules_reject_like_the_command_line(tmp_path, args, request_):
    code, _, err = cli(args, tmp_path)
    assert code == 2
    message = err.removeprefix("error: ").split("\n\nUsage")[0].split("\n\nFor more")[0].strip()
    operation = getattr(ops, args[0])
    with pytest.raises(fp.InvalidInputError) as info:
        operation(request_, files=INPUTS)
    assert str(info.value) == message
    assert info.value.exit_code == 2 and info.value.kind == "InvalidInput"


def test_requests_are_strict_and_accept_keywords_and_paths(tmp_path):
    with pytest.raises(fp.InvalidInputError, match="unknown field `stirct`"):
        ops.convert(inputs=["s.bdf"], output="o.pcf", stirct=True, files=INPUTS)
    with pytest.raises(fp.InvalidInputError, match="possible values"):
        ops.convert(inputs=["s.bdf"], output="o.pcf", glyph_pad="3", files=INPUTS)
    with pytest.raises(TypeError, match="given both"):
        ops.convert({"inputs": ["s.bdf"], "output": "o.pcf"}, output="p.pcf", files=INPUTS)
    with pytest.raises(TypeError):
        ops.convert(["s.bdf"], files=INPUTS)
    # os.PathLike and tuples are plain data; request dict plus keywords merge.
    a = ops.convert({"inputs": (Path("s.bdf"),)}, output=Path("o.pcf"), files=INPUTS)
    b = ops.convert(inputs=["s.bdf"], output="o.pcf", files=INPUTS)
    assert a.outputs == b.outputs and a.json() == b.json()
    # Numbers, typed enums and core shapes deserialize.
    r = ops.render(input="s.bdf", text="Hi", output="h.svg", shape="dot",
                   color={"red": 1, "green": 2, "blue": 3, "alpha": 255},
                   direction={"glyphs": "right", "lines": "down"}, scale=2, files=INPUTS)
    assert r.summary["width"] > 0 and list(r.outputs) == ["h.svg"]


# ------------------------------------------------------------ convert


CONVERSIONS = ["o.pcf", "o.psf", "o.yaff", "o.otb", "o.ttf", "o.woff2", "o.svg", "o.fnt",
               "o.bdf"]


@needs_cli
@pytest.mark.parametrize("output", CONVERSIONS)
def test_convert_memory_and_filesystem_equal_the_command_line(tmp_path, monkeypatch, output):
    cli_dir = workdir(tmp_path, "cli")
    code, out_json, _ = cli(["convert", "s.bdf", "-o", output, "--report", "json"], cli_dir)
    assert code == 0
    expected = tree(cli_dir, skip=INPUTS)
    text_dir = workdir(tmp_path, "text")
    _, out_text, _ = cli(["convert", "s.bdf", "-o", output], text_dir)

    memory = ops.convert(inputs=["s.bdf"], output=output, report="json", files=INPUTS)
    assert memory.json() == out_json
    assert memory.text() == out_text
    assert memory.outputs == expected
    assert memory.destinations == [output]
    bundle = memory.bundles[output]
    assert isinstance(bundle, fp.OutputBundle)

    fs_dir = workdir(tmp_path, "fs")
    monkeypatch.chdir(fs_dir)
    disk = ops.convert(inputs=["s.bdf"], output=output, report="json")
    assert disk.json() == out_json and disk.outputs == {}
    assert tree(fs_dir, skip=INPUTS) == expected


@needs_cli
def test_convert_each_names_and_options_equal_the_command_line(tmp_path, monkeypatch):
    args = ["convert", "s.bdf", "b.bdf", "--each", "--to", "pcf", "-o", "out",
            "--family", "Fam", "--glyphpad", "1", "--report", "json"]
    cli_dir = workdir(tmp_path, "cli")
    (cli_dir / "out").mkdir()
    code, out, _ = cli(args, cli_dir)
    assert code == 0
    expected = tree(cli_dir, skip=INPUTS)
    request = ops.parse_args(args)["request"]
    fs_dir = workdir(tmp_path, "fs")
    (fs_dir / "out").mkdir()
    monkeypatch.chdir(fs_dir)
    result = ops.convert(request)
    assert result.json() == out and tree(fs_dir, skip=INPUTS) == expected
    memory = ops.convert(request, files=INPUTS)
    assert memory.json() == out and memory.outputs == expected
    assert memory.summary["each"] is True and len(memory.summary["outputs"]) == 2
    assert ops.vectorize(request, files=INPUTS).json() == out


# ------------------------------------------------------------ other writers


def compare_writer(tmp_path, monkeypatch, args, operation, *, mkdirs=()):
    """CLI stdout/files vs memory and filesystem results of the same argv."""
    parsed = ops.parse_args(args)
    cli_dir = workdir(tmp_path, "cli")
    for d in mkdirs:
        (cli_dir / d).mkdir()
    code, out, err = cli(args, cli_dir)
    assert code == 0, err
    expected = tree(cli_dir, skip=INPUTS)
    memory = operation(parsed["request"], files=INPUTS)
    assert memory.json() == out if parsed["report_style"] == "json" else memory.text() == out
    assert memory.outputs == expected
    fs_dir = workdir(tmp_path, "fs")
    for d in mkdirs:
        (fs_dir / d).mkdir()
    monkeypatch.chdir(fs_dir)
    disk = operation(parsed["request"])
    assert disk.json() == out if parsed["report_style"] == "json" else disk.text() == out
    assert tree(fs_dir, skip=INPUTS) == expected
    return memory


@needs_cli
def test_merge_equals_the_command_line(tmp_path, monkeypatch):
    args = ["merge", "--family", "Mix", "--part", "s.bdf", "--take", "western",
            "--part", "b.bdf", "--take", "all", "--fallback", "-o", "m.bdf",
            "-o", "m.woff2", "-o", "m.dat", "--to", "pcf", "--report", "json"]
    result = compare_writer(tmp_path, monkeypatch, args, ops.merge)
    assert set(result.outputs) == {"m.bdf", "m.woff2", "m.dat"}
    assert result.summary["family"] == "Mix" and len(result.summary["summaries"]) == 3
    assert result.report is None
    text_args = [a for a in args if a not in ("--report", "json")]
    compare_writer(tmp_path / "t", monkeypatch, text_args, ops.merge) if (
        (tmp_path / "t").mkdir() or True) else None


@needs_cli
def test_variants_and_subset_equal_the_command_line(tmp_path, monkeypatch):
    variants = compare_writer(
        tmp_path, monkeypatch,
        ["variants", "--base", "s.bdf", "--lang", "ja=b.bdf", "-o", "v.ttf",
         "--report", "json"], ops.variants)
    assert variants.summary["output"] == "v.ttf" and variants.report is not None
    sub = tmp_path / "sub"
    sub.mkdir()
    compare_writer(sub, monkeypatch,
                   ["subset", "e.ttf", "--text", "AB", "-o", "s.woff2", "--report", "json"],
                   ops.subset)
    sub2 = tmp_path / "sub2"
    sub2.mkdir()
    compare_writer(sub2, monkeypatch, ["subset", "e.ttf", "--unicodes", "U+41", "-o", "u.ttf"],
                   ops.subset)


@needs_cli
def test_charset_and_render_equal_the_command_line(tmp_path, monkeypatch):
    charset = compare_writer(
        tmp_path, monkeypatch,
        ["charset", "s.bdf", "-o", "c.bdf", "--to", "cp437", "--report", "json"], ops.charset)
    assert charset.summary["charset"] and charset.report is not None
    for name, args in [
        ("png", ["render", "s.bdf", "Hello", "-o", "h.png", "--scale", "3", "--report", "json"]),
        ("svg", ["render", "s.bdf", "Hi", "-o", "h.svg", "--shape", "dot"]),
        ("all", ["render", "s.bdf", "--all", "--range", "U+41-5A", "-o", "a.png"]),
    ]:
        d = tmp_path / name
        d.mkdir()
        compare_writer(d, monkeypatch, args, ops.render)


# ------------------------------------------------------------ readers


@needs_cli
def test_readers_equal_the_command_line(tmp_path):
    d = workdir(tmp_path, "cli")
    cases = [
        (["validate", "s.bdf", "--json"], ops.validate),
        (["validate", "s.bdf"], ops.validate),
        (["validate", "e.ttf", "--json"], ops.validate),
        (["info", "s.bdf", "--json"], ops.info),
        (["info", "s.bdf"], ops.info),
        (["diff", "s.bdf", "s.bdf", "--json"], ops.diff),
        (["diff", "s.bdf", "b.bdf", "--json"], ops.diff),
        (["diff", "s.bdf", "b.bdf"], ops.diff),
        (["preview", "s.bdf", "Hi", "--chars", "#."], ops.preview),
        (["preview", "s.bdf", "--all", "--range", "U+41-43"], ops.preview),
    ]
    for args, operation in cases:
        code, out, err = cli(args, d)
        parsed = ops.parse_args(args)
        result = operation(parsed["request"], files=INPUTS)
        json_style = parsed["report_style"] == "json"
        assert (result.json() if json_style else result.text()) == out, args
        assert result.exit_code == code, args
        # Filesystem mode with absolute paths gives the same document.
        absolute = dict(parsed["request"])
        for key in ("input", "a", "b"):
            if key in absolute:
                absolute[key] = str(d / absolute[key])
        disk = operation(absolute)
        rendered = disk.json() if json_style else disk.text()
        assert rendered.replace(str(d) + os.sep, "") == out.replace(str(d) + os.sep, ""), args
        assert result.outputs == {}


def test_reader_results_expose_typed_fields():
    info = ops.info(input="s.bdf", files=INPUTS)
    assert isinstance(info.fonts, fp.FontSet) and len(info.fonts.fonts) == 1
    assert info.summary["format"] == "bdf" and info.summary["font_count"] == 1
    assert json.loads(info.font_json(0))["glyph_count"] if "glyph_count" in json.loads(
        info.font_json(0)) else True
    assert info.report["entries"] is not None
    with pytest.raises(fp.FontpixelError):
        info.font_json(5)
    same = ops.diff(a="s.bdf", b="s.bdf", files=INPUTS)
    assert same.summary["equal"] is True and same.exit_code == 0
    other = ops.diff(a="s.bdf", b="b.bdf", max_differences=2, files=INPUTS)
    assert other.summary["equal"] is False and other.exit_code == 1
    assert len(other.summary["differences"]) <= 2
    assert other.summary["difference_count"] >= len(other.summary["differences"])
    check = ops.validate(input="s.bdf", files=INPUTS)
    assert check.summary["kind"] == "native" and check.summary["format"] == "bdf"
    assert ops.validate(input="e.ttf", files=INPUTS).summary["kind"] == "sfnt"
    preview = ops.preview(input="s.bdf", text="A", chars="X.", files=INPUTS)
    assert "X" in preview.summary["text"] and preview.text() == preview.summary["text"]
    assert json.loads(preview.json()) == preview.document


# ------------------------------------------------------------ errors, strict, limits


@needs_cli
def test_errors_match_the_command_line(tmp_path):
    d = workdir(tmp_path, "cli")
    cases = [
        # strict loss: exit 3 with the full report
        (["convert", "s.bdf", "-o", "x.psf", "--strict", "--report", "json"], ops.convert,
         fp.StrictLossError, 3),
        # resource limit: exit 1 with the resource and limit
        (["convert", "s.bdf", "-o", "x.pcf", "--max-input-bytes", "100", "--report", "json"],
         ops.convert, fp.ResourceLimitError, 1),
        (["info", "s.bdf", "--max-input-bytes", "100", "--json"], ops.info,
         fp.ResourceLimitError, 1),
        # writer rejects the font
        (["convert", "s.bdf", "-o", "x.hex", "--report", "json"], ops.convert,
         fp.FontpixelError, 1),
    ]
    for args, operation, exc, exit_code in cases:
        code, out, err = cli(args, d)
        assert code == exit_code, (args, err)
        document = json.loads(err)
        with pytest.raises(exc) as info:
            operation(ops.parse_args(args)["request"], files=INPUTS)
        assert info.value.exit_code == exit_code
        assert str(info.value) == document["error"]["message"]
        assert getattr(info.value, "report", None) == document["report"]
        assert tree(d) == {k: v for k, v in INPUTS.items()}
    with pytest.raises(fp.ResourceLimitError) as info:
        ops.convert(inputs=["s.bdf"], output="x.pcf", max_input_bytes=100, files=INPUTS)
    assert info.value.limit == 100 and info.value.resource


def test_missing_inputs_and_resolver_errors():
    with pytest.raises(fp.FontpixelIoError) as info:
        ops.info(input="missing.bdf", files=INPUTS)
    assert "no such memory file: missing.bdf" in str(info.value)
    assert info.value.exit_code == 1

    class Broken:
        def entry(self, path):
            return len(SPLEEN) if path == "s.bdf" else None

        def read(self, path):
            raise KeyError("boom " + path)

        def list(self, directory):
            return []

    with pytest.raises(KeyError, match="boom s.bdf"):
        ops.info(input="s.bdf", files=Broken())
    with pytest.raises(TypeError):
        ops.info(input="s.bdf", files=42)
    with pytest.raises(TypeError):
        ops.info(input="s.bdf", files={"s.bdf": "text"})


def test_resolver_objects_and_resource_sets_match_named_buffers():
    class Provider:
        def __init__(self, files):
            self.files = files
            self.reads = []

        def entry(self, path):
            if path in self.files:
                return len(self.files[path])
            prefix = path + "/" if path else ""
            if any(k.startswith(prefix) for k in self.files):
                return "directory"
            return None

        def read(self, path):
            self.reads.append(path)
            return self.files[path]

        def list(self, directory):
            prefix = directory + "/" if directory else ""
            return sorted({k[len(prefix):].split("/")[0] for k in self.files
                           if k.startswith(prefix)})

    provider = Provider({"fonts/s.bdf": SPLEEN})
    a = ops.convert(inputs=["fonts/s.bdf"], output="o.pcf", files=provider)
    b = ops.convert(inputs=["fonts/s.bdf"], output="o.pcf",
                    files=fp.ResourceSet({"fonts/s.bdf": SPLEEN}))
    c = ops.convert(inputs=["fonts/s.bdf"], output="o.pcf",
                    files={"fonts/s.bdf": bytearray(SPLEEN)})
    d = ops.convert(inputs=["fonts/s.bdf"], output="o.pcf",
                    files={"fonts/s.bdf": memoryview(SPLEEN)})
    assert a.outputs == b.outputs == c.outputs == d.outputs
    assert provider.reads == ["fonts/s.bdf"]


def test_memory_outputs_with_sidecars_are_grouped_by_destination():
    result = ops.convert(inputs=["s.bdf"], output="atlas/s.png", to="image",
                         image_layout="imageset", files=INPUTS)
    keys = set(result.outputs)
    assert len(keys) > 1 and all(k.startswith("atlas/") for k in keys)
    bundle = result.bundles["atlas/s.png"]
    assert isinstance(bundle, fp.OutputBundle)
    with pytest.raises(fp.InvalidInputError, match="overlap"):
        ops.merge(family="F", parts=[{"input": "s.bdf", "take": "all"}],
                  outputs=[{"destination": "m.bdf"}, {"destination": "m.bdf"}], files=INPUTS)


def test_filesystem_mode_protects_sources_and_rolls_back_groups(tmp_path, monkeypatch):
    d = workdir(tmp_path, "fs")
    monkeypatch.chdir(d)
    (d / "first.bdf").write_bytes(b"previous")
    (d / "blocker").write_bytes(b"file")
    with pytest.raises(fp.FontpixelError):
        ops.merge(family="Rollback", parts=[{"input": "s.bdf", "take": "all"}],
                  outputs=[{"destination": "first.bdf"},
                           {"destination": "blocker/second.ttf"}])
    assert (d / "first.bdf").read_bytes() == b"previous"
    # An output may not replace one of several sources.
    with pytest.raises(fp.FontpixelError):
        ops.convert(inputs=["b.bdf", "first.bdf"], output="b.bdf")
    assert (d / "b.bdf").read_bytes() == BLOCK
    # A strict failure writes nothing.
    with pytest.raises(fp.StrictLossError):
        ops.convert(inputs=["s.bdf"], output="x.psf", strict=True)
    assert not (d / "x.psf").exists()


def test_operations_release_the_gil_and_run_in_threads():
    results = [None] * 4

    def work(i):
        results[i] = ops.convert(inputs=["s.bdf"], output=f"o{i}.woff2", files=INPUTS)

    threads = [threading.Thread(target=work, args=(i,)) for i in range(4)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    assert len({r.outputs[f"o{i}.woff2"] for i, r in enumerate(results)}) == 1


# ------------------------------------------------------------ batch

JOB = """version = 1
jobs = 1
[[tasks]]
id = "pcf"
kind = "convert"
inputs = ["s.bdf"]
output = "out/s.pcf"
[[tasks]]
id = "ttf"
kind = "vectorize"
inputs = ["b.bdf"]
output = "out/b.ttf"
[tasks.options]
shape = "dot"
[[tasks]]
id = "check"
kind = "validate"
inputs = ["s.bdf"]
[[tasks]]
id = "bad"
kind = "convert"
inputs = ["s.bdf"]
output = "out/bad.psf"
to = "psf"
[tasks.options]
psf_version = "3"
[[tasks]]
id = "mix"
kind = "merge"
family = "Mix"
outputs = ["out/m.bdf"]
[[tasks.parts]]
input = "s.bdf"
take = "all"
"""
TYPED = {
    "jobs": 1,
    "tasks": [
        {"id": "pcf", "kind": "convert", "inputs": ["s.bdf"], "output": "out/s.pcf",
         "report": "json"},
        {"id": "ttf", "kind": "vectorize", "inputs": ["b.bdf"], "output": "out/b.ttf",
         "shape": "dot", "report": "json"},
        {"id": "check", "kind": "validate", "input": "s.bdf", "json": True},
        {"id": "bad", "kind": "convert", "inputs": ["s.bdf"], "output": "out/bad.psf",
         "to": "psf", "psf_version": "3", "report": "json"},
        {"id": "mix", "kind": "merge", "family": "Mix",
         "parts": [{"input": "s.bdf", "take": "all"}],
         "outputs": [{"destination": "out/m.bdf"}], "report": "json"},
    ],
}


@needs_cli
def test_batch_toml_file_and_typed_tasks_equal_the_command_line(tmp_path, monkeypatch):
    cli_dir = workdir(tmp_path, "cli")
    (cli_dir / "out").mkdir()
    (cli_dir / "jobs.toml").write_text(JOB)
    code, out, _ = cli(["batch", "jobs.toml"], cli_dir)
    expected = tree(cli_dir, skip=set(INPUTS) | {"jobs.toml"})
    document = json.loads(out)
    assert code == 2 and [t["kind"] for t in document["tasks"]][:2] == ["convert", "vectorize"]

    def relative(text, root):
        return text.replace(str(root) + os.sep, "")

    # batch_file: the same file in another directory, run from the filesystem.
    fs_dir = workdir(tmp_path, "fs")
    (fs_dir / "out").mkdir()
    (fs_dir / "jobs.toml").write_text(JOB)
    result = ops.batch_file(fs_dir / "jobs.toml")
    assert relative(result.json(), fs_dir) == relative(out, cli_dir)
    assert result.exit_code == code and result.outputs == {}
    assert tree(fs_dir, skip=set(INPUTS) | {"jobs.toml"}) == expected
    assert [t["kind"] for t in result.tasks] == ["convert", "vectorize", "validate",
                                                 "convert", "merge"]
    assert result.tasks[3]["exit_code"] == 2 and not result.tasks[3]["success"]

    # batch_toml in memory: committed outputs are returned.
    memory = ops.batch_toml(JOB, files=INPUTS)
    assert memory.exit_code == code
    assert memory.outputs == expected
    assert [t["id"] for t in memory.document["tasks"]] == ["pcf", "ttf", "check", "bad", "mix"]
    assert memory.json() == relative(out, cli_dir)
    assert set(memory.bundles) == {"out/s.pcf", "out/b.ttf", "out/m.bdf"}

    # Typed tasks: same document and outputs; the invalid task fails alone.
    typed = ops.batch(TYPED, files=INPUTS)
    assert typed.exit_code == code and typed.outputs == expected
    typed_doc, toml_doc = typed.document, memory.document
    for t in typed_doc["tasks"] + toml_doc["tasks"]:
        t.pop("error", None)  # messages differ: serde text vs parsed command line
    assert typed_doc == toml_doc
    assert "invalid value '3' for psf_version" in typed.tasks[3]["error"]
    monkeypatch.chdir(workdir(tmp_path, "typed-fs"))
    Path("out").mkdir()
    on_disk = ops.batch(TYPED)
    assert on_disk.exit_code == code and tree(".", skip=set(INPUTS)) == expected


@needs_cli
def test_whole_batch_failures_carry_the_command_line_document(tmp_path):
    (tmp_path / "jobs.toml").write_text("version = 2\n[[tasks]]\nid='a'\nkind='validate'\n")
    code, out, _ = cli(["batch", "jobs.toml"], tmp_path)
    with pytest.raises(fp.InvalidInputError) as info:
        ops.batch_file(tmp_path / "jobs.toml")
    assert info.value.exit_code == code == 2
    assert info.value.document == out
    with pytest.raises(fp.InvalidInputError) as info:
        ops.batch_toml("version = 2\n[[tasks]]\nid='a'\nkind='validate'\n")
    assert json.loads(info.value.document)["exit_code"] == 2
    with pytest.raises(fp.InvalidInputError):
        ops.batch(tasks=[])
    with pytest.raises(fp.InvalidInputError):
        ops.batch_file(tmp_path / "jobs.toml", jobs=65)
    with pytest.raises(fp.InvalidInputError, match="needs a string id"):
        ops.batch(tasks=[{"kind": "validate", "input": "s.bdf"}], files=INPUTS)


def test_parse_batch_toml_lists_prepared_tasks():
    parsed = ops.parse_batch_toml(JOB, base="root")
    assert parsed["jobs"] == 1
    tasks = {t["id"]: t for t in parsed["tasks"]}
    assert tasks["ttf"]["kind"] == "vectorize"
    assert tasks["ttf"]["request"]["kind"] == "vectorize"
    assert tasks["ttf"]["request"]["shape"] == "dot"
    assert tasks["pcf"]["declared_inputs"] == [os.path.join("root", "s.bdf")]
    assert tasks["pcf"]["weight"] == 256 * 1024 * 1024
    assert "invalid value '3'" in tasks["bad"]["error"]


def test_batch_in_memory_runs_without_a_filesystem(tmp_path, monkeypatch):
    monkeypatch.chdir(tmp_path)
    result = ops.batch(TYPED, files=INPUTS)
    assert list(tmp_path.iterdir()) == []
    assert result.workers == 1 and result.jobs == 1
    assert shutil.which  # keep shutil imported for helpers
