// @ts-check
// The frame around the views: bottom tabs, view switching, toasts.

import { h, icon } from '../dom.js';
import { t } from '../i18n.js';

/** @typedef {{ el: HTMLElement, show?: (arg?: any) => void, hide?: () => void }} View */

/** @type {Record<string, View>} */
const views = {};
let current = '';
/** @type {HTMLElement} */
let tabs;

const TABS = /** @type {const} */ (['collection', 'give', 'receive', 'me']);

/** @param {HTMLElement} root @param {Record<string, View>} all */
export function mountShell(root, all) {
  Object.assign(views, all);
  for (const v of Object.values(views)) {
    v.el.hidden = true;
    root.append(v.el);
  }
  tabs = h('nav', { class: 'tabs', 'aria-label': 'Pips' },
    TABS.map((name) => h('button', { dataset: { tab: name }, onclick: () => go(name) }, icon(name), t(`tab.${name}`))));
  root.append(tabs, toastEl);
}

/** Shows a view; `arg` is passed to its `show`. @param {string} name @param {any} [arg] */
export function go(name, arg) {
  if (current && current !== name) {
    views[current].hide?.();
    views[current].el.hidden = true;
  }
  if (current !== name) toastEl.hidden = true;
  current = name;
  views[name].el.hidden = false;
  views[name].show?.(arg);
  for (const b of tabs.querySelectorAll('button')) {
    if (/** @type {HTMLElement} */ (b).dataset.tab === name) b.setAttribute('aria-current', 'page');
    else b.removeAttribute('aria-current');
  }
}

export const currentView = () => current;

const toastText = h('span');
const toastAction = h('button', { class: 'text-btn' });
const toastEl = h('div', { class: 'toast', role: 'status', hidden: true }, toastText, toastAction);
/** @type {number|undefined} */
let toastTimer;

/** @param {string} text @param {{label: string, run: () => void}} [action] */
export function toast(text, action) {
  toastText.textContent = text;
  toastAction.hidden = !action;
  toastAction.textContent = action?.label ?? '';
  toastAction.onclick = () => {
    toastEl.hidden = true;
    action?.run();
  };
  // Over an open card the bottom belongs to its buttons: the toast moves up.
  toastEl.classList.toggle('high', document.querySelector('.overlay') != null);
  toastEl.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (toastEl.hidden = true), action ? 5000 : 2400);
}

/**
 * Asks before doing something. Built into the page: native confirm() looks
 * foreign in an installed app and is refused outright in some frames.
 * With `href`, the "yes" button is a real link, so opening it is the
 * person's own click rather than script.
 * @param {string} text @param {{yes: string, no?: string, danger?: boolean, href?: string}} opts
 * @returns {Promise<boolean>}
 */
export function ask(text, opts) {
  return new Promise((resolve) => {
    const finish = (/** @type {boolean} */ v) => { box.remove(); document.removeEventListener('keydown', onKey); resolve(v); };
    const onKey = (/** @type {KeyboardEvent} */ e) => { if (e.key === 'Escape') finish(false); };
    const yesCls = `btn ${opts.danger ? 'warn' : 'key'}`;
    const yes = opts.href
      ? h('a', { class: yesCls, href: opts.href, target: '_blank', rel: 'noopener noreferrer', onclick: () => finish(true) }, opts.yes)
      : h('button', { class: yesCls, onclick: () => finish(true) }, opts.yes);
    const box = h('div', { class: 'dialog', role: 'alertdialog', 'aria-modal': 'true', onclick: (/** @type {Event} */ e) => { if (e.target === box) finish(false); } },
      h('div', { class: 'dialog-box' }, h('p', {}, text),
        h('div', { class: 'row-btns' }, h('button', { class: 'btn', onclick: () => finish(false) }, opts.no ?? t('ask.cancel')), yes)));
    document.addEventListener('keydown', onKey);
    document.body.append(box);
    /** @type {HTMLElement} */ (yes).focus();
  });
}
