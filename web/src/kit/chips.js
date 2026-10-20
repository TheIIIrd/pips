// @ts-check
// A row of toggle chips that scrolls sideways: tags to filter by.

import { h } from '../dom.js';

/** @param {{label: string, onToggle: (value: string) => void}} o */
export function chips(o) {
  const el = h('div', { class: 'chips', role: 'group', 'aria-label': o.label });
  return {
    el,
    /** @param {{value: string, text: string, count?: number, on: boolean}[]} items */
    update(items) {
      el.replaceChildren(...items.map((c) => h('button', {
        type: 'button', 'aria-pressed': String(c.on), onclick: () => o.onToggle(c.value),
      }, c.text, c.count != null ? h('i', {}, String(c.count)) : null)));
      el.hidden = !items.length;
    },
  };
}
