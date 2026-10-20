// @ts-check
// A search field with a clear button.

import { h, icon } from '../dom.js';

/** @param {{placeholder: string, clearLabel: string, onInput: (v: string) => void}} o */
export function searchField(o) {
  const input = /** @type {HTMLInputElement} */ (h('input', {
    type: 'search', placeholder: o.placeholder, autocomplete: 'off', enterkeyhint: 'search', spellcheck: false,
    oninput: () => { clear.hidden = !input.value; o.onInput(input.value); },
    onkeydown: (/** @type {KeyboardEvent} */ e) => { if (e.key === 'Enter') input.blur(); },
  }));
  const clear = h('button', {
    type: 'button', class: 'clear', 'aria-label': o.clearLabel, hidden: true,
    onclick: () => { set(''); o.onInput(''); },
  }, icon('close'));
  const el = h('label', { class: 'search-field' }, icon('search'), input, clear);
  /** @param {string} v */
  function set(v) {
    input.value = v;
    clear.hidden = !v;
  }
  return { el, set };
}
