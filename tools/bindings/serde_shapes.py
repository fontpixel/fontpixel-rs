"""Python typing shapes of the serde types of `fontpixel` and `fontpixel-ops`.

Reads nightly rustdoc JSON (the files `tools/bindings/gate.sh` builds) and
derives, for every public struct/enum that implements serde, the Python value
it crosses the binding boundary as:

* unit-only enums -> ``Literal[...]`` of the serde variant names
  (``rename_all``/``rename`` applied; ``alias`` names are input-only and listed
  in the docstring instead);
* structs -> ``TypedDict`` (keys optional when the container or field has
  ``default`` or the field is an ``Option``; ``flatten`` groups merged);
* data-carrying enums -> unions of single-key ``TypedDict``s (externally
  tagged) or of tagged ``TypedDict``s (``tag = "..."``);
* newtypes -> their inner type.

Used by gen_pyi.py, which renders the result as ``fontpixel/types.py``.
"""
from __future__ import annotations

import json
import keyword
import re
from dataclasses import dataclass, field
from pathlib import Path

# Hand-written shapes for types with manual serde impls (their derived proxy
# structs are private, so rustdoc cannot show them).
MANUAL = {
    # bitmap/mod.rs serde_impl::{RawRef, Raw}
    "fontpixel::bitmap::Bitmap": ("struct", [
        ("width", "int", True, "Width in pixels."),
        ("height", "int", True, "Height in pixels."),
        ("data", "SerdeBytes", True,
         "Canonical storage: top-down rows, Mono packed MSB-first and padded to whole bytes, Gray8 one byte per pixel."),
    ]),
    # model/mod.rs serde_impl::{FontRef, FontRaw}
    "fontpixel::model::Font": ("struct", [
        ("glyphs", "list[GlyphDict | None]", True, "Glyph slots by stable ID; removed slots are None."),
        ("metrics", "FontMetrics", True, "Font-wide metrics."),
        ("names", "FontNames", True, "Naming fields."),
        ("charset", "FontCharset", True, "Declared charset."),
        ("source_format", "str | None", False, "Format ID the font was read from."),
        ("headers", "PropertiesDict", True, "Format header properties."),
        ("properties", "PropertiesDict", True, "Font properties."),
    ]),
    # Newtypes with private fields (derived serde = the inner value).
    "fontpixel::model::GlyphId": ("alias", "int"),
    "fontpixel::model::properties::Properties": ("alias", "list[Property]"),
}
# Plain-data shapes of types that also have a handle class keep the handle's
# name free: `fontpixel.Font` is the handle, `fontpixel.types.FontDict` its dict.
FIXED_NAMES = {
    "fontpixel::model::Font": "FontDict",
    "fontpixel::model::FontSet": "FontSetDict",
    "fontpixel::model::Glyph": "GlyphDict",
    "fontpixel::model::Codepoint": "CodepointDict",
    "fontpixel::model::properties::Properties": "PropertiesDict",
    "fontpixel::bitmap::Bitmap": "BitmapDict",
    "fontpixel::resources::OutputBundle": "OutputBundleDict",
    "fontpixel::resources::OutputAsset": "OutputAssetDict",
    "fontpixel::resources::Written": "WrittenDict",
}
# `#[serde(with = ...)]` helpers and their Python shapes.
WITH = {
    "crate::sfnt::serde_tag::tag": "str",
    "crate::sfnt::serde_tag::list": "list[str]",
    "crate::sfnt::serde_tag::set": "list[str]",
    "crate::sfnt::serde_tag::map": "dict[str, {V}]",
}
INT = {"u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize"}
STR_PATHS = {"String", "str", "Cow", "PathBuf", "Path", "OsString", "OsStr", "Box<str>"}
SEQ = {"Vec", "VecDeque", "BTreeSet", "HashSet", "SmallVec", "BinaryHeap"}
MAP = {"BTreeMap", "HashMap", "IndexMap"}
WRAP = {"Box", "Arc", "Rc", "Cow", "RefCell", "Cell"}


def serde_attrs(item):
    """The comma-separated parts of every `#[serde(...)]` attribute."""
    parts = []
    for a in item.get("attrs", []):
        text = a.get("other", "") if isinstance(a, dict) else str(a)
        for m in re.finditer(r"#\[serde\((.*)\)\]", text, re.S):
            parts += split_top(m.group(1))
    return parts


def split_top(text):
    out, depth, cur, quote = [], 0, "", False
    for ch in text:
        if ch == '"':
            quote = not quote
        if not quote:
            if ch in "([{":
                depth += 1
            elif ch in ")]}":
                depth -= 1
            elif ch == "," and depth == 0:
                out.append(cur.strip())
                cur = ""
                continue
        cur += ch
    if cur.strip():
        out.append(cur.strip())
    return out


def attr_value(parts, key):
    vals = []
    for p in parts:
        m = re.fullmatch(rf'{key}\s*=\s*"([^"]*)"', p)
        if m:
            vals.append(m.group(1))
    return vals


def has_flag(parts, key):
    return any(p == key or p.startswith(key + " ") or p.startswith(key + "=") for p in parts)


def rename(name, rule):
    if not rule:
        return name
    words = re.findall(r"[A-Z]?[a-z0-9]+|[A-Z]+(?![a-z])", name) if "_" not in name else name.split("_")
    words = [w for w in words if w]
    if rule == "snake_case":
        return "_".join(w.lower() for w in words)
    if rule == "kebab-case":
        return "-".join(w.lower() for w in words)
    if rule == "lowercase":
        return name.lower()
    if rule == "UPPERCASE":
        return name.upper()
    if rule == "SCREAMING_SNAKE_CASE":
        return "_".join(w.upper() for w in words)
    if rule == "camelCase":
        s = "".join(w.capitalize() for w in words)
        return s[:1].lower() + s[1:]
    if rule == "PascalCase":
        return "".join(w.capitalize() for w in words)
    raise ValueError(f"unsupported rename_all rule {rule}")


@dataclass
class Field:
    key: str
    type: str
    required: bool
    doc: str


@dataclass
class Shape:
    """One generated Python type."""
    path: str              # canonical Rust path
    kind: str              # literal | struct | union | alias
    doc: str = ""
    values: list = field(default_factory=list)   # literal values
    fields: list = field(default_factory=list)   # struct fields
    members: list = field(default_factory=list)  # union member type expressions
    alias: str = ""
    extra: list = field(default_factory=list)    # auxiliary shapes (variant dicts)
    name: str = ""


class SerdeModel:
    def __init__(self, doc_dir: Path, reserved=(), class_names=()):
        # Names the generated module defines otherwise (binding classes,
        # shared aliases, typing imports): serde types move off them.
        self.reserved = set(reserved) | set(class_names)
        self.class_names = set(class_names)
        self.docs = []
        for crate in ("fontpixel", "fontpixel_ops"):
            p = doc_dir / f"{crate}.json"
            if p.exists():
                self.docs.append(json.loads(p.read_text()))
        if not self.docs:
            raise SystemExit(f"no rustdoc JSON in {doc_dir}; run tools/bindings/gate.sh first")
        self.items = {}     # (doc_no, id) -> item
        self.path_of = {}   # (doc_no, id) -> canonical path
        self.by_path = {}   # canonical path -> (doc_no, id)
        for n, d in enumerate(self.docs):
            ext = {str(k): v for k, v in d.get("external_crates", {}).items()}
            for k, v in d["paths"].items():
                if v["crate_id"] != 0:
                    continue
                path = "::".join(v["path"])
                self.path_of[(n, k)] = path
            for k, it in d["index"].items():
                if it.get("crate_id") == 0:
                    self.items[(n, k)] = it
            self.ext = ext
        for key, path in self.path_of.items():
            it = self.items.get(key)
            if it and ("struct" in it["inner"] or "enum" in it["inner"]):
                self.by_path.setdefault(path, key)
        self.serde = {}
        for key, it in self.items.items():
            inner = it["inner"]
            kind = "struct" if "struct" in inner else "enum" if "enum" in inner else None
            if not kind or key not in self.path_of:
                continue
            traits = set()
            for impl_id in inner[kind].get("impls", []):
                impl = self.items.get((key[0], str(impl_id)))
                if not impl:
                    continue
                tr = impl["inner"]["impl"].get("trait")
                if tr:
                    traits.add(tr["path"].split("::")[-1])
            if "Serialize" in traits or "Deserialize" in traits:
                self.serde[self.path_of[key]] = key
        self._index_public()
        self.shapes = {}
        self.names = {}
        missing =[p for p in list(FIXED_NAMES) + list(MANUAL) if p not in self.serde]
        if missing:
            raise SystemExit(f"serde_shapes: no serde type at {missing}")
        self._assign_names()
        for path in sorted(self.serde):
            self.shapes[path] = self._shape(path)

    # ---------------------------------------------------------------- naming
    def _assign_names(self):
        paths = sorted(self.serde) + sorted(p for p in MANUAL if p not in self.serde)
        for p in MANUAL:
            if p not in self.serde:
                self.serde[p] = None
        depth = {p: 1 for p in paths}
        # The plain-data shape of a binding class `X` is `XDict`.
        fixed = dict(FIXED_NAMES)
        for cls in sorted(self.class_names):
            cands = [p for p in paths if p.split("::")[-1] == cls and p not in fixed]
            if len(cands) == 1 and cls + "Dict" not in fixed.values():
                fixed[cands[0]] = cls + "Dict"
        while True:
            names = {}
            for p in paths:
                if p in fixed:
                    names.setdefault(fixed[p], []).append(p)
                    continue
                segs = [s for s in p.split("::")[1:]]
                tail = segs[-depth[p]:]
                name = "".join(camel(s) for s in tail[:-1]) + tail[-1]
                names.setdefault(name, []).append(p)
            taken = [p for name, ps in names.items() if name in self.reserved
                     for p in ps if p not in fixed]
            for p in taken:
                if depth[p] >= len(p.split("::")) - 1:
                    raise SystemExit(f"cannot name serde type {p} apart from a reserved name")
                depth[p] += 1
            clash = [ps for ps in names.values() if len(ps) > 1]
            if not clash and not taken:
                break
            for ps in clash:
                # The one entry already at the shortest depth keeps its name.
                low = min(depth[p] for p in ps)
                keep = [p for p in ps if depth[p] == low and p not in fixed]
                for p in ps:
                    if p in fixed or (len(keep) == 1 and p == keep[0]):
                        continue
                    if depth[p] >= len(p.split("::")) - 1:
                        raise SystemExit(f"cannot name serde type {p} uniquely")
                    depth[p] += 1
        for name, ps in names.items():
            self.names[ps[0]] = name

    def name(self, path):
        return self.names.get(path)

    def _index_public(self):
        """Every public path (re-exports included) -> (doc, id, owner path)."""
        self.public = {}
        for n, d in enumerate(self.docs):
            index = d["index"]

            def visit(item_id, prefix, seen, n=n, index=index):
                item = index.get(str(item_id))
                if item is None or item_id in seen:
                    return
                seen = seen | {item_id}
                inner = item["inner"]
                kind = next(iter(inner))
                if kind == "use":
                    use = inner["use"]
                    if use.get("id") is not None and not use.get("is_glob"):
                        target = index.get(str(use["id"]))
                        if target is not None and target.get("visibility") == "public":
                            alias = use.get("name")
                            if alias and alias != target.get("name"):
                                # `pub use a::B as C`: C names B's item.
                                self.public.setdefault("::".join(prefix + [alias]), (n, str(use["id"]), None))
                                return
                            visit(use["id"], prefix, seen)
                    return
                if item.get("visibility") != "public":
                    return
                name = item.get("name")
                path = "::".join(prefix + [name]) if name else "::".join(prefix)
                if kind == "module":
                    for child in inner["module"]["items"]:
                        visit(child, prefix + [name], seen)
                    return
                if kind in ("struct", "enum", "function", "type_alias"):
                    self.public.setdefault(path, (n, str(item_id), None))
                if kind in ("struct", "enum"):
                    for impl_id in inner[kind].get("impls", []):
                        impl = index.get(str(impl_id))
                        if not impl or impl["inner"]["impl"].get("trait") is not None:
                            continue
                        for m in impl["inner"]["impl"]["items"]:
                            mi = index.get(str(m))
                            if mi and mi.get("visibility") == "public" and "function" in mi["inner"]:
                                self.public.setdefault(path + "::" + mi["name"], (n, str(m), path))

            visit(d["root"], [], frozenset())

    def canonical(self, path):
        """Canonical path of a public type path (re-exports resolved)."""
        hit = self.public.get(path)
        if hit is None:
            return None
        return self.path_of.get((hit[0], hit[1]))

    def fn_output(self, path):
        """Python type of the value a public Rust function/method returns
        (`Result<T>` unwrapped), or None."""
        hit = self.public.get(path)
        if hit is None:
            return None
        n, key, owner = hit
        item = self.items.get((n, key))
        if item is None or "function" not in item["inner"]:
            return None
        out = item["inner"]["function"]["sig"]["output"]
        if out is None:
            return "None"
        self._self_path = self.canonical(owner) if owner else None
        try:
            return self.resolve(n, out)
        finally:
            self._self_path = None

    _self_path = None

    def read_model(self, rust_path):
        """"FontSet" or "Font": the model a core reader such as
        `f::fzx::read` returns in its `(model, Report)` tuple."""
        segs = rust_path.replace("f::", "formats::", 1).split("::")
        module, fname = segs[:-1], segs[-1]
        for (n, k), it in self.items.items():
            if "function" not in it["inner"] or it.get("name") != fname:
                continue
            path = self.path_of.get((n, k))
            if path is None:
                span = it.get("span") or {}
                # Functions in private modules are not in `paths`; match by file.
                fname_file = span.get("filename", "")
                if not fname_file.startswith("crates/fontpixel/src/" + "/".join(module) + "/") and \
                        fname_file != "crates/fontpixel/src/" + "/".join(module) + ".rs":
                    continue
            elif path.split("::")[1:len(module) + 1] != module:
                continue
            out = it["inner"]["function"]["sig"]["output"]
            text = json.dumps(out)
            m = re.search(r'"tuple": \[\{"resolved_path": \{"path": "([\w:]+)"', text)
            if m:
                return m.group(1).split("::")[-1]
        return None

    # ---------------------------------------------------------------- types
    def resolve(self, doc_no, ty, generics=()):
        """Python type expression of a rustdoc type."""
        k = next(iter(ty))
        v = ty[k]
        if k == "primitive":
            if v in INT:
                return "int"
            if v in ("f32", "f64"):
                return "float"
            if v == "bool":
                return "bool"
            if v in ("char", "str"):
                return "str"
            if v == "unit" or v == "()":
                return "None"
            return "Any"
        if k == "borrowed_ref":
            inner = v["type"]
            if next(iter(inner)) == "slice":
                return self.seq(doc_no, inner["slice"])
            if next(iter(inner)) == "primitive" and inner["primitive"] == "str":
                return "str"
            return self.resolve(doc_no, inner, generics)
        if k == "slice":
            return self.seq(doc_no, v)
        if k == "array":
            return self.seq(doc_no, v["type"])
        if k == "tuple":
            if not v:
                return "None"
            return "tuple[" + ", ".join(self.resolve(doc_no, t, generics) for t in v) + "]"
        if k == "generic":
            if v == "Self" and self._self_path in self.names:
                return self.names[self._self_path]
            return "Any"
        if k == "resolved_path":
            name = v["path"].split("::")[-1]
            args = [a["type"] for a in ((v.get("args") or {}).get("angle_bracketed") or {}).get("args", []) if "type" in a]
            if name == "Result" and args:
                return self.resolve(doc_no, args[0], generics)
            if name == "Option" and args:
                return union(self.resolve(doc_no, args[0], generics), "None")
            if name in SEQ and args:
                return self.seq(doc_no, args[0])
            if name in MAP and len(args) >= 2:
                return f"dict[{self.resolve(doc_no, args[0], generics)}, {self.resolve(doc_no, args[1], generics)}]"
            if name in WRAP and args:
                if name == "Box" and next(iter(args[0])) == "slice":
                    return self.seq(doc_no, args[0]["slice"])
                return self.resolve(doc_no, args[0], generics)
            if name in ("PathBuf", "Path"):
                return "StrPath"
            if name in ("String", "OsString"):
                return "str"
            if name == "PhantomData":
                return "None"
            alias = self.items.get((doc_no, str(v["id"])))
            if alias is not None and "type_alias" in alias["inner"]:
                return self.resolve(doc_no, alias["inner"]["type_alias"]["type"], generics)
            target = self.path_of.get((doc_no, str(v["id"])))
            if target is None:
                # Cross-crate reference (fontpixel-ops -> fontpixel).
                target = self.external_path(doc_no, v)
            if target and target in self.names:
                return self.names[target]
            return "Any"
        return "Any"

    def external_path(self, doc_no, v):
        d = self.docs[doc_no]
        entry = d["paths"].get(str(v["id"]))
        if entry and entry["crate_id"] != 0:
            path = "::".join(entry["path"])
            if path in self.names:
                return path
            # Re-exported item: match by its last segments.
            tail = path.split("::")[-1]
            hits = [p for p in self.names if p.split("::")[-1] == tail and p.startswith(entry["path"][0])]
            if len(hits) == 1:
                return hits[0]
        return None

    def seq(self, doc_no, elem):
        if next(iter(elem)) == "primitive" and elem["primitive"] == "u8":
            return "SerdeBytes"
        return f"list[{self.resolve(doc_no, elem)}]"

    # ---------------------------------------------------------------- shapes
    def _shape(self, path):
        if path in MANUAL:
            spec = MANUAL[path]
            if spec[0] == "alias":
                return Shape(path, "alias", alias=spec[1], name=self.names[path])
            return Shape(path, "struct", fields=[Field(*f) for f in spec[1]], name=self.names[path],
                         doc=self.doc_of(path))
        key = self.serde[path]
        it = self.items[key]
        parts = serde_attrs(it)
        name = self.names[path]
        doc = first_para(it.get("docs") or "")
        for t in attr_value(parts, "try_from") + attr_value(parts, "from"):
            if t in ("String", "&str"):
                return Shape(path, "alias", alias="str", doc=doc, name=name)
        if "struct" in it["inner"]:
            return self._struct(key, it, parts, path, name, doc)
        return self._enum(key, it, parts, path, name, doc)

    def doc_of(self, path):
        key = self.serde.get(path)
        if key is None:
            return ""
        return first_para(self.items[key].get("docs") or "")

    def _fields(self, key, field_ids, parts):
        doc_no = key[0]
        rule = (attr_value(parts, "rename_all") or [None])[0]
        container_default = has_flag(parts, "default")
        out = []
        for fid in field_ids:
            f = self.items.get((doc_no, str(fid)))
            if f is None:
                continue
            fparts = serde_attrs(f)
            if any(p in ("skip", "skip_deserializing") for p in fparts):
                continue
            ty = f["inner"]["struct_field"]
            if has_flag(fparts, "flatten"):
                target = self.resolve(doc_no, ty)
                inner_path = next((p for p, n in self.names.items() if n == target), None)
                if inner_path and self.serde.get(inner_path):
                    ik = self.serde[inner_path]
                    iit = self.items[ik]
                    if "struct" in iit["inner"]:
                        plain = iit["inner"]["struct"]["kind"].get("plain")
                        if plain:
                            sub = self._fields(ik, plain["fields"], serde_attrs(iit))
                            for s in sub:
                                s.required = s.required and not container_default
                            out += sub
                            continue
                out.append(Field("**" + f["name"], "Any", False, "flattened group"))
                continue
            key_name = (attr_value(fparts, "rename") or [rename(f["name"], rule)])[0]
            withs = attr_value(fparts, "with")
            if withs and withs[0] in WITH:
                pytype = WITH[withs[0]]
                if "{V}" in pytype:
                    args = ((ty.get("resolved_path") or {}).get("args") or {}).get("angle_bracketed", {}).get("args", [])
                    v = self.resolve(doc_no, args[1]["type"]) if len(args) > 1 else "Any"
                    pytype = pytype.replace("{V}", v)
            elif withs or attr_value(fparts, "deserialize_with") and not has_flag(fparts, "flatten"):
                pytype = self.resolve(doc_no, ty)
            else:
                pytype = self.resolve(doc_no, ty)
            optional_type = "None" in pytype.split(" | ")
            required = not (container_default or has_flag(fparts, "default") or optional_type)
            out.append(Field(key_name, pytype, required, first_para(f.get("docs") or "")))
        return out

    def _struct(self, key, it, parts, path, name, doc):
        kind = it["inner"]["struct"]["kind"]
        if "plain" in kind:
            fields = self._fields(key, kind["plain"]["fields"], parts)
            return Shape(path, "struct", doc=doc, fields=fields, name=name)
        if "tuple" in kind:
            ids = [i for i in kind["tuple"] if i is not None]
            if not ids:
                raise SystemExit(f"{path}: newtype with private fields needs a MANUAL shape")
            types = [self.resolve(key[0], self.items[(key[0], str(i))]["inner"]["struct_field"]) for i in ids]
            if len(types) == 1:
                return Shape(path, "alias", doc=doc, alias=types[0], name=name)
            return Shape(path, "alias", doc=doc, alias="tuple[" + ", ".join(types) + "]", name=name)
        return Shape(path, "alias", doc=doc, alias="None", name=name)

    def _enum(self, key, it, parts, path, name, doc):
        doc_no = key[0]
        rule = (attr_value(parts, "rename_all") or [None])[0]
        tag = (attr_value(parts, "tag") or [None])[0]
        untagged = has_flag(parts, "untagged")
        literals, members, extra, aliases = [], [], [], []
        for vid in it["inner"]["enum"]["variants"]:
            var = self.items.get((doc_no, str(vid)))
            if var is None:
                continue
            vparts = serde_attrs(var)
            if any(p in ("skip", "skip_deserializing") for p in vparts):
                continue
            vname = (attr_value(vparts, "rename") or [rename(var["name"], rule)])[0]
            aliases += attr_value(vparts, "alias")
            vk = var["inner"]["variant"]["kind"]
            vdoc = first_para(var.get("docs") or "")
            if vk == "plain":
                if tag:
                    sub = Shape(path + "::" + var["name"], "struct", name=name + var["name"], doc=vdoc,
                                fields=[Field(tag, f'Literal["{vname}"]', True, "Variant tag.")])
                    extra.append(sub)
                    members.append(sub.name)
                else:
                    literals.append(vname)
                continue
            if "tuple" in vk:
                ids = [i for i in vk["tuple"] if i is not None]
                types = [self.resolve(doc_no, self.items[(doc_no, str(i))]["inner"]["struct_field"]) for i in ids]
                payload = types[0] if len(types) == 1 else "tuple[" + ", ".join(types) + "]"
                if untagged:
                    members.append(payload)
                    continue
                sub = Shape(path + "::" + var["name"], "struct", name=name + var["name"], doc=vdoc,
                            fields=[Field(vname, payload, True, vdoc)])
            else:
                fields = self._fields((doc_no, None), vk["struct"]["fields"], [])
                if tag:
                    sub = Shape(path + "::" + var["name"], "struct", name=name + var["name"], doc=vdoc,
                                fields=[Field(tag, f'Literal["{vname}"]', True, "Variant tag.")] + fields)
                else:
                    inner = Shape(path + "::" + var["name"] + "::fields", "struct",
                                  name=name + var["name"] + "Fields", doc=vdoc, fields=fields)
                    extra.append(inner)
                    sub = Shape(path + "::" + var["name"], "struct", name=name + var["name"], doc=vdoc,
                                fields=[Field(vname, inner.name, True, vdoc)])
            extra.append(sub)
            members.append(sub.name)
        if aliases:
            doc = (doc + "\n\n" if doc else "") + "Also accepted on input: " + ", ".join(f"``{a}``" for a in aliases) + "."
        if literals and not members:
            return Shape(path, "literal", doc=doc, values=literals, name=name)
        if literals:
            members = ["Literal[" + ", ".join(json.dumps(v) for v in literals) + "]"] + members
        return Shape(path, "union", doc=doc, members=members, extra=extra, name=name)


def camel(seg):
    return "".join(w.capitalize() for w in seg.split("_"))


def union(a, b):
    parts = a.split(" | ")
    return a if b in parts else f"{a} | {b}"


def first_para(doc):
    para = doc.strip().split("\n\n")[0]
    return " ".join(line.strip() for line in para.splitlines())


def ident(key):
    return key.isidentifier() and not keyword.iskeyword(key)
