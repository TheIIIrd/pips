// @ts-check
// What is shown: the place, whose cards, the search words and tags, and
// the order. Plain functions of plain data, so they run under node too.

/** Most tags on one card, and the longest, in bytes: pips-store's limits.
 * On the back, two rows show what fits; the rest open from a "+N" chip. */
export const MAX_TAGS = 16;
export const MAX_TAG_BYTES = 32;

/** @typedef {'collection'|'archive'|'trash'} Place */
/** @typedef {'all'|'mine'|'got'} Owner */
/** @typedef {'date'|'author'} Grouping */
/** @typedef {{place: Place, owner: Owner, query: string, by: Grouping}} Filter */
/** What these functions need of a card. @typedef {{state: Place, mine: boolean, received: number, title: string, author: string, tags: string[]}} Item */

/**
 * A tag as stored: one lower-case word, '#' and spaces dropped or joined.
 * Empty when nothing is left. The same rules as pips-store's check_tag.
 * @param {string} s
 */
export function normalTag(s) {
  const t = s.trim().replace(/^#+/, '').replace(/\s+/g, '-').replace(/#/g, '').replace(/\p{Cc}/gu, '').toLowerCase();
  const enc = new TextEncoder();
  let out = '';
  for (const ch of t) {
    if (enc.encode(out + ch).length > MAX_TAG_BYTES) break;
    out += ch;
  }
  return out;
}

/** The words of a query; words starting with '#' are tags. @param {string} q */
export const words = (q) => q.split(/\s+/).filter(Boolean);

/** @param {Item} c @param {string} q */
export function matches(c, q) {
  const hay = `${c.title} ${c.author}`.toLowerCase();
  return words(q).every((w) => (w.startsWith('#') ? c.tags.includes(normalTag(w)) : hay.includes(w.toLowerCase())));
}

/** @param {Item} c @param {Filter} f */
export const shows = (c, f) => c.state === f.place && (f.owner === 'all' || (f.owner === 'mine') === c.mine) && matches(c, f.query);

/**
 * The shown cards in order: newest first, or by author and newest first
 * within one, so the feed follows the groups.
 * @template {Item} T @param {T[]} all @param {Filter} f @returns {T[]}
 */
export function select(all, f) {
  return all.filter((c) => shows(c, f)).sort((a, b) =>
    (f.by === 'author' ? a.author.localeCompare(b.author) : 0) || b.received - a.received);
}

/** Toggles one tag word in a query. @param {string} q @param {string} tag */
export function toggleTag(q, tag) {
  const w = words(q), word = `#${tag}`;
  return (w.includes(word) ? w.filter((x) => x !== word) : [...w, word]).join(' ');
}

/** Every tag among `items`, most used first. @param {Item[]} items @returns {[string, number][]} */
export function tagCounts(items) {
  const n = new Map();
  for (const c of items) for (const t of c.tags) n.set(t, (n.get(t) ?? 0) + 1);
  return [...n].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
}
