// Type declarations of `fontpixel/compat/bdfparser/cli` (bdfparser-js's CLI,
// Node.js only, in-process).
import type { Font } from "./compat_bdfparser.js";

/** Output streams used by {@link main} */
export type CliIO = {
  stdout: (text: string) => void;
  stderr: (text: string) => void;
};

/** Load a font file (BDF, PCF, gzip-compressed PCF/BDF or TTF, auto-detected). */
export declare function loadFontFile(path: string, pixelsize?: number): Font;

/** Run the `bdfparser` command; returns the exit status (0, 1 error, 2 usage error). */
export declare function main(argv?: string[], io?: CliIO): number;
