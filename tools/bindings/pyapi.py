"""Walk the public Python API of the installed `fontpixel` package.

Shared by gen_pyref.py (the reference section of docs/python-api.md) and the
docstring test (crates/fontpixel-py/tests/test_typing.py). An *entry* is one
public module, class, function, method, property or constant, under the name
users import it by (`fontpixel.Font.draw`, `fontpixel.ops.convert`,
`fontpixel.compat.bdfparser.Glyph.draw`), never the private
`fontpixel._native` path.
"""
from __future__ import annotations

import importlib
import inspect
import pkgutil
import sys
import types
from dataclasses import dataclass

# Not part of the reference: the generated typing module (documented as a
# whole), private modules and the bdfparser command-line entry point.
SKIP_MODULES = ("fontpixel.types", "fontpixel.compat.bdfparser.__main__")


@dataclass
class Entry:
    name: str          # public dotted name
    kind: str          # module | class | exception | function | method | staticmethod | classmethod | property | constant
    obj: object
    signature: str     # "(...)" or "" (properties, constants, modules)
    doc: str


def modules():
    """Every public fontpixel module, imported, by public dotted name."""
    import fontpixel
    import fontpixel.compat.bdfparser
    import fontpixel.ops  # noqa: F401
    for info in pkgutil.walk_packages(fontpixel.__path__, "fontpixel."):
        if not any(part.startswith("_") for part in info.name.split(".")) and info.name not in SKIP_MODULES:
            importlib.import_module(info.name)
    out = {}
    for name, mod in sorted(sys.modules.items()):
        if not isinstance(mod, types.ModuleType) or not (name == "fontpixel" or name.startswith("fontpixel.")):
            continue
        if any(part.startswith("_") for part in name.split(".")) or name in SKIP_MODULES:
            continue
        out[name] = mod
    return out


def public_names(name, mod):
    explicit = getattr(mod, "__all__", None)
    names = list(explicit) if explicit is not None else [n for n in vars(mod) if not n.startswith("_")]
    out = []
    for n in sorted(names):
        obj = getattr(mod, n, None)
        if obj is None or isinstance(obj, types.ModuleType):
            continue
        origin = getattr(obj, "__module__", None)
        if explicit is None and isinstance(obj, (type, types.FunctionType)) and origin and \
                not (origin == "fontpixel" or origin.startswith("fontpixel.") or origin.startswith("_")):
            continue   # imported helpers such as typing.Any or dataclasses.field
        if explicit is None and getattr(mod, "__file__", None) and inspect.isfunction(obj) and \
                not (origin or "").startswith("fontpixel"):
            continue
        out.append(n)
    return out


def signature(obj):
    try:
        return str(inspect.signature(obj))
    except (TypeError, ValueError):
        return ""


# CPython supplies the docstrings of slot wrappers (`__contains__`, ...), and
# their wording changes between versions; the generated files use one text.
SLOT_DOCS = {"__contains__": "Return key in self."}


def doc(obj):
    if type(obj).__name__ == "wrapper_descriptor" and getattr(obj, "__name__", None) in SLOT_DOCS:
        return SLOT_DOCS[obj.__name__]
    return inspect.cleandoc(getattr(obj, "__doc__", None) or "")


def class_entries(qual, cls):
    yield Entry(qual, "exception" if issubclass(cls, BaseException) else "class", cls,
                signature(cls) if not issubclass(cls, BaseException) else "", doc(cls))
    if issubclass(cls, BaseException):
        return
    seen = set()
    for klass in cls.__mro__:
        if klass is object or klass.__module__ in ("builtins",) or \
                not (klass.__module__ or "").startswith(("fontpixel", "_")):
            continue
        for attr, raw in sorted(vars(klass).items()):
            if attr.startswith("_") or attr in seen:
                continue
            seen.add(attr)
            value = getattr(cls, attr, None)
            tname = type(raw).__name__
            if isinstance(raw, property) or tname in ("getset_descriptor", "member_descriptor"):
                yield Entry(f"{qual}.{attr}", "property", raw, "", doc(raw))
            elif isinstance(raw, staticmethod) or tname == "builtin_function_or_method":
                yield Entry(f"{qual}.{attr}", "staticmethod", value, signature(value), doc(value))
            elif isinstance(raw, classmethod) or tname == "classmethod_descriptor":
                yield Entry(f"{qual}.{attr}", "classmethod", value, signature(value), doc(value))
            elif callable(raw):
                yield Entry(f"{qual}.{attr}", "method", raw, signature(raw), doc(raw))
            else:
                yield Entry(f"{qual}.{attr}", "constant", raw, "", "")


def walk():
    """Every public entry, modules first, in a stable order."""
    seen_classes = {}
    for name, mod in modules().items():
        yield Entry(name, "module", mod, "", doc(mod))
        for attr in public_names(name, mod):
            obj = getattr(mod, attr)
            qual = f"{name}.{attr}"
            if isinstance(obj, types.GenericAlias):
                # `Point = tuple[...]`: before Python 3.11 this also passes
                # `isinstance(obj, type)`; document it as an alias everywhere.
                yield Entry(qual, "type_alias", obj, "", f"`{attr} = {obj!r}`")
            elif isinstance(obj, type):
                # A class re-exported by several modules is documented once,
                # under the first (shortest) public module that exports it.
                if id(obj) in seen_classes:
                    yield Entry(qual, "alias", obj, "", f"Same class as `{seen_classes[id(obj)]}`.")
                    continue
                seen_classes[id(obj)] = qual
                yield from class_entries(qual, obj)
            elif callable(obj):
                yield Entry(qual, "function", obj, signature(obj), doc(obj))
            else:
                yield Entry(qual, "constant", obj, "", "")
