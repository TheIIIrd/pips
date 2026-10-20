// @ts-check
// The card: a frame with the art and a caption, and a back side.

import { h, byteLimit } from '../dom.js';
import { t, date } from '../i18n.js';
import { app, setNote, NOTE_BYTES } from '../state.js';
import { toast, ask } from './shell.js';

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
 * @param {{ back?: boolean, bare?: boolean }} [opts]
 */
export function frame(entry, opts = {}) {
  const { card, rec } = entry;
  const front = h('div', { class: 'face front' },
    h('div', { class: 'art' }, artCanvas(card, rec.digest)),
    h('div', { class: 'cap' }, h('b', {}, card.title || '—'), h('i', {}, `${byline(card)} · ${date(rec.received, { day: '2-digit', month: '2-digit' })}`)));
  const flip = h('div', { class: 'flip' }, front, opts.back ? backSide(entry) : null);
  const el = h('div', { class: `frame${opts.bare ? ' bare' : ''}` }, flip);
  applyShade(el, card.frame);
  return el;
}

/** @param {import('../state.js').Entry} entry */
function backSide({ card, rec, author }) {
  const me = app.identity.publicKey().hex;
  const copy = (/** @type {string} */ text) => h('button', {
    onclick: (/** @type {Event} */ e) => {
      e.stopPropagation();
      navigator.clipboard?.writeText(text).then(() => toast(t('toast.copied')), () => {});
    },
  }, t('back.copy'));

  const links = [];
  if (card.handle) links.push(h('div', { class: 'chip-line' }, h('small', {}, card.handle.network), h('span', {}, card.handle.text), copy(card.handle.text)));
  if (card.link) {
    // Never a bare anchor: a link on a stranger's card opens only after the
    // person has seen exactly where it goes.
    const open = h('button', {
      onclick: (/** @type {Event} */ e) => {
        e.stopPropagation();
        const url = `https://${card.link}`;
        ask(t('link.confirm', new URL(url).host), { yes: t('ask.open'), href: url });
      },
    }, t('back.open'));
    links.push(h('div', { class: 'chip-line' }, h('small', {}, 'web'), h('span', {}, card.link), copy(card.link), open));
  }

  const chain = rec.mine
    ? [t('back.yours')]
    : [t('back.drawn-by', byline(card)), rec.from ? t('back.from', `@${rec.from.nick}`) : null, t('back.to-you')].filter(Boolean);

  const ed = rec.edition;
  const edition = ed ? h('div', { class: 'meta' }, `${t('back.edition', ed.edition, ed.of)} — ${ed.holder === me ? t('back.edition-yours') : t('back.edition-not-yours')}`) : null;

  const others = author ? [...app.cards.values()].filter((e) => e.author?.hex === author.hex && e.rec.digest !== rec.digest).length : 0;
  // Trimmed to the backup's limit first, then saved.
  const note = byteLimit(/** @type {HTMLTextAreaElement} */ (h('textarea', { value: rec.note })), NOTE_BYTES);
  note.addEventListener('input', () => setNote(rec.digest, note.value));
  note.addEventListener('pointerdown', (e) => e.stopPropagation());

  return h('div', { class: 'face back' }, h('div', { class: 'back-in' },
    h('div', {}, h('h3', {}, card.title || '—'), h('div', { class: 'meta' }, byline(card))),
    links.length ? h('div', {}, h('h4', {}, t('back.links')), links) : null,
    h('div', {}, h('h4', {}, t('back.provenance')), h('div', { class: 'meta' }, chain.join(' → ')),
      others ? h('div', { class: 'tech' }, t('back.more', others)) : null),
    h('div', {}, h('h4', {}, t('back.when')), h('div', { class: 'meta' }, date(rec.received, { day: 'numeric', month: 'long', year: 'numeric' }))),
    h('div', {}, h('h4', {}, t('back.authentic')),
      author ? h('div', { class: 'ok-mark' }, `✓ ${t('back.signed')} · ${author.short}`) : h('div', { class: 'bad-mark' }, t('back.unsigned')),
      edition),
    h('div', {}, h('h4', {}, t('back.note')), note, h('div', { class: 'tech' }, t('back.note.local'))),
    h('div', { class: 'tech' }, t('back.tech', card.width, card.height, rec.payload.length)),
  ));
}

