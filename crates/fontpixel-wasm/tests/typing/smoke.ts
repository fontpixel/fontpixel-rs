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
import * as bdfparser from "fontpixel/compat/bdfparser";
import { main as bdfparserMain } from "fontpixel/compat/bdfparser/cli";
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
