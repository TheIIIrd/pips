// @ts-check
// The card: a frame with the art and a caption. (Its back, in the
// collection, is collection/back.js.)

import { h } from '../dom.js';
import { t, date } from '../i18n.js';
import { app } from '../state.js';

/** Frame shades a card may name (format field `frame`, 0..=15). */
export const FRAME_SHADES = [
  { paper: '#f6f3eb', ink: '#1c1d22', dim: '#8e8578' }, // paper
  { paper: '#e3dccb', ink: '#1c1d22', dim: '#857b6b' }, // bone
  { paper: '#dde3e8', ink: '#1b2027', dim: '#6f7a86' }, // mist
  { paper: '#dce5d3', ink: '#1d241b', dim: '#6e7c66' }, // sage
  { paper: '#f1dddc', ink: '#2a1b1d', dim: '#8f7072' }, // blush
  { paper: '#ebd9b8', ink: '#2b2214', dim: '#8b7656' }, // sand
  { paper: '#3a3f4b', ink: '#f1efe9', dim: '#a9aebb' }, // slate
  { paper: '#1e1f25', ink: '#edebe4', dim: '#8b8f9c' }, // night
];

/** @type {Map<string, ImageData>} */
const artCache = new Map();

/** A canvas with the card's first frame. @param {any} card @param {string} [key] cache key */
export function artCanvas(card, key) {
  let img = key ? artCache.get(key) : undefined;
  if (!img) {
    const r = app.core.render(card, 0);
    img = new ImageData(new Uint8ClampedArray(r.rgba), r.width, r.height);
    if (key) artCache.set(key, img);
  }
  const cv = /** @type {HTMLCanvasElement} */ (h('canvas', { width: img.width, height: img.height }));
  /** @type {CanvasRenderingContext2D} */ (cv.getContext('2d')).putImageData(img, 0, 0);
  return cv;
}

/** @param {HTMLElement} el @param {number} shade */
function applyShade(el, shade) {
  const s = FRAME_SHADES[shade] ?? FRAME_SHADES[0];
  el.style.setProperty('--shade', s.paper);
  el.style.setProperty('--shade-ink', s.ink);
  el.style.setProperty('--shade-dim', s.dim);
}

/** @param {any} card */
const byline = (card) => (card.author ? `@${card.author}` : '—');

/**
 * A card frame.
 * @param {import('../state.js').Entry} entry
 * @param {{ bare?: boolean }} [opts]
 */
export function frame(entry, opts = {}) {
  const { card, rec } = entry;
  const front = h('div', { class: 'face front' },
    h('div', { class: 'art' }, artCanvas(card, rec.digest)),
    h('div', { class: 'cap' }, h('b', {}, card.title || '—'), h('i', {}, `${byline(card)} · ${date(rec.received, { day: '2-digit', month: '2-digit' })}`)));
  const flip = h('div', { class: 'flip' }, front);
  const el = h('div', { class: `frame${opts.bare ? ' bare' : ''}` }, flip);
  applyShade(el, card.frame);
  return el;
}
