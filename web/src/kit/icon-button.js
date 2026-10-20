// @ts-check
// A round button with an icon, a name for screen readers, and an optional
// count in its corner (the archive, the bin).

import { h, icon } from '../dom.js';

/**
 * @param {{icon: Parameters<typeof icon>[0], label: string, onClick: () => void, plain?: boolean}} o
 * plain: no background, for a back arrow at the edge of a bar.
 */
export function iconButton(o) {
  const badge = h('i', { class: 'ib-badge' });
  const el = /** @type {HTMLButtonElement} */ (h('button', {
    type: 'button', class: `ib${o.plain ? ' plain' : ''}`, 'aria-label': o.label, title: o.label, onclick: o.onClick,
  }, icon(o.icon), badge));
  return {
    el,
    /** @param {number} n 0 hides it */
    count(n) { badge.textContent = n ? String(n) : ''; },
  };
}
