// @ts-check
// A row of actions at the bottom of the screen that slides in and out:
// what can be done with the chosen cards, or with the card in front.

import { h, icon } from '../dom.js';

/** @typedef {{id: string, label: string, icon?: Parameters<typeof icon>[0], kind?: 'key'|'warn', run: () => void}} Action */

/** @param {{label: string, compact?: boolean}} o compact: icons with small captions */
export function actionBar(o) {
  const el = h('div', { class: `action-bar${o.compact ? ' compact' : ''}`, role: 'toolbar', 'aria-label': o.label });
  let shown = false;
  return {
    el,
    /** @param {Action[]} actions */
    set(actions) {
      el.replaceChildren(...actions.map((a) => h('button', {
        type: 'button', class: `btn${a.kind ? ` ${a.kind}` : ''}`, dataset: { act: a.id }, onclick: a.run,
      }, a.icon ? icon(a.icon) : null, h('span', {}, a.label))));
    },
    /** @param {boolean} on */
    show(on) {
      if (on === shown) return;
      shown = on;
      el.classList.toggle('up', on);
      el.inert = !on;
    },
  };
}
