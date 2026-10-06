// `fontpixel/compat/bdfparser/cjk`: bdfparser-js's `bdfparser/cjk` side-effect
// module. bdfparser-js loads its exact multi-byte (CJK) charset tables here;
// fontpixel compiles the exact tables of every charset (GB 2312, GBK, CP936,
// GB18030, EUC-JP / JIS X 0208, Shift_JIS, CP932, EUC-KR / KS C 5601, CP949,
// Johab, Big5, Big5-HKSCS, CP950, ...) into the `full` module, so importing
// this module only registers an empty marker object, for import compatibility.
import { registerCharsetTables } from './compat_bdfparser.js';

/** Registered marker: the tables themselves are built into the WebAssembly module. */
export const cjkTables = Object.freeze({ tables: Object.freeze({}), gb18030: Object.freeze([]) });

registerCharsetTables(cjkTables);
