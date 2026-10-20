// @ts-check
// A person's mark: a symmetric 8×8 pattern from their key, so people can
// tell keys apart at a glance. The same key always gives the same mark.

import { h } from '../dom.js';

const PIPS16 = ['#f7f3ea', '#cfc8ba', '#8a8299', '#4a4458', '#17151f', '#f0b38a', '#b9693f', '#6e3d2b', '#c4392f', '#e8a62c', '#8cc24a', '#4e7a3a', '#7fc4e8', '#3b5fa8', '#7b5bbf', '#d96fa0'];

/** A symmetric 8×8 pattern from the key, so a person can tell keys apart at a glance. @param {string} hex */
export function identicon(hex) {
  const cv = /** @type {HTMLCanvasElement} */ (h('canvas', { width: 8, height: 8 }));
  const g = /** @type {CanvasRenderingContext2D} */ (cv.getContext('2d'));
  g.fillStyle = '#1f2129';
  g.fillRect(0, 0, 8, 8);
  const bytes = hex.match(/../g)?.map((x) => parseInt(x, 16)) ?? [];
  const colour = PIPS16[5 + (bytes[31] % 11)];
  for (let y = 0; y < 8; y++) {
    for (let x = 0; x < 4; x++) {
      if (!((bytes[y] >> x) & 1)) continue;
      g.fillStyle = colour;
      g.fillRect(x, y, 1, 1);
      g.fillRect(7 - x, y, 1, 1);
    }
  }
  return cv;
}

