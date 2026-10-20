// @ts-check
// A sheet that comes down from the top, over a dimmed screen. It can follow
// a finger (`follow`) before it opens for good, and goes away with a swipe
// up, a tap on the dimmed screen or Escape.

import { h } from '../dom.js';

/** @param {{label: string, onClose?: () => void}} o */
export function topSheet(o) {
  const body = h('div', { class: 'top-sheet-body' });
  const el = h('div', { class: 'top-sheet', role: 'dialog', 'aria-label': o.label, 'aria-hidden': 'true' }, body, h('div', { class: 'grab' }));
  const scrim = h('div', { class: 'scrim', onclick: () => close() });
  let open = false;

  /** 0 hidden … 1 shown. @param {number} f @param {boolean} [anim] */
  function follow(f, anim = false) {
    el.classList.toggle('anim', anim);
    scrim.classList.toggle('anim', anim);
    el.style.transform = `translateY(${-(1 - f) * 101}%)`;
    scrim.style.opacity = String(f * 0.45);
  }
  function show() {
    open = true;
    el.setAttribute('aria-hidden', 'false');
    scrim.classList.add('on');
    follow(1, true);
    document.addEventListener('keydown', onKey);
  }
  function close() {
    if (!open) return follow(0, true);
    open = false;
    el.setAttribute('aria-hidden', 'true');
    scrim.classList.remove('on');
    follow(0, true);
    document.removeEventListener('keydown', onKey);
    o.onClose?.();
  }
  const onKey = (/** @type {KeyboardEvent} */ e) => { if (e.key === 'Escape') close(); };
  // A swipe up puts it away.
  let y0 = /** @type {number|null} */ (null);
  el.addEventListener('pointerdown', (e) => { y0 = e.clientY; });
  el.addEventListener('pointerup', (e) => { if (y0 != null && e.clientY - y0 < -30) close(); y0 = null; });

  follow(0);
  return {
    /** Both parts, to be placed in the page: the dim layer, then the sheet. */
    els: [scrim, el],
    body, follow, show, close,
    get open() { return open; },
    /** Colours the sheet like a card's paper. @param {{paper: string, ink: string, dim: string}} s */
    paint(s) {
      el.style.setProperty('--shade', s.paper);
      el.style.setProperty('--shade-ink', s.ink);
      el.style.setProperty('--shade-dim', s.dim);
    },
  };
}
