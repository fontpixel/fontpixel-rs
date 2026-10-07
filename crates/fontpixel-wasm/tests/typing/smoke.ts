// TypeScript smoke test of the package declarations (`tsc --noEmit --strict`,
// run by scripts/check_wasm.sh against the built package). It is never
// executed; it checks that typical code type-checks and that misuse fails.
import {
  Bitmap,
  BitmapProjection,
  Codepoint,
  Font,
  FontSet,
  Glyph,
  Input,
  OutputBundle,
  Properties,
  ReadContext,
  ResourceSet,
  Written,
  applyBitmapProjection,
  applyNames,
  reportPush,
  InvalidInputError,
  MissingResourceError,
  ResourceLimitError,
  UnsupportedError,
  BatchResult,
  FontpixelWorker,
  OperationResult,
  capabilities,
  createWorker,
  info,
  init,
  initSync,
  merge,
  mergeInWorker,
  normalize,
  ops,
  read,
  render,
  requireCapability,
  version,
  wasmUrl,
  withAsyncResources,
} from "fontpixel";
import * as bdfparser from "fontpixel/compat/bdfparser";
import { main as bdfparserMain } from "fontpixel/compat/bdfparser/cli";
import type {
  Capability,
  CodepointDict,
  ModelUsage,
  ProjectionResult,
  PropertyValue,
  Report,
  InfoDocument,
  MergeFilesResult,
  Progress,
  RenderResult,
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

  // Model, metadata and resource API.
  const cp = Codepoint.unicode(65);
  const id: number | undefined = font.glyphId(cp) ?? font.glyphId("A") ?? font.glyphId({ Unicode: 65 });
  void id;
  const pairs: [CodepointDict, number][] = font.codepoints();
  void pairs;
  for (const g of font) {
    const gid: number | undefined = g.id;
    void gid;
  }
  for (const f of set) void f.length;
  const props: Properties = font.properties;
  props.set("BIG", 2n ** 64n - 1n);
  props.push("COMMENT", "x", new Uint8Array(1));
  const value: PropertyValue | undefined = props.get("BIG");
  void value;
  font.editGlyph(0, (g: Glyph) => {
    g.name = "edited";
    g.shift(1n, -1);
  });
  const usage: ModelUsage = font.retainedUsage(1 << 20);
  void usage;
  const fresh = new FontSet([font, font.toDict()], { sourceFormat: "bdf" });
  const names: Report = applyNames(fresh, { family: "Demo" }, limits);
  const outcome: ProjectionResult = applyBitmapProjection(fresh, {
    scalable_advance: { from_device: { units_per_em: 1000, reference_ppem: 13 } },
    properties: [{ set: { name: "QUAD_WIDTH", value: 7n } }],
  });
  const retained: number = outcome.retained_bytes + BitmapProjection.MAX_PROPERTY_EDITS;
  void [names, retained];
  const rs = new ResourceSet({ "a.bin": new Uint8Array(1) });
  const ctx = new ReadContext(new Input(bytes, { name: "a.bdf", resources: rs }), limits);
  const got: Uint8Array = ctx.resource("a.bin");
  const listed: string[] = ctx.listResourcesLimited(null, 10, 1000);
  void [got, listed];
  const bundle = new OutputBundle("a.bin", [["a.bin", new Uint8Array(1)]]);
  for (const asset of bundle) {
    const p: string = asset.path;
    void p;
  }
  const written = new Written(bundle);
  const pushed: Report = reportPush(written.report, {
    level: "warning",
    impact: "loss",
    code: "x",
    location: { font_index: 0, glyph_index: null, codepoint: null, asset: null },
    message: "m",
  });
  void pushed;
  // @ts-expect-error codepoint kinds are "unicode" | "legacy"
  new Codepoint(1, "other");
  // ops and the W8.3 calls
  const converted: OperationResult = ops.convert(
    { inputs: ["a.bdf"], output: "a.pcf" },
    { files: { "a.bdf": bytes }, progress: (p: Progress) => p.step !== "encode" },
  );
  const outputs: Record<string, Uint8Array> = converted.outputs;
  const doc: string = converted.json(false);
  const batch: BatchResult = ops.batch({ tasks: [{ id: "a", kind: "validate", input: "a.bdf" }] }, { files: new Map([["a.bdf", bytes]]) });
  const code: number = batch.exitCode;
  const infoDoc: InfoDocument = info(bytes, { name: "a.bdf" });
  const merged: MergeFilesResult = merge([{ bytes, take: "western", name: "a.bdf" }], { family: "Mix", outputs: ["m.bdf"] });
  const files: Record<string, Uint8Array> = merged.files;
  const pixels: RenderResult = render(bytes, "Hi", { format: "rgba", scale: 2 });
  const worker: FontpixelWorker = await createWorker({ module, cancellation: "auto" });
  const viaWorker: MergeFilesResult = await worker.merge([{ bytes, take: "all" }], { family: "Mix", outputs: ["m.woff2"] }, {
    onProgress: (p) => void p.done,
    signal: new AbortController().signal,
  });
  const once: MergeFilesResult = await mergeInWorker([{ bytes, take: "all" }], { family: "Mix", outputs: ["m.bdf"] });
  worker.terminate();
  void [outputs, doc, code, infoDoc, files, pixels.data, viaWorker, once];
  // @ts-expect-error progress cannot cross into a worker
  await worker.merge([{ bytes, take: "all" }], { family: "Mix", outputs: ["m.bdf"], progress: () => false });

  // @ts-expect-error unknown option keys are not ReadArgs
  read(bytes, { formt: "bdf" });
  // @ts-expect-error limits are numbers
  const bad: ResourceLimits = { max_glyphs: "10" };
  void bad;
  // @ts-expect-error a removed glyph slot (null) is not a glyph
  font.addGlyph(dict.fonts[0].glyphs[0]);

  // bdfparser-js compatible API (fontpixel/compat/bdfparser)
  const compat: bdfparser.Font = bdfparser.Font.create({ pixelsize: 8 });
  const g: bdfparser.Glyph = compat.newglyph(65, bdfparser.Bitmap.fromstring("#."), { mode: 1 });
  const meta: bdfparser.GlyphMetaInFont | undefined = compat.glyphs.get(65);
  const rows: string[] = compat.draw("A").bindata;
  const pcf: Uint8Array = compat.topcf({ glyphpad: 4 });
  const codec: bdfparser.CharsetCodec = bdfparser.getCodec("gb2312", { gl: true });
  const prev: bdfparser.WarningHandler | null = bdfparser.setWarningHandler((m: string) => void m);
  const code: number = bdfparserMain(["--version"], { stdout: () => undefined, stderr: () => undefined });
  void [g.cp(), meta, rows, pcf, codec.char2cp("A"), prev, code];
  // @ts-expect-error bdfparser-js's option names
  compat.topcf({ glyph_pad: "4" });
}

// Bitmap operations, rendering, charsets and outlines (appended by the
// bitmap/render/charset/outline unit; a separate import keeps merges simple).
import * as fpx from "fontpixel";

export function smokeBitmapRender(font: fpx.Font): void {
  const b: fpx.Bitmap = fpx.Bitmap.fromText(".#\n#.", { palette: { ".": 0, "#": 1 } });
  const g: fpx.Bitmap = fpx.Bitmap.fromRows([[0, 255]], { depth: "gray8", limits: fpx.defaultBitmapLimits() });
  const moved: fpx.Bitmap = b.shift(1n, -1).crop(4, 4, 0, 0).concat(b, { direction: "down", gap: 1 });
  b.composite(moved, 0, 0, "xor");
  const rows: Uint8Array[] = g.toRows();
  const values: fpx.WideInt[] = b.toRowValues();
  const text: string = b.toText([[0, " "], [1, "@"]]);
  const box: fpx.Bounds = b.bbox();
  const layout: fpx.ByteLayout = fpx.defaultByteLayout();
  const packed: Uint8Array = b.toBytes({ ...layout, bit_order: "lsb" });
  const all: fpx.Bitmap = fpx.Bitmap.concatAll([b, moved], { alignment: "end", gaps: [2n] });
  void [rows, values, text, box, packed, all];

  const options: fpx.RenderOptions = { mode: "cell", direction: fpx.parseWritingDirection("tbrl") };
  const rendered: fpx.Rendered = font.draw("Hi", options, font.glyph(0));
  const report: fpx.Report = rendered.report;
  const png: Uint8Array = fpx.toPng(rendered.bitmap, { scale: 2, foreground: fpx.parseRgba("#f00") });
  const svg: string = fpx.toSvg(font.drawAll("descending", [[65, 90]]).bitmap, { background: fpx.rgbaTransparent() });
  const dict: fpx.RenderedDict = font.drawCodepoints([65, "B", { Legacy: 7 }, null]).toDict();
  void [report, png, svg, dict];

  const codec: fpx.CharsetCodec = fpx.CharsetCodec.forFont(font);
  const unicode: number | undefined = codec.cp2unicode(0x41);
  const table = fpx.charsetTable();
  const available: boolean = table[0].available;
  const block: fpx.UnicodeRange | undefined = fpx.unicodeBlock(0x4e00);
  const kuten: [number, number] | undefined = fpx.eucToKuten(0xb0a1);
  void [unicode, available, block, kuten];

  const contours: fpx.Contour[] = fpx.outlineContours(b, { shape: "dot", dot_size: 0.9 });
  const raster: fpx.Bitmap = fpx.outlineRasterize(contours, 100, [0, 0], 2, 2);
  const edges: fpx.Edge[] = [{ line: [[0, 0], [1, 1]] }];
  const path: fpx.Bitmap = fpx.pathRasterize(edges, 1, [0, 0], 2, 2, { max_edges: 4 });
  void [raster, path, fpx.outlineDefaults().Limits.max_points];

  // @ts-expect-error depth is "mono" | "gray8"
  fpx.Bitmap.fromRows([[0]], { depth: "rgb" });
  // @ts-expect-error render modes are a closed set
  font.draw("x", { mode: "fast" });
}

import * as formatApi from "fontpixel";
export function smokeFormats(set: FontSet, bytes: Uint8Array): void {
  const registry = new formatApi.Registry();
  registry.setPcfWriteOptions({});
  const result: Written = registry.write(set, "pcf");
  const bundle: OutputBundle = result.output;
  const data: Uint8Array = bundle.intoSingle();
  const again: Written = Written.fromDict(result.toDict());
  const asset: formatApi.OutputAssetLike = { path: "font.pcf", data };
  const rebuilt = new OutputBundle("font.pcf", [asset]);
  formatApi.formats.bdf.read(bytes);
  registry.free(); result.free(); bundle.free(); again.free(); rebuilt.free();
}

// OpenType, WOFF, variants and library merge (unit J4).
import { Cff, Collection, Selection, Sfnt, SfntBuilder, Variants, mergeLib, sfnt, variants, woff } from "fontpixel";
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
  const merged: MergeLibResult = mergeLib.merge([{ font: base, selection: new Selection("U+41-5A"), dy: 1n }], { family: "X" });
  const mergedFont: Font = merged.font;
  void [os2, name, checksum, cmap, glyphs, faces, metadata, out, single, mergedFont];
  // @ts-expect-error the merge family is required
  mergeLib.merge([], {});
  // @ts-expect-error tags are strings or bytes
  s.table(1);
}

import type { TaskRequest, BatchTaskSpec } from "fontpixel";
export function smokeTaggedRequests(): void {
  const task: TaskRequest = { kind: "validate", input: "a.bdf" };
  const batch: BatchTaskSpec = { id: "check", ...task };
  ops.batch({ tasks: [batch] }, { files: {} });
  // @ts-expect-error externally tagged shape is not the serde contract
  const wrong: TaskRequest = { validate: { input: "a.bdf" } };
  // @ts-expect-error unknown request fields must not be admitted by an index signature
  const typo: BatchTaskSpec = { id: "check", kind: "validate", inpt: "a.bdf" };
  void [wrong, typo];
}
