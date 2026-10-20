// @ts-check
// Give: pick one of your cards, and its code is already on screen.

import { h, icon, replace } from '../dom.js';
import { t } from '../i18n.js';
import { app, mine, subscribe } from '../state.js';
import { frame } from './card.js';
import { qrFor, keepAwake } from './qr.js';
import { openImport } from './import.js';
import { editionPanel } from './edition.js';

export function giveView() {
  /** @type {string|null} */
  let selected = null;
  /** @type {'copy'|'edition'} */
  let mode = 'copy';
  let stop = () => {};
  let release = () => {};
  const strip = h('div', { class: 'own-strip', role: 'listbox' });
  const main = h('div', { class: 'give-main' });
  const el = h('section', { class: 'view' },
    h('div', { class: 'bar' }, h('div', { class: 'title' }, h('h1', {}, t('give.title'))),
      h('button', { class: 'icon-btn', 'aria-label': t('give.new'), title: t('give.new'), onclick: openImport }, icon('plus'))),
    strip, main);

  function render() {
    stop();
    const own = mine();
    if (!own.length) {
      strip.replaceChildren();
      replace(main, h('div', { class: 'empty' }, h('b', {}, t('give.empty')), t('give.empty.hint'),
        h('button', { class: 'btn key wide', style: { marginTop: '18px' }, onclick: openImport }, icon('image'), t('give.new'))));
      return;
    }
    if (!selected || !own.some((e) => e.rec.digest === selected)) selected = own[0].rec.digest;
    replace(strip, own.map((e) => h('button', {
      class: 'tile', role: 'option', 'aria-pressed': String(e.rec.digest === selected), 'aria-label': e.card.title,
      onclick: () => { selected = e.rec.digest; render(); },
    }, frame(e, { bare: true }))));

    const entry = /** @type {import('../state.js').Entry} */ (app.cards.get(/** @type {string} */ (selected)));
    const seg = h('div', { class: 'seg', role: 'group' },
      h('button', { 'aria-pressed': String(mode === 'copy'), onclick: () => { mode = 'copy'; render(); } }, t('give.mode.copy')),
      h('button', { 'aria-pressed': String(mode === 'edition'), onclick: () => { mode = 'edition'; render(); } }, t('give.mode.edition')));
    if (mode === 'edition') {
      replace(main, h('div', { class: 'give-card' }, frame(entry)), seg, editionPanel(entry.rec.digest));
      return;
    }
    const envelope = app.identity.sealAsAuthor(entry.rec.payload, undefined);
    const q = qrFor(envelope, Math.min(320, innerWidth - 64));
    stop = q.stop;
    replace(main,
      h('div', { class: 'give-card' }, frame(entry)),
      seg,
      q.el,
      h('p', { class: 'qr-note' }, q.plan.kind === 'single' ? t('give.single', envelope.length) : t('give.carousel', q.plan.fragments)));
  }

  subscribe(() => { if (!el.hidden) render(); });
  return {
    el,
    /** @param {string} [digest] */
    show(digest) {
      if (digest) selected = digest;
      render();
      keepAwake().then((r) => (release = r));
    },
    hide() { stop(); release(); },
  };
}
