// TypeScript smoke test of the package declarations (`tsc --noEmit --strict`,
// run by scripts/check_wasm.sh against the built package). It is never
// executed; it checks that typical code type-checks and that misuse fails.
import {
  Bitmap,
  Font,
  FontSet,
  Glyph,
  InvalidInputError,
  MissingResourceError,
  ResourceLimitError,
  UnsupportedError,
  capabilities,
  init,
  initSync,
  normalize,
  read,
  requireCapability,
  version,
  wasmUrl,
  withAsyncResources,
} from "fontpixel";
import type {
  Capability,
  FontMetrics,
  FontSetDict,
  GlyphBitmap,
  Loaded,
  ReadArgs,
  ResourceLimits,
  SerdeTypes,
  WideInt,
} from "fontpixel";

export async function smoke(bytes: Uint8Array, module: WebAssembly.Module): Promise<void> {
  await init();
  initSync(module);
  const v: string = version();
  const url: URL = wasmUrl;
  const caps: Capability[] = capabilities();
  const missing = caps.filter((c) => !c.available).map((c) => `${c.name}: ${c.reason} -> ${c.alternative}`);
  void [v, url, missing];

  const limits: ResourceLimits = { max_glyphs: 10, max_input_bytes: 1 << 20 };
  const args: ReadArgs = { format: "bdf", name: "a.bdf", limits, resources: { "a.bin": new Uint8Array(1) } };
  const loaded: Loaded = read(bytes, args);
  const set: FontSet = loaded.fonts;
  const font: Font = set.font(0);
  const ids: number[] = font.glyphIds();
  const glyph: Glyph | undefined = font.glyph(ids[0]);
  if (glyph) {
    const bitmap: Bitmap = glyph.bitmap;
    bitmap.setPixel(0, 0, 1);
    glyph.bitmap = bitmap;
    const plain: GlyphBitmap = bitmap.toDict();
    void plain;
  }
  const dict: FontSetDict = set.toDict();
  const metrics: FontMetrics = dict.fonts[0].metrics;
  const ascent: WideInt | null | undefined = metrics.ascent;
  if (typeof ascent === "bigint") {
    const big: bigint = ascent;
    void big;
  }
  const report = loaded.report;
  void report.entries;
  const added: number = font.addGlyph(new Glyph(Bitmap.fromText("#"), { codepoints: [65, { Legacy: 7 }] }));
  void added;

  const normalized = normalize("resources::ResourceLimits", {}) as SerdeTypes["resources::ResourceLimits"];
  const glyphs: number | undefined = normalized.max_glyphs;
  void glyphs;

  try {
    requireCapability("filesystem");
  } catch (e) {
    if (e instanceof UnsupportedError) {
      const why: string | undefined = e.reason;
      void why;
    } else if (e instanceof ResourceLimitError) {
      const limit: number = e.limit;
      void limit;
    } else if (e instanceof InvalidInputError) {
      const code: number = e.exitCode;
      void code;
    }
  }

  const viaAsync: Loaded = await withAsyncResources(
    (resources) => read(bytes, { format: "hbf", resources }),
    async (path) => (path === "font.bin" ? new Uint8Array(4) : null),
  );
  void viaAsync;
  try {
    read(bytes, { resources: (path, maxBytes) => (maxBytes > 0 && path ? new ArrayBuffer(1) : undefined) });
  } catch (e) {
    if (e instanceof MissingResourceError) {
      const path: string = e.path;
      void path;
    }
  }
  {
    using scoped = Bitmap.fromText("#.");
    void scoped.width;
  }

  // @ts-expect-error unknown option keys are not ReadArgs
  read(bytes, { formt: "bdf" });
  // @ts-expect-error limits are numbers
  const bad: ResourceLimits = { max_glyphs: "10" };
  void bad;
  // @ts-expect-error plain data is not a handle
  font.addGlyph(dict.fonts[0].glyphs[0]);
}

// OpenType, WOFF, variants and library merge (unit J4).
import { Cff, Collection, Selection, Sfnt, SfntBuilder, Variants, merge, sfnt, variants, woff } from "fontpixel";
import type { BytesWithReport, MergeLibResult, NameTable, SfntTableRecord, VariantsEncoded, WoffDecoded } from "fontpixel";

export function smokeOpenType(bytes: Uint8Array, base: Font): void {
  const s = new Sfnt(bytes, { max_tables: 64 });
  const records: SfntTableRecord[] = s.tables();
  const os2: Uint8Array | undefined = s.table("OS/2");
  const name: NameTable = sfnt.name.decode(s.table("name") ?? new Uint8Array());
  const rebuilt = new SfntBuilder(s.flavor);
  for (const r of records) rebuilt.insert(r.tag, r.data);
  const checksum: number = sfnt.checksum(rebuilt.build());
  const cmap: Uint8Array = sfnt.cmap.encode({ 65: 1 }, 2).data;
  const glyphs: number = Cff.fromSfnt(s).length;
  const faces = new Collection(bytes).faces.map((f) => f.tables.length);
  const web: BytesWithReport = woff.woff2.encode(bytes, { quality: 5 });
  const back: WoffDecoded = woff.woff2.decode(web.bytes);
  const metadata: Uint8Array | null = back.auxiliary.metadata;
  const v = new Variants(base);
  v.addLanguage("zh-Hant", base.toDict());
  const out: VariantsEncoded = v.encode({ flavor: "cff" });
  const single: Uint8Array = variants.encodeSingle(base).bytes;
  const merged: MergeLibResult = merge.merge([{ font: base, selection: new Selection("U+41-5A"), dy: 1n }], { family: "X" });
  const mergedFont: Font = merged.font;
  void [os2, name, checksum, cmap, glyphs, faces, metadata, out, single, mergedFont];
  // @ts-expect-error the merge family is required
  merge.merge([], {});
  // @ts-expect-error tags are strings or bytes
  s.table(1);
}
