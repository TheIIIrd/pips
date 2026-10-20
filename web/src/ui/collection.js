// @ts-check
// The collection: a continuous grid, labelled by week, or by month when
// pinched out. Weeks are labels inside the flow, not rows of their own, so
// a sparse week never leaves a row of empty space.

import { h, icon, replace } from '../dom.js';
import { t, date, language } from '../i18n.js';
import { collection, pile, daysLeft, subscribe } from '../state.js';
import { frame } from './card.js';
import { openViewer } from './viewer.js';

/** @typedef {'months'|'weeks'} Level */
const LEVELS = /** @type {Level[]} */ (['months', 'weeks']);

export function collectionView() {
  /** @type {Level} */
  let level = 'weeks';
  const count = h('span', { class: 'sub' });
  const badge = h('span', { class: 'badge' });
  const body = h('div');
  const scroll = h('div', { class: 'scroll' }, body);
  const chip = h('div', { class: 'level-chip faded', role: 'group' },
    LEVELS.map((l) => h('button', { dataset: { level: l }, onclick: () => setLevel(l, true) }, t(`level.${l}`))));
  const el = h('section', { class: 'view' },
    h('div', { class: 'bar' },
      h('div', { class: 'title' }, h('h1', {}, t('collection.title')), count),
      h('button', { class: 'icon-btn', 'aria-label': t('archive.title'), onclick: () => openArchive() }, icon('archive'), badge)),
    chip, scroll);

  /** @type {number|undefined} */
  let chipTimer;
  const showChip = () => {
    chip.classList.remove('faded');
    chip.querySelectorAll('button').forEach((b) => b.setAttribute('aria-pressed', String(/** @type {HTMLElement} */ (b).dataset.level === level)));
    clearTimeout(chipTimer);
    chipTimer = setTimeout(() => chip.classList.add('faded'), 1600);
  };

  /** @param {Level} l @param {boolean} [_fromChip] */
  function setLevel(l, _fromChip) {
    if (l === level) return showChip();
    // Tiles glide from where they were to where they land.
    const before = new Map([...body.querySelectorAll('.tile')].map((n) => [/** @type {HTMLElement} */ (n).dataset.digest, n.getBoundingClientRect()]));
    level = l;
    render();
    for (const n of body.querySelectorAll('.tile')) {
      const a = before.get(/** @type {HTMLElement} */ (n).dataset.digest);
      const b = n.getBoundingClientRect();
      if (!a || !b.width) continue;
      n.animate([{ transform: `translate(${a.left - b.left}px, ${a.top - b.top}px) scale(${a.width / b.width})`, transformOrigin: '0 0' },
        { transform: 'none', transformOrigin: '0 0' }], { duration: 280, easing: 'cubic-bezier(.3,.7,.2,1)' });
    }
    showChip();
  }

  function render() {
    const cards = collection();
    count.textContent = t('collection.count', cards.length);
    const stored = pile('archive').length + pile('trash').length;
    badge.textContent = String(stored);
    badge.hidden = !stored;
    if (!cards.length) {
      replace(body, h('div', { class: 'empty' }, h('b', {}, t('collection.empty')), t('collection.empty.hint')));
      return;
    }
    const digests = cards.map((e) => e.rec.digest);
    let last = '';
    const grid = h('div', { class: 'grid', dataset: { level } }, cards.map((e, i) => {
      const key = level === 'weeks' ? weekKey(e.rec.received) : monthKey(e.rec.received);
      const label = key !== last ? (level === 'weeks' ? weekLabel(e.rec.received) : monthLabel(e.rec.received)) : null;
      last = key;
      return h('button', {
        class: 'tile', dataset: { digest: e.rec.digest, ...(label ? { label } : {}) },
        'aria-label': e.card.title, onclick: () => openViewer(digests.slice(), i, 'collection'),
      }, frame(e, { bare: level === 'months' }));
    }));
    replace(body, grid);
  }

  // Pinch (two pointers) and ctrl+wheel (a trackpad pinch) move along the axis.
  /** @type {Map<number, PointerEvent>} */
  const pts = new Map();
  let d0 = 0, fired = false;
  scroll.addEventListener('pointerdown', (e) => {
    if (e.pointerType !== 'touch') return;
    pts.set(e.pointerId, e);
    if (pts.size === 2) { d0 = dist(); fired = false; }
  });
  scroll.addEventListener('pointermove', (e) => {
    if (!pts.has(e.pointerId)) return;
    pts.set(e.pointerId, e);
    if (pts.size !== 2 || fired) return;
    const r = dist() / d0;
    if (r < 0.8) { fired = true; setLevel('months'); }
    if (r > 1.25) { fired = true; setLevel('weeks'); }
  });
  const up = (/** @type {PointerEvent} */ e) => pts.delete(e.pointerId);
  scroll.addEventListener('pointerup', up);
  scroll.addEventListener('pointercancel', up);
  const dist = () => { const [a, b] = [...pts.values()]; return Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY); };
  let acc = 0;
  scroll.addEventListener('wheel', (e) => {
    if (!e.ctrlKey) return;
    e.preventDefault();
    acc += e.deltaY;
    if (Math.abs(acc) < 40) return;
    setLevel(acc > 0 ? 'months' : 'weeks');
    acc = 0;
  }, { passive: false });

  subscribe(() => { if (!el.hidden) render(); });
  return { el, show: render };
}

/** Archive and bin, as one screen with two tabs. */
function openArchive() {
  /** @type {'archive'|'trash'} */
  let which = 'archive';
  const note = h('p', { class: 'hint' });
  const body = h('div');
  const seg = h('div', { class: 'seg', role: 'group' });
  const close = () => { unsub(); overlay.remove(); };
  const overlay = h('section', { class: 'overlay', role: 'dialog' },
    h('div', { class: 'bar' }, h('button', { class: 'icon-btn', 'aria-label': 'back', onclick: close }, icon('back')),
      h('div', { class: 'title' }, h('h1', {}, t('archive.title')))),
    seg, h('div', { class: 'scroll' }, note, body));
  const paint = () => {
    seg.replaceChildren(...['archive', 'trash'].map((k) => h('button', {
      'aria-pressed': String(k === which), onclick: () => { which = /** @type {any} */ (k); paint(); },
    }, `${t(k === 'archive' ? 'archive.title' : 'archive.trash')} · ${pile(/** @type {any} */ (k)).length}`)));
    note.textContent = t(which === 'archive' ? 'archive.note' : 'trash.note');
    const items = pile(which);
    const digests = items.map((e) => e.rec.digest);
    replace(body, items.length
      ? h('div', { class: 'grid' }, items.map((e, i) => h('button', {
        class: 'tile', dataset: { digest: e.rec.digest, ...(which === 'trash' ? { label: `${daysLeft(e.rec)} d` } : {}) },
        onclick: () => openViewer(digests.slice(), i, which),
      }, frame(e))))
      : h('div', { class: 'empty' }, t('archive.empty')));
  };
  const unsub = subscribe(paint);
  document.getElementById('app')?.append(overlay);
  paint();
}

// ---------------------------------------------------------------- dates --

/** Monday-based week start, local time. @param {number} ms */
function weekStart(ms) {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  d.setDate(d.getDate() - ((d.getDay() + 6) % 7));
  return d;
}
/** @param {number} ms */
const weekKey = (ms) => weekStart(ms).toDateString();
/** @param {number} ms */
const monthKey = (ms) => { const d = new Date(ms); return `${d.getFullYear()}-${d.getMonth()}`; };
/** @param {number} ms */
function weekLabel(ms) {
  const a = weekStart(ms), b = new Date(a);
  b.setDate(b.getDate() + 6);
  const sameMonth = a.getMonth() === b.getMonth();
  const end = date(b.getTime(), { day: 'numeric', month: 'short' });
  return sameMonth ? `${a.getDate()}–${end}` : `${date(a.getTime(), { day: 'numeric', month: 'short' })} – ${end}`;
}
/** @param {number} ms */
function monthLabel(ms) {
  const d = new Date(ms);
  const opts = /** @type {Intl.DateTimeFormatOptions} */ ({ month: 'long', ...(d.getFullYear() !== new Date().getFullYear() ? { year: 'numeric' } : {}) });
  const s = d.toLocaleDateString(language() === 'ru' ? 'ru-RU' : 'en-GB', opts);
  return s.charAt(0).toUpperCase() + s.slice(1);
}
