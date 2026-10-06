#!/usr/bin/env python3
"""List every public item of the Rust crates the bindings must cover.

Reads nightly rustdoc JSON (`cargo +nightly rustdoc -Z unstable-options
--output-format json`) for each crate and writes one sorted JSON inventory:

    {"items": [{"path": "fontpixel::model::Font::glyph", "kind": "method",
                "parent": "fontpixel::model::Font", "serde": [..]}, ...]}

Items are reachable public modules, structs, enums (and their variants), traits,
functions, constants, type aliases, inherent public methods and public struct
fields. `serde` lists which of Serialize/Deserialize a struct or enum implements,
so the checker can treat the fields of a serde-adapted type as covered.

Usage: inventory.py OUT.json CRATE.json [CRATE.json ...]
"""
import json
import sys


def items_of(doc):
    index = doc["index"]
    out = {}

    def get(i):
        return index.get(str(i))

    def traits_of(item, kind):
        names = set()
        for impl_id in item["inner"][kind].get("impls", []):
            impl = get(impl_id)
            if not impl:
                continue
            trait = impl["inner"]["impl"].get("trait")
            if trait:
                names.add(trait.get("path", "").split("::")[-1])
        return names

    paths = doc.get("paths", {})
    by_id = {}

    def canonical(item_id, fallback):
        entry = paths.get(str(item_id))
        return "::".join(entry["path"]) if entry and entry.get("path") else fallback

    def add(path, kind, parent, item_id=None, **extra):
        # Re-exports reach one item under several paths: keep the canonical one
        # and record the others as aliases.
        if item_id is not None and item_id in by_id:
            entry = by_id[item_id]
            if path != entry["path"] and path not in entry.setdefault("aliases", []):
                entry["aliases"].append(path)
            return entry["path"]
        if item_id is not None:
            path = canonical(item_id, path)
        entry = out.setdefault(path, {"path": path, "kind": kind, "parent": parent, **extra})
        if item_id is not None:
            by_id[item_id] = entry
        return entry["path"]

    def visit(item_id, prefix, seen):
        item = get(item_id)
        if item is None or item_id in seen:
            return
        seen = seen | {item_id}
        inner = item["inner"]
        kind = next(iter(inner))
        name = item.get("name")
        if kind == "use":
            use = inner["use"]
            if use.get("id") is not None and not use.get("is_glob"):
                target = get(use["id"])
                if target is not None and target.get("visibility") == "public":
                    # A re-export exposes the target under this module's path.
                    visit(use["id"], prefix, seen)
            return
        if item.get("visibility") != "public":
            return
        path = "::".join(prefix + [name]) if name else "::".join(prefix)
        if kind == "module":
            add(path, "module", "::".join(prefix), item_id)
            for child in inner["module"]["items"]:
                visit(child, prefix + [name], seen)
            return
        if kind in ("struct", "enum", "trait"):
            serde = sorted(traits_of(item, kind) & {"Serialize", "Deserialize"}) if kind != "trait" else []
            seen_before = item_id in by_id
            path = add(path, kind, "::".join(prefix), item_id, serde=serde)
            if seen_before:
                return
            if kind == "struct":
                fields = inner["struct"]["kind"]
                if isinstance(fields, dict) and "plain" in fields:
                    for f in fields["plain"]["fields"]:
                        fi = get(f)
                        if fi and fi.get("visibility") == "public":
                            add(path + "::" + fi["name"], "field", path)
            if kind == "enum":
                for v in inner["enum"]["variants"]:
                    vi = get(v)
                    if vi:
                        add(path + "::" + vi["name"], "variant", path)
            impls = inner[kind].get("impls", []) if kind != "trait" else []
            for impl_id in impls:
                impl = get(impl_id)
                if not impl or impl["inner"]["impl"].get("trait") is not None:
                    continue
                for m in impl["inner"]["impl"]["items"]:
                    mi = get(m)
                    if mi and mi.get("visibility") == "public" and mi.get("name"):
                        mk = "method" if "function" in mi["inner"] else next(iter(mi["inner"]))
                        add(path + "::" + mi["name"], mk, path)
            if kind == "trait":
                for m in inner["trait"]["items"]:
                    mi = get(m)
                    if mi and mi.get("name"):
                        add(path + "::" + mi["name"], "trait_item", path)
            return
        if kind in ("function", "constant", "type_alias", "static", "macro"):
            add(path, kind, "::".join(prefix), item_id)

    visit(doc["root"], [], frozenset())
    return out


def main():
    out_path, *docs = sys.argv[1:]
    if not docs:
        raise SystemExit(__doc__)
    merged = {}
    for doc_path in docs:
        with open(doc_path) as f:
            merged.update(items_of(json.load(f)))
    items = sorted(merged.values(), key=lambda x: x["path"])
    with open(out_path, "w") as f:
        json.dump({"items": items}, f, indent=1, ensure_ascii=False)
        f.write("\n")
    kinds = {}
    for i in items:
        kinds[i["kind"]] = kinds.get(i["kind"], 0) + 1
    print(json.dumps({"items": len(items), "kinds": kinds}))


if __name__ == "__main__":
    main()
