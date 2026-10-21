// @ts-check
// Where each card is at each level of the zoom axis. Pure geometry: cards
// in, rectangles out. A rectangle is {x, y, w, cap}: the paper's width and
// how much of the caption shows (0 a bare tile … 1 the full frame).
//
//   level 0  months   small tiles, a group per month (or per author)
//   level 1  weeks    larger tiles, a group per week (or per author)
//   level 2  feed     one card in the middle, neighbours tucked in
//   level 3  card     one card, large, ready to turn over

/** @typedef {{x: number, y: number, w: number, cap: number}} Rect */
/** @typedef {{key: string, first: any, count: number}} Group */
/** @typedef {{W: number, H: number, top: number, bottom: number, lead: number}} Frame
 * top, bottom: room kept for bars; lead: extra room above the months (search). */

const GAP = 6, PAD = 16;
/** The widest the grid gets; wider screens keep margins. */
export const MAX_W = 1100;

/** Monday-based week start, local time. @param {number} ms */
export function weekStart(ms) {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  d.setDate(d.getDate() - ((d.getDay() + 6) % 7));
  return d.getTime();
}

/**
 * Consecutive runs of cards that share a key.
 * @template T @param {T[]} list @param {(c: T) => string} key
 * @returns {{key: string, cards: T[]}[]}
 */
export function runs(list, key) {
  /** @type {{key: string, cards: T[]}[]} */
  const out = [];
  for (const c of list) {
    const k = key(c);
    if (!out.length || out[out.length - 1].key !== k) out.push({ key: k, cards: [] });
    out[out.length - 1].cards.push(c);
  }
  return out;
}

/** The grouping key of a card at a grid level.
 * @param {0|1} level @param {'date'|'author'} by @param {{received: number, author: string}} c */
export function groupKey(level, by, c) {
  if (by === 'author') return `a:${c.author}`;
  if (level === 0) { const d = new Date(c.received); return `m:${d.getFullYear()}-${d.getMonth()}`; }
  return `w:${weekStart(c.received)}`;
}

/** Columns: tiles keep a comfortable size on any screen — more of them, not giants. @param {0|1} level @param {number} W */
export const columns = (level, W) => { const w = Math.min(W, MAX_W); return level === 0 ? Math.max(5, Math.round(w / 84)) : Math.max(3, Math.round(w / 150)); };

/**
 * A grid level: every card's rectangle (in content coordinates, before
 * scrolling), the group labels, each a cell of its own, and how far the
 * content scrolls.
 * @template T
 * @param {0|1} level @param {{key: string, cards: T[]}[]} groups @param {Frame} f
 */
export function grid(level, groups, f) {
  const cols = columns(level, f.W);
  // On a wide screen the grid keeps a readable width, centred.
  const pad = Math.max(PAD, (f.W - MAX_W) / 2 + PAD);
  const s = (f.W - 2 * pad - (cols - 1) * GAP) / cols;
  const top = f.top + (level === 0 ? f.lead : 0);
  /** @type {Map<T, Rect>} */
  const rects = new Map();
  /** @type {{key: string, cards: T[], x: number, y: number, w: number}[]} */
  const labels = [];
  let k = 0;
  const at = () => ({ x: pad + (k % cols) * (s + GAP), y: top + Math.floor(k / cols) * (s + GAP), w: s });
  for (const g of groups) {
    labels.push({ ...g, ...at() });
    k++;
    for (const c of g.cards) {
      rects.set(c, { ...at(), cap: 0 });
      k++;
    }
  }
  const height = top + Math.ceil(k / cols) * (s + GAP) + f.bottom + 24;
  return { rects, labels, max: Math.max(0, height - f.H), cell: s };
}

/** The feed card's width. @param {Frame} f */
export const feedWidth = (f) => Math.min(f.W * 0.68, (f.H - f.top - f.bottom) * 0.6, 520);
/** The single card's width. @param {Frame} f */
export const cardWidth = (f) => Math.min(f.W * 0.84, (f.H - f.top - f.bottom - 24) / 1.29, 560);

/**
 * A card in the feed (or, with `single`, the card level), `i` places from
 * the start of the feed whose centre is at `pos`. Neighbours tuck in close
 * and a little lower; the centre one is lifted.
 * @param {number} i @param {number} pos @param {boolean} single @param {Frame} f
 * @returns {Rect & {o: number, z: number}}
 */
export function feedRect(i, pos, single, f) {
  const d = i - pos, a = Math.abs(d);
  if (single) {
    const w = cardWidth(f);
    return { x: (f.W - w) / 2 + d * f.W * 1.05, y: f.top + 8, w, cap: 1, o: a < 0.5 ? 1 : 0, z: 100 };
  }
  const w = feedWidth(f);
  // A neighbour moves out fast at first and settles: halfway (when the
  // two cards swap which is on top) they no longer overlap, so neither
  // seems to pass through the other.
  const near = Math.min(a, 1), out = 1 - (1 - near) * (1 - near);
  const off = Math.sign(d) * (out * w * 0.66 + Math.max(0, a - 1) * w * 0.2);
  const s = 1 - Math.min(a, 1.6) * 0.12;
  const h = w * 1.29;
  return {
    x: f.W / 2 - (w * s) / 2 + off,
    y: f.top + (f.H - f.top - f.bottom - h) / 2 + Math.min(a, 1) * 22,
    w: w * s, cap: 1, o: a > 3.2 ? Math.max(0, 4 - a) : 1, z: 100 - Math.round(a * 10),
  };
}

