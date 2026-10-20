// @ts-check
// First run: create a key (and write down its words) or restore one.
// Nothing is stored until the person has confirmed the words.

import { h, replace, byteLimit } from '../dom.js';
import { t } from '../i18n.js';
import { app, setIdentity, setProfile } from '../state.js';

/** @param {HTMLElement} root @param {() => void} done */
export function onboarding(root, done) {
  const page = h('div', { class: 'onb' });
  root.append(page);

  const logo = () => {
    const cv = /** @type {HTMLCanvasElement} */ (h('canvas', { class: 'logo', width: 8, height: 8 }));
    const g = /** @type {CanvasRenderingContext2D} */ (cv.getContext('2d'));
    const rows = ['..XXXX..', '.X....X.', 'X.X..X.X', 'X......X', 'X.X..X.X', 'X..XX..X', '.X....X.', '..XXXX..'];
    rows.forEach((r, y) => [...r].forEach((c, x) => { if (c === 'X') { g.fillStyle = '#6d8fd6'; g.fillRect(x, y, 1, 1); } }));
    return cv;
  };

  function welcome() {
    replace(page, logo(), h('h1', {}, t('onb.title')), h('p', {}, t('onb.lead')), h('div', { class: 'grow' }),
      h('button', { class: 'btn key wide', onclick: create }, t('onb.create')),
      h('div', { style: { height: '8px' } }),
      h('button', { class: 'btn wide', onclick: restore }, t('onb.restore')));
  }

  function create() {
    const identity = app.core.Identity.fromSeed(crypto.getRandomValues(new Uint8Array(32)));
    const next = /** @type {HTMLButtonElement} */ (h('button', { class: 'btn key wide', disabled: true, onclick: () => name(identity) }, t('onb.next')));
    const ack = /** @type {HTMLInputElement} */ (h('input', { type: 'checkbox', onchange: () => (next.disabled = !ack.checked) }));
    replace(page, h('h1', {}, t('onb.words.title')), h('p', {}, t('onb.words.lead')),
      h('div', { class: 'word-grid' }, identity.words().split(' ').map((/** @type {string} */ w) => h('span', {}, w))),
      h('label', { class: 'check' }, ack, t('onb.words.ack')), h('div', { class: 'grow' }), next);
  }

  function restore() {
    const words = /** @type {HTMLTextAreaElement} */ (h('textarea', { class: 'words', autocapitalize: 'none', autocomplete: 'off', spellcheck: false }));
    const error = h('div', { class: 'error' });
    const go = () => {
      try {
        name(app.core.Identity.fromWords(words.value));
      } catch (e) {
        error.textContent = /** @type {any} */ (e).code === 'bad_words' ? `${t('err.bad_words')} (${/** @type {any} */ (e).message})` : t('err.generic');
      }
    };
    replace(page, h('h1', {}, t('onb.restore.title')), h('p', {}, t('onb.restore.lead')), words, error,
      h('div', { class: 'grow' }), h('button', { class: 'btn key wide', onclick: go }, t('onb.restore.go')),
      h('div', { style: { height: '8px' } }), h('button', { class: 'btn wide', onclick: welcome }, '←'));
  }

  /** @param {any} identity */
  function name(identity) {
    const nick = byteLimit(/** @type {HTMLInputElement} */ (h('input', { autocomplete: 'nickname', placeholder: 'kitsu' })), 24);
    const finish = async () => {
      await setIdentity(identity);
      await setProfile({ nick: nick.value.trim(), handle: null, link: '' });
      page.remove();
      done();
    };
    replace(page, h('h1', {}, t('onb.nick.title')), h('p', {}, t('onb.nick.lead')),
      h('div', { class: 'field' }, nick), h('div', { class: 'grow' }), h('button', { class: 'btn key wide', onclick: finish }, t('onb.done')));
    nick.focus();
  }

  welcome();
}
