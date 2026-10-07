// Shim for bdfparser-js `src/outline.ts`: re-exports the fontpixel compatibility
// API (`fontpixel/compat/bdfparser`, resolved to the built package by
// run_reference.mjs's vitest config).
// `flattenContour` is bdfparser-js's internal curve flattener (no public
// fontpixel counterpart: the Rust rasterizer flattens internally); it is
// ported below as a test-inspection helper, see KNOWN_DIFFERENCES.md.
export { bitmapContours, rasterize } from 'fontpixel/compat/bdfparser';

// Port of bdfparser-js `flattenInto` (src/outline.ts, MIT), a test-inspection
// helper: it only reads contours produced by fontpixel.
const flattenInto = (contour, PX, PY) => {
  const n = contour.length;
  let len = 0;
  if (n === 0) return 0;
  let s = -1;
  for (let i = 0; i < n; i++) {
    if (contour[i][2]) {
      s = i;
      break;
    }
  }
  let x0;
  let y0;
  if (s < 0) {
    const a = contour[n - 1];
    const b = contour[0];
    x0 = (a[0] + b[0]) / 2;
    y0 = (a[1] + b[1]) / 2;
    s = 0;
  } else {
    x0 = contour[s][0];
    y0 = contour[s][1];
    s += 1;
  }
  const sx = x0;
  const sy = y0;
  PX[len] = x0;
  PY[len++] = y0;
  let cx = 0;
  let cy = 0;
  let ctrl = false;
  const quad = (x2, y2) => {
    for (let k = 1; k < 16; k++) {
      const t = k / 16;
      const mt = 1 - t;
      const a = mt * mt;
      const b = 2 * mt * t;
      const c = t * t;
      PX[len] = a * x0 + b * cx + c * x2;
      PY[len++] = a * y0 + b * cy + c * y2;
    }
    PX[len] = x2;
    PY[len++] = y2;
    x0 = x2;
    y0 = y2;
  };
  const total = n - (s === 0 && !contour[0][2] ? 0 : 1);
  for (let j = 0; j <= total; j++) {
    let px;
    let py;
    let on;
    if (j === total) {
      px = sx;
      py = sy;
      on = true;
    } else {
      const p = contour[(s + j) % n];
      px = p[0];
      py = p[1];
      on = p[2];
    }
    if (on) {
      if (ctrl) {
        quad(px, py);
        ctrl = false;
      } else {
        x0 = px;
        y0 = py;
        PX[len] = px;
        PY[len++] = py;
      }
    } else {
      if (ctrl) {
        quad((cx + px) / 2, (cy + py) / 2);
      }
      cx = px;
      cy = py;
      ctrl = true;
    }
  }
  return len;
};

export const flattenContour = (contour) => {
  const PX = [];
  const PY = [];
  const len = flattenInto(contour, PX, PY);
  const out = [];
  for (let i = 0; i < len; i++) out.push([PX[i], PY[i]]);
  return out;
};
