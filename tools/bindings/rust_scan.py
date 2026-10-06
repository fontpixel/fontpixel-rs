"""Scan the pyo3 binding sources (crates/fontpixel-py/src) for Python callables.

A small item-level parser (not a Rust compiler): it finds `#[pyclass]`
structs, `#[pymethods]` impl blocks and `#[pyfunction]` functions, and records
for each function its Python name and kind (from `#[pyo3(name = ...)]`,
`#[getter]`, `#[setter]`, `#[new]`, `#[staticmethod]`, `#[classmethod]`), its
doc comment, the Rust types of its parameters and return value, and *hints*
for `&Bound<PyAny>` parameters: the Rust type a parameter is converted to in
the body (`let t: name::NameTable = req(table)?`) or the `*_arg` helper it is
passed to (`bytes_arg(data)`).

Functions generated inside `macro_rules!` are not seen here; gen_pyi.py types
them through pattern rules in pyi_overrides.py.
"""
from __future__ import annotations

import re
from dataclasses import dataclass, field
from pathlib import Path


@dataclass
class RustFn:
    file: str
    line: int
    rust_name: str
    py_name: str
    kind: str                    # function | method | getter | setter | new | static | classmethod
    cls: str | None              # Rust struct name of the impl block
    doc: str
    params: list                 # [(name, rust type text)]
    ret: str
    hints: dict = field(default_factory=dict)   # param -> ("type"|"helper", rust text, file)
    ret_hint: tuple | None = None
    bytes_fields: list | None = None   # result fields returned as bytes (with_bytes)


@dataclass
class RustClass:
    rust_name: str
    py_name: str
    module: str | None
    fields: list = field(default_factory=list)   # [(py name, rust type, doc, writable)]
    options: set = field(default_factory=set)    # bare #[pyclass] options: frozen, eq, ord, ...
    file: str = ""


class Lexer:
    """Character-level helpers that skip strings, chars and comments."""

    def __init__(self, text):
        self.t = text

    def skip_ws(self, i):
        t = self.t
        while i < len(t):
            if t[i].isspace():
                i += 1
            elif t.startswith("//", i) and not t.startswith("///", i) and not t.startswith("//!", i):
                i = t.find("\n", i)
                i = len(t) if i < 0 else i
            elif t.startswith("/*", i):
                i = t.find("*/", i) + 2
            else:
                break
        return i

    def skip_literal(self, i):
        """If a string/char literal starts at i, return its end; else None."""
        t = self.t
        m = re.match(r'(b?r)(#*)"', t[i:i + 10])
        if m and (i == 0 or not (t[i - 1].isalnum() or t[i - 1] == "_")):
            hashes = m.group(2)
            end = t.find('"' + hashes, i + len(m.group(0)))
            return end + 1 + len(hashes)
        if t[i] == '"' or (t[i] == "b" and t[i + 1:i + 2] == '"' and not (t[i - 1].isalnum() or t[i - 1] == "_")):
            j = i + (2 if t[i] == "b" else 1)
            while t[j] != '"':
                j += 2 if t[j] == "\\" else 1
            return j + 1
        if t[i] == "'":
            m = re.match(r"'(\\u\{[0-9a-fA-F]+\}|\\x[0-9a-fA-F]{2}|\\.|[^\\'])'", t[i:i + 14])
            if m:
                return i + len(m.group(0))
        return None

    def match(self, i, open_ch, close_ch):
        """Index just past the bracket matching t[i] == open_ch."""
        t = self.t
        depth = 0
        while i < len(t):
            end = self.skip_literal(i)
            if end is not None:
                i = end
                continue
            if t.startswith("//", i):
                i = t.find("\n", i)
                continue
            if t.startswith("/*", i):
                i = t.find("*/", i) + 2
                continue
            c = t[i]
            if c == open_ch:
                depth += 1
            elif c == close_ch:
                depth -= 1
                if depth == 0:
                    return i + 1
            i += 1
        raise ValueError("unbalanced")

    def item_end(self, i):
        """End of a non-function item starting at i (`;` or a braced body)."""
        t = self.t
        depth = 0
        while i < len(t):
            end = self.skip_literal(i)
            if end is not None:
                i = end
                continue
            c = t[i]
            if c in "([":
                i = self.match(i, c, ")" if c == "(" else "]")
                continue
            if c == "{":
                return self.match(i, "{", "}")
            if c == ";" and depth == 0:
                return i + 1
            i += 1
        return i


def split_top(text, sep=","):
    out, depth, cur = [], 0, ""
    for ch in text:
        if ch in "<([{":
            depth += 1
        elif ch in ">)]}":
            depth -= 1
        if ch == sep and depth == 0:
            out.append(cur.strip())
            cur = ""
            continue
        cur += ch
    if cur.strip():
        out.append(cur.strip())
    return out


USES: dict = {}   # file -> {local name: full path}


def parse_uses(text):
    """`use` declarations of a file as {local name: full path}."""
    out = {}

    def expand(prefix, tree):
        tree = tree.strip()
        if tree.endswith("}") and "{" in tree:
            head, _, inner = tree.partition("{")
            base = prefix + [s for s in head.strip().rstrip(":").split("::") if s]
            for part in split_top(inner[:-1]):
                expand(base, part)
            return
        m = re.fullmatch(r"([\w:]+?)(?:\s+as\s+(\w+))?", tree)
        if not m:
            return
        segs = prefix + [s for s in m.group(1).split("::") if s]
        if segs[-1] == "self":
            segs = segs[:-1]
        if segs and segs[-1] != "*":
            out[m.group(2) or segs[-1]] = "::".join(segs)

    for m in re.finditer(r"^\s*(?:pub(?:\([^)]*\))?\s+)?use\s+([^;]+);", text, re.M):
        expand([], " ".join(m.group(1).split()))
    return out


def scan_file(path: Path, rel: str):
    text = path.read_text()
    USES[rel] = parse_uses(text)
    lx = Lexer(text)
    fns, classes = [], []
    _scan_region(lx, 0, len(text), None, fns, classes, rel, text)
    return fns, classes


def _scan_region(lx, start, end, impl_cls, fns, classes, rel, text):
    i = start
    docs, attrs = [], []
    while True:
        i = lx.skip_ws(i)
        if i >= end:
            return
        if text.startswith("///", i) or text.startswith("//!", i):
            nl = text.find("\n", i)
            nl = end if nl < 0 else nl
            if text.startswith("///", i):
                line = text[i + 3:nl]
                docs.append(line[1:] if line.startswith(" ") else line)
            i = nl + 1
            continue
        if text.startswith("#[", i) or text.startswith("#![", i):
            j = text.index("[", i)
            k = lx.match(j, "[", "]")
            attrs.append(text[j + 1:k - 1].strip())
            i = k
            continue
        m = re.match(r"(?:pub(?:\([^)]*\))?\s+)?(?:const\s+|async\s+|unsafe\s+)*fn\s+(\w+)", text[i:i + 200])
        if m:
            i = _parse_fn(lx, i, m, impl_cls, docs, attrs, fns, rel, text)
            docs, attrs = [], []
            continue
        m = re.match(r"(?:pub(?:\([^)]*\))?\s+)?struct\s+(\w+)", text[i:i + 200])
        if m and any(a.startswith("pyclass") for a in attrs):
            cls = _pyclass(m.group(1), next(a for a in attrs if a.startswith("pyclass")))
            cls.file = rel
            k = lx.item_end(i)
            body = text[i:k]
            if "{" in body:
                _class_fields(body[body.index("{") + 1:body.rindex("}")], cls)
            classes.append(cls)
            i = k
            docs, attrs = [], []
            continue
        m = re.match(r"impl\s*(?:<[^>]*>\s*)?(\w+)\s*\{", text[i:i + 200])
        if m and "pymethods" in attrs:
            j = text.index("{", i)
            k = lx.match(j, "{", "}")
            _scan_region(lx, j + 1, k - 1, m.group(1), fns, classes, rel, text)
            i = k
            docs, attrs = [], []
            continue
        m = re.match(r"(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*\{", text[i:i + 200])
        if m:
            j = text.index("{", i)
            k = lx.match(j, "{", "}")
            _scan_region(lx, j + 1, k - 1, None, fns, classes, rel, text)
            i = k
            docs, attrs = [], []
            continue
        if text.startswith("macro_rules!", i):
            j = i + len("macro_rules!")
            while text[j] not in "{([":
                j += 1
            close = {"{": "}", "(": ")", "[": "]"}[text[j]]
            i = lx.match(j, text[j], close)
            docs, attrs = [], []
            continue
        i = lx.item_end(i)
        docs, attrs = [], []


def _pyclass(rust_name, attr):
    name = re.search(r'name\s*=\s*"([^"]+)"', attr)
    module = re.search(r'module\s*=\s*"([^"]+)"', attr)
    rc = RustClass(rust_name, name.group(1) if name else rust_name, module.group(1) if module else None)
    inner = attr[attr.find("(") + 1:attr.rfind(")")] if "(" in attr else ""
    rc.options = {o.strip() for o in split_top(inner) if re.fullmatch(r"\w+", o.strip())}
    return rc


def _class_fields(body, cls):
    """`#[pyo3(get)]`/`#[pyo3(get, set)]` fields of a pyclass struct."""
    for m in re.finditer(r"((?:\s*///[^\n]*\n)*)\s*#\[pyo3\(([^)]*)\)\]\s*(?:pub\s+)?(\w+)\s*:\s*([^,\n]+),?", body):
        docs = " ".join(line.strip()[3:].strip() for line in m.group(1).strip().splitlines())
        opts = [o.strip() for o in m.group(2).split(",")]
        if "get" in opts:
            cls.fields.append((m.group(3), m.group(4).strip(), docs, "set" in opts))


def _parse_fn(lx, i, m, impl_cls, docs, attrs, fns, rel, text):
    name = m.group(1)
    line = text.count("\n", 0, i) + 1
    j = i + m.end()
    j = lx.skip_ws(j)
    if text[j] == "<":
        j = _match_angle(text, j)
    j = lx.skip_ws(j)
    assert text[j] == "(", (rel, name, text[j:j + 40])
    k = lx.match(j, "(", ")")
    params_text = text[j + 1:k - 1]
    rest_start = k
    # return type up to `where` or `{` (or `;` for declarations)
    depth = 0
    p = rest_start
    while p < len(text):
        c = text[p]
        if c in "<([":
            depth += 1
        elif c in ">)]":
            depth -= 1 if not (c == ">" and text[p - 1] == "-") else 0
        if depth == 0 and (c == "{" or c == ";" or text.startswith("where", p)):
            break
        p += 1
    sig_tail = text[rest_start:p].strip()
    ret = sig_tail[2:].strip() if sig_tail.startswith("->") else "()"
    if text[p] == ";":
        body = ""
        end = p + 1
    else:
        b = text.index("{", p)
        end = lx.match(b, "{", "}")
        body = text[b:end]
    is_py = impl_cls is not None or "pyfunction" in attrs
    if not is_py and "$" not in name:
        HELPER_RETURNS.setdefault(rel, {})[name] = " ".join(ret.split())
        fields = bytes_fields(body, rel)
        if fields:
            BYTES_HELPERS.setdefault(rel, {})[name] = fields
    if not is_py or "$" in name:
        return end
    kind = "function" if impl_cls is None else "method"
    py_name = name
    for a in attrs:
        if a == "new":
            kind = "new"
        elif a == "staticmethod":
            kind = "static"
        elif a == "classmethod":
            kind = "classmethod"
        elif a.startswith("getter"):
            kind = "getter"
            g = re.fullmatch(r"getter\s*\(\s*(\w+)\s*\)", a)
            py_name = g.group(1) if g else name.removeprefix("get_")
        elif a.startswith("setter"):
            kind = "setter"
            g = re.fullmatch(r"setter\s*\(\s*(\w+)\s*\)", a)
            py_name = g.group(1) if g else name.removeprefix("set_")
        elif a.startswith("pyo3"):
            n = re.search(r'\bname\s*=\s*"([^"]+)"', a)
            if n:
                py_name = n.group(1)
    if kind == "new":
        py_name = "__new__"
    params = []
    for part in split_top(params_text):
        if not part or re.fullmatch(r"(?:&\s*(?:'\w+\s+)?)?(?:mut\s+)?self", part):
            continue
        pm =re.match(r"(?:mut\s+)?(\w+)\s*:\s*(.+)$", part, re.S)
        if not pm:
            continue
        pname, ptype = pm.group(1), " ".join(pm.group(2).split())
        if pname in ("self", "slf") or re.match(r"Python\s*<", ptype) or ptype.startswith("&Bound<'_, PyType>") or "PyType>" in ptype and pname == "cls":
            continue
        params.append((pname, ptype))
    hints = {k: (v[0], v[1], rel) for k, v in _hints(body, [p for p, _ in params]).items()}
    fn = RustFn(rel, line, name, py_name, kind, impl_cls, "\n".join(docs).strip(), params,
                " ".join(ret.split()), hints)
    fn.ret_hint = _ret_hint(body, rel)
    fn.bytes_fields = bytes_fields(body, rel)
    fns.append(fn)
    return end


WITH_BYTES = re.compile(r"with_bytes\(\s*py\s*,\s*&?\s*\w+\s*,\s*&\[(.*?)\]\s*\)", re.S)


def bytes_fields(body, rel):
    """Fields a body returns as `bytes` through `with_bytes` (directly or via a
    helper such as `encoded_cmap`), or None."""
    tail = body.rstrip().rstrip("}").rstrip()
    last = tail[tail.rfind(";") + 1:] if ";" in tail else tail.lstrip("{")
    m = WITH_BYTES.search(last)
    if m:
        return re.findall(r'\(\s*"(\w+)"', m.group(1))
    h = re.match(r"\s*(?:Ok\(\s*)?(?:[\w.]+\(\|\w+\|\s*)?(\w+)\(\s*py\s*,", last)
    if h and h.group(1) in BYTES_HELPERS.get(rel, {}):
        return BYTES_HELPERS[rel][h.group(1)]
    return None


def _ret_hint(body, rel):
    """What a `PyAny` result is built from, judged by the body's final expression:
    ("bytes",), ("type", rust type, file) or ("call", rust path, file)."""
    tail = body.rstrip().rstrip("}").rstrip()
    last = tail[tail.rfind(";") + 1:] if ";" in tail else tail.lstrip("{")
    last = last.strip()
    if re.match(r"(?:Ok\(\s*)?pybytes\(", last):
        return ("bytes",)
    conv = r"(?:Ok\(\s*)?(?:[\w.]+\(\|\w+\|\s*)?(?:conv::)?(?:out|to_py|with_bytes|encoded_cmap|tables_dict)\(\s*py\s*,\s*&\s*(?:py\.detach\(\|\|\s*)?"
    m = re.match(conv + r"([\w:]+?)::default\(\)", last)
    if m:
        return ("type", m.group(1), rel)
    m = re.match(conv + r"([\w:]+?)(?:::<[^>]*>)?\s*\(", last)
    if m and "::" in m.group(1):
        return ("call", m.group(1), rel)
    m = re.match(conv + r"(\w+)\s*[,)]", last)
    if m:
        var = m.group(1)
        t = re.search(rf"let\s+(?:mut\s+)?{var}\s*:\s*([\w:<>, ]+?)\s*=", body)
        if t:
            return ("type", t.group(1), rel)
        c = re.search(rf"let\s+(?:mut\s+)?{var}\s*=\s*(?:py\.detach\(\|\|\s*)?([\w:]+?)(?:::<[^>]*>)?\s*\(", body)
        if c and "::" in c.group(1):
            return ("call", c.group(1), rel)
        # `let e = t.encode(..)` with `let t: Type = ..`: the method `Type::encode`.
        c = re.search(rf"let\s+(?:mut\s+)?{var}\s*=\s*(?:py\.detach\(\|\|\s*)?(\w+)\.(\w+)\s*\(", body)
        if c:
            t = re.search(rf"let\s+(?:mut\s+)?{c.group(1)}\s*:\s*([\w:]+)\s*=", body)
            if t:
                return ("call", f"{t.group(1)}::{c.group(2)}", rel)
    return None


def _match_angle(text, j):
    depth = 0
    while j < len(text):
        if text[j] == "<":
            depth += 1
        elif text[j] == ">" and text[j - 1] != "-":
            depth -= 1
            if depth == 0:
                return j + 1
        j += 1
    return j


HELPER_CALL = re.compile(
    r"let\s+(?:mut\s+)?\w+\s*:\s*([\w:<>, &']+?)\s*=\s*(?:[\w:]+::)?(\w+)\s*(?:::<[^>]*>)?\s*\(\s*(?:py\s*,\s*)?(?:Some\s*\()?\s*&?\s*(\w+)")
TURBO_CALL = re.compile(r"\b(?:[\w:]+::)?(\w+)\s*::<\s*([\w:<>, ]+?)\s*>\s*\(\s*(?:Some\s*\()?\s*&?\s*(\w+)")
PLAIN_CALL = re.compile(
    r"\b(\w+_arg|\w+_args|InputArgs::new|SetArg::extract|with_font)\s*\(\s*(?:Some\s*\()?\s*&?\s*(\w+)")
# Generic serde conversions whose target type names the Python shape.
SERDE_HELPERS = {"req", "opt", "from_py", "from_py_required", "enum_arg", "depythonize", "parse", "opt_or_default", "limits_or_default", "build"}


OPTS_CALL = re.compile(r"\b(?:read_bytes_opts|write_x_opts|read_ctx_opts|write_x_primary_opts|"
                       r"read_bytes_opts_limits|write_set_wo_opts)\s*::<([^>]*)>\s*\(([^;]*)")


def _hints(body, params):
    hints = {}
    # Generic codec adapters: the last turbofish type is the options type.
    for m in OPTS_CALL.finditer(body):
        opts = split_top(m.group(1))[-1].strip()
        args = re.findall(r"\b\w+\b", m.group(2))
        for p in params:
            if p in args and p == "options" and opts != "_":
                hints.setdefault(p, ("type", opts))
    for m in HELPER_CALL.finditer(body):
        ty, helper, arg = m.groups()
        if arg in params and arg not in hints and helper in SERDE_HELPERS:
            hints[arg] = ("type", ty.strip())
        elif arg in params and arg not in hints and helper.endswith("_arg"):
            hints[arg] = ("helper", helper)
    for m in TURBO_CALL.finditer(body):
        helper, ty, arg = m.groups()
        if arg in params and arg not in hints and helper in SERDE_HELPERS:
            hints[arg] = ("type", ty.strip())
    for m in PLAIN_CALL.finditer(body):
        helper, arg = m.groups()
        if arg in params and arg not in hints:
            hints[arg] = ("helper", helper)
    return hints


HELPER_RETURNS: dict = {}   # file -> {non-Python fn name: Rust return type}
BYTES_HELPERS: dict = {}    # file -> {helper fn name: fields it returns as bytes}


def scan(src: Path):
    HELPER_RETURNS.clear()
    BYTES_HELPERS.clear()
    USES.clear()
    fns, classes = [], []
    for p in sorted(src.glob("*.rs")):
        f, c = scan_file(p, p.name)
        fns += f
        classes += c
    return fns, classes


def format_namespaces(src: Path):
    """The `namespaces!` table of formats_direct.rs: module -> arms.

    Each arm is (kind, [arguments]) as written, e.g.
    ("ctx_opts", ["f::fzx::read", "f::fzx::FzxReadOptions"]).
    """
    text = (src / "formats_direct.rs").read_text()
    start = text.index("namespaces! {")
    out = {}
    pat = re.compile(r"^\s*(\w+)\s*\[([^\]]*)\]\s*\{\s*detect:\s*\((.*?)\),\s*read:\s*\((.*?)\),"
                     r"\s*write:\s*\((.*?)\)\s*\}", re.M)
    for m in pat.finditer(text[start:]):
        arms = {}
        for key, arm in zip(("detect", "read", "write"), m.group(3, 4, 5)):
            kind, _, rest = arm.strip().partition(" ")
            arms[key] = (kind, split_top(rest) if rest else [])
        ids = [i.strip().strip('"') for i in m.group(2).split(",") if i.strip()]
        out[m.group(1)] = {"ids": ids, **arms}
    return out


def ops_operations(src: Path):
    """`writer_op!`/`reader_op!` invocations of ops.rs: {operation: request type}."""
    text = (src / "ops.rs").read_text()
    out = {}
    for m in re.finditer(r"^(?:writer_op|reader_op)!\(\s*((?:\s*///[^\n]*\n)*)\s*(\w+),\s*([\w:]+)", text, re.M):
        out[m.group(2)] = m.group(3)
    return out


def registry_setters(src: Path):
    """`setters!` tables of formats_options.rs: {"read"|"write": {format id: Rust type}}."""
    text = (src / "formats_options.rs").read_text()
    body = text[text.index("setters! {"):]
    out = {"read": {}, "write": {}}
    section = None
    for line in body.splitlines()[1:]:
        s = line.strip()
        if s.startswith("read {"):
            section = "read"
            continue
        if s.startswith("write {") or s.startswith("checked_write {"):
            section = "write"
            continue
        if s.startswith("}"):
            if section is None:
                break
            section = None
            continue
        m = re.match(r'((?:"[^"]+"\s*\|?\s*)+)=>\s*\w+\s*:\s*([\w:]+)', s)
        if m and section:
            for fid in re.findall(r'"([^"]+)"', m.group(1)):
                out[section][fid] = m.group(2)
    return out
