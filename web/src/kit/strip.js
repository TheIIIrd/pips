// @ts-check
// A plain dark strip that rises from the bottom with the latest arrivals of
// some place, newest first. A newcomer takes the first slot and the others
// move one along; `room` makes space before it lands.

import { h } from '../dom.js';

const SLOT = 64;

/** @typedef {{id: string, make: () => HTMLElement}} StripItem */

export function strip() {
  const title = h('b');
  const count = h('span');
  const row = h('div', { class: 'strip-row' });
  const empty = h('div', { class: 'strip-empty' });
  const el = h('div', { class: 'strip', 'aria-hidden': 'true' }, h('div', { class: 'strip-head' }, title, count), row, empty);
  /** @type {Map<string, HTMLElement>} */
  const minis = new Map();

  /** 0 hidden … 1 up. @param {number} f @param {boolean} [anim] */
  function reveal(f, anim = false) {
    el.classList.toggle('anim', anim);
    el.style.transform = `translateY(${(1 - f) * 101}%)`;
  }

  /**
   * @param {StripItem[]} items newest first
   * @param {{room?: number, landed?: string, title: string, count: string, empty: string}} o
   * landed: the item that has just touched down, shown in place at once.
   */
  function draw(items, o) {
    title.textContent = o.title;
    count.textContent = o.count;
    const room = o.room ?? 0;
    empty.textContent = o.empty;
    empty.hidden = !!(items.length || room);
    const fit = Math.max(1, Math.floor((row.clientWidth + 8) / SLOT));
    for (const [id, m] of minis) if (!items.some((i) => i.id === id)) { m.remove(); minis.delete(id); }
    items.forEach((it, k) => {
      let m = minis.get(it.id);
      if (!m) {
        m = h('div', { class: 'mini' }, it.make());
        row.append(m);
        minis.set(it.id, m);
        if (it.id === o.landed) {
          m.style.transition = 'none';
          m.style.transform = `translateX(${(k + room) * SLOT}px)`;
          void m.offsetWidth;
          m.style.transition = '';
        } else {
          m.style.transform = `translateX(${-SLOT}px)`;
          m.style.opacity = '0';
          void m.offsetWidth;
        }
      }
      const slot = k + room;
      m.style.transform = `translateX(${slot * SLOT}px)`;
      m.style.opacity = slot < fit ? '1' : '0';
    });
  }

  /** Where slot `k` is on screen once the strip is fully up. @param {number} k */
  function slot(k) {
    const r = row.getBoundingClientRect();
    const shift = el.getBoundingClientRect().top - (innerHeight - el.offsetHeight);
    return { x: r.left + k * SLOT, y: r.top - shift, w: 56 };
  }

  reveal(0);
  return { el, reveal, draw, slot };
}
