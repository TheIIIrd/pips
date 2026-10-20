// @ts-check
// Showing bytes as QR codes: one code, or a carousel that cycles.

import { h } from '../dom.js';
import { app } from '../state.js';

const QUIET = 4;
const FRAME_MS = 220;

/** Draws text as a QR symbol with whole pixels per module. @param {HTMLCanvasElement} cv @param {string} text @param {number} maxPx */
function draw(cv, text, maxPx) {
  const m = app.core.qrMatrix(text);
  const side = m.size + QUIET * 2;
  const scale = Math.max(1, Math.floor(maxPx / side));
  cv.width = side;
  cv.height = side;
  cv.style.width = cv.style.height = `${side * scale}px`;
  const g = /** @type {CanvasRenderingContext2D} */ (cv.getContext('2d'));
  const img = g.createImageData(side, side);
  img.data.fill(255);
  for (let y = 0; y < m.size; y++) {
    for (let x = 0; x < m.size; x++) {
      if (!m.modules[y * m.size + x]) continue;
      const o = ((y + QUIET) * side + x + QUIET) * 4;
      img.data[o] = img.data[o + 1] = img.data[o + 2] = 0x17;
    }
  }
  g.putImageData(img, 0, 0);
}

/**
 * A QR display for `bytes`; returns the element, how it is shown, and a stop
 * function for carousels.
 * @param {Uint8Array} bytes @param {number} maxPx
 */
export function qrFor(bytes, maxPx) {
  const cv = /** @type {HTMLCanvasElement} */ (h('canvas', { 'aria-label': 'QR' }));
  const plan = app.core.qrPlan(bytes);
  let timer = 0;
  if (plan.kind === 'single') {
    draw(cv, plan.text, maxPx);
  } else {
    const carousel = new app.core.Carousel(bytes);
    let seq = 0;
    const next = () => draw(cv, carousel.frameText(seq++), maxPx);
    next();
    timer = window.setInterval(next, FRAME_MS);
  }
  return { el: h('div', { class: 'qr-box' }, cv), plan, stop: () => clearInterval(timer) };
}

/** A QR display for text that is already in the QR alphabet (a paper key,
 * a recipient code). @param {string} text @param {number} maxPx */
export function qrText(text, maxPx) {
  const cv = /** @type {HTMLCanvasElement} */ (h('canvas', { 'aria-label': 'QR' }));
  draw(cv, text, maxPx);
  return h('div', { class: 'qr-box' }, cv);
}

/** Keeps the screen on while a code is shown, where the browser allows it. */
export async function keepAwake() {
  try {
    const lock = await /** @type {any} */ (navigator).wakeLock?.request('screen');
    return () => lock?.release();
  } catch {
    return () => {};
  }
}
