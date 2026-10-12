# fontpixel (JavaScript / WebAssembly)

Bitmap font toolkit for JavaScript on a Rust core compiled to WebAssembly:
read, edit, render and write bitmap fonts in 99 formats (BDF, PCF, PSF, YAFF,
FNT/FON, BMFont, image atlases, OpenType bitmap and outline fonts,
WOFF/WOFF2, ...), with OpenType `locl` variants, subsets, merging and every
`fontpixel` command-line operation in-process. `fontpixel/compat/bdfparser`
is a compatible replacement for [bdfparser-js](https://github.com/tomchen/bdfparser-js) 3.

- ES modules, Node 18+ and modern browsers (main thread or Web Worker)
- TypeScript declarations for every function, class and option
- fonts stay in WebAssembly memory; JavaScript holds handles
- memory-only API (bytes in, bytes out); Node file helpers in
  `fontpixel/node-files`; a Worker helper for long operations with progress
  and cancellation

```sh
npm install fontpixel
```

## Quick start

```js
import * as fp from 'fontpixel';

await fp.init();                                  // load the WebAssembly module once
const bytes = new Uint8Array(await (await fetch('spleen-5x8.bdf')).arrayBuffer());
const { fonts, report } = fp.read(bytes, { name: 'spleen-5x8.bdf' });
const font = fonts.font(0);
console.log(font.length, report.entries.length);

const pcf = fp.write(fonts, 'pcf').output.intoSingle();
const png = fp.toPng(font.draw('Hello').bitmap);
font.free();
fonts.free();                                     // or `using` / Symbol.dispose
```

In browsers the module (`fontpixel_wasm_bg.wasm`, next to `index.js`) is
fetched by `init()`; bundlers (Vite, webpack 5, esbuild, Rollup) copy it
through `new URL(..., import.meta.url)`. Serve it as `application/wasm`.
Large builds must be instantiated asynchronously on a browser's main thread
(`await init()`).

## Builds and sizes

The package contains several builds of the same API. Pick one per application;
each is a separate WebAssembly module, and functions that a build leaves out
throw `FeatureDisabledError`/`UnsupportedError` (check `capabilities()`).

<!-- scripts/build_npm.sh replaces SIZE_* / GZ_* with the sizes of the packed builds. -->

| import | contents | `.wasm` | gzip |
|---|---|---:|---:|
| `fontpixel` | everything: 99 formats, bdfparser compatibility, CJK charset tables, TOML batch jobs | SIZE_FULL | GZ_FULL |
| `fontpixel/web` | common formats (BDF, PCF, YAFF, OTB, TTF/OTF, WOFF/WOFF2, PSF, FNT/FON, HEX, BMFont, C), CJK tables | SIZE_WEB | GZ_WEB |
| `fontpixel/bdfparser` | the bdfparser-js compatibility layer (BDF, PCF, TrueType, CJK tables) and the native API for those formats | SIZE_BDFPARSER | GZ_BDFPARSER |
| `fontpixel/minimal` | BDF, PCF and YAFF | SIZE_MINIMAL | GZ_MINIMAL |

Each build has the same entry points under its prefix:
`fontpixel/web/node-files`, `fontpixel/bdfparser/compat/bdfparser`, and so on.

## bdfparser-js compatibility

```js
import { Font } from 'fontpixel/compat/bdfparser';      // or 'fontpixel/bdfparser/compat/bdfparser'
import 'fontpixel/compat/bdfparser/cjk';                // accepted; the CJK tables are built in

const font = await new Font().load_url('spleen-5x8.bdf');      // or load_bytes / load_string
console.log(font.draw('Hello').toString());
```

The module initializes WebAssembly itself (top-level `await`), so CommonJS
code loads it with `await import(...)`. `fontpixel/compat/bdfparser/cli` is
bdfparser-js's command line. These import paths are stable within fontpixel
0.x; bdfparser 4.x is a thin package over them. The behaviour differences from
bdfparser-js 3.x are listed in
[bdfparser-compat-differences.md](https://github.com/fontpixel/fontpixel-rs/blob/main/docs/bdfparser-compat-differences.md).

## Documentation

- [JavaScript guide](https://github.com/fontpixel/fontpixel-rs/blob/main/docs/js-api.md):
  loading, handles and release, options, errors, limits, resources, Workers
- [API reference](https://github.com/fontpixel/fontpixel-rs/blob/main/docs/js-api-reference.md)
- [Runnable examples](https://github.com/fontpixel/fontpixel-rs/blob/main/examples/js/recipes.mjs)
- [Changelog](https://github.com/fontpixel/fontpixel-rs/blob/main/CHANGELOG.md)

## License

MIT. The charset tables derive from CPython's codecs and some semantics from
other projects; see `THIRD_PARTY_NOTICES.md` and `LICENSES/` in the package.
