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
  CodepointDict,
  ModelUsage,
  ProjectionResult,
  PropertyValue,
  Report,
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

  // @ts-expect-error unknown option keys are not ReadArgs
  read(bytes, { formt: "bdf" });
  // @ts-expect-error limits are numbers
  const bad: ResourceLimits = { max_glyphs: "10" };
  void bad;
  // @ts-expect-error a removed glyph slot (null) is not a glyph
  font.addGlyph(dict.fonts[0].glyphs[0]);
}
