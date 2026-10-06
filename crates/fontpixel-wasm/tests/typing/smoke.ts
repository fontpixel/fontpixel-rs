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
import type {
  Capability,
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
  // @ts-expect-error plain data is not a handle
  font.addGlyph(dict.fonts[0].glyphs[0]);
}
