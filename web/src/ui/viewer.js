// @ts-check
// One card, large: front and back, and what can be done with it. Opened
// from the collection, the archive and the bin, each with its own actions.

import { h, icon } from '../dom.js';
import { t } from '../i18n.js';
import { app, move, purge } from '../state.js';
import { frame } from './card.js';
import { qrFor, keepAwake } from './qr.js';
import { go, toast, ask } from './shell.js';

/** @typedef {'collection'|'archive'|'trash'} Mode */

/** @param {string[]} digests @param {number} index @param {Mode} mode */
export function openViewer(digests, index, mode) {
  let at = index;
  let turned = false;
  const stage = h('div', { class: 'viewer-stage' });
  const pos = h('span', { class: 'sub' });
  const seg = h('div', { class: 'seg', role: 'group' });
  const acts = h('div', { class: 'actions' });
  const close = () => {
    overlay.remove();
    document.removeEventListener('keydown', onKey);
  };
  const overlay = h('section', { class: 'overlay', role: 'dialog', 'aria-modal': 'true' },
    h('div', { class: 'bar' },
      h('button', { class: 'icon-btn', 'aria-label': 'close', onclick: close }, icon('down')),
      h('div', { class: 'title' }, h('h1', {}, t(mode === 'collection' ? 'collection.title' : mode === 'archive' ? 'archive.title' : 'archive.trash')), pos)),
    stage, seg, acts);

  const flipTo = (/** @type {boolean} */ v) => {
    turned = v;
    stage.querySelector('.flip')?.classList.toggle('turned', turned);
    seg.querySelectorAll('button').forEach((b, i) => b.setAttribute('aria-pressed', String(i === (turned ? 1 : 0))));
  };
  seg.append(
    h('button', { onclick: () => flipTo(false) }, t('viewer.front')),
    h('button', { onclick: () => flipTo(true) }, t('viewer.back')));

  const paint = () => {
    const entry = app.cards.get(digests[at]);
    if (!entry) return close();
    const f = frame(entry, { back: true });
    f.addEventListener('click', () => flipTo(!turned));
    stage.replaceChildren(f);
    pos.textContent = digests.length > 1 ? `${at + 1} / ${digests.length}` : '';
    flipTo(false);
    acts.replaceChildren(...actions(entry));
  };

  /** Removes the current card from this viewer after it moved elsewhere. */
  const drop = () => {
    digests.splice(at, 1);
    if (!digests.length) return close();
    at = Math.min(at, digests.length - 1);
    paint();
  };

  /** @param {import('../state.js').Entry} e */
  function actions(e) {
    const d = e.rec.digest;
    const btn = (/** @type {string} */ label, /** @type {string} */ cls, /** @type {() => void} */ run, /** @type {any} */ ic) =>
      h('button', { class: `btn wide ${cls}`, onclick: run }, ic ? icon(ic) : null, label);
    if (mode === 'collection') {
      return [
        e.rec.mine
          ? btn(t('viewer.give'), 'key', () => { close(); go('give', d); }, 'give')
          : btn(t('viewer.share-copy'), 'key', () => shareCopy(e), 'give'),
        btn(t('viewer.to-archive'), '', () => {
          move(d, 'archive');
          drop();
          toast(t('toast.archived'), { label: t('toast.undo'), run: () => move(d, 'collection') });
        }, 'archive'),
      ];
    }
    if (mode === 'archive') {
      return [
        btn(t('viewer.restore'), 'key', () => { move(d, 'collection'); drop(); toast(t('toast.restored')); }, 'restore'),
        btn(t('viewer.to-trash'), 'warn', () => {
          move(d, 'trash');
          drop();
          toast(t('toast.trashed'), { label: t('toast.undo'), run: () => move(d, 'archive') });
        }, 'trash'),
      ];
    }
    return [
      btn(t('viewer.untrash'), 'key', () => { move(d, 'collection'); drop(); toast(t('toast.restored')); }, 'restore'),
      btn(t('viewer.purge'), 'warn', async () => {
        if (!(await ask(t('confirm.purge'), { yes: t('ask.delete'), danger: true }))) return;
        purge(d);
        drop();
        toast(t('toast.purged'));
      }, 'trash'),
    ];
  }

  // Sideways swipe pages through the pile; down closes.
  let sx = 0, sy = 0, tracking = false;
  stage.addEventListener('pointerdown', (e) => { sx = e.clientX; sy = e.clientY; tracking = true; });
  stage.addEventListener('pointerup', (e) => {
    if (!tracking) return;
    tracking = false;
    const dx = e.clientX - sx, dy = e.clientY - sy;
    if (Math.abs(dx) > 60 && Math.abs(dx) > Math.abs(dy)) step(dx < 0 ? 1 : -1);
    else if (dy > 110 && Math.abs(dy) > Math.abs(dx)) close();
  });
  const step = (/** @type {number} */ d) => {
    const n = Math.max(0, Math.min(digests.length - 1, at + d));
    if (n !== at) { at = n; paint(); }
  };
  const onKey = (/** @type {KeyboardEvent} */ e) => {
    if (e.target instanceof HTMLTextAreaElement) return;
    if (e.key === 'Escape') close();
    else if (e.key === 'ArrowLeft') step(-1);
    else if (e.key === 'ArrowRight') step(1);
    else if (e.key === ' ') { e.preventDefault(); flipTo(!turned); }
  };
  document.addEventListener('keydown', onKey);

  document.getElementById('app')?.append(overlay);
  paint();
}

/** Someone else's card, passed on as a copy signed with our name. @param {import('../state.js').Entry} e */
function shareCopy(e) {
  const env = app.identity.sealAsSender(e.rec.payload, app.profile.nick || 'pips');
  const q = qrFor(env, Math.min(340, innerWidth - 64));
  let release = () => {};
  keepAwake().then((r) => (release = r));
  const close = () => { q.stop(); release(); sheet.remove(); };
  const sheet = h('section', { class: 'overlay', role: 'dialog' },
    h('div', { class: 'bar' }, h('button', { class: 'icon-btn', 'aria-label': 'close', onclick: close }, icon('close')),
      h('div', { class: 'title' }, h('h1', {}, e.card.title || '—'))),
    h('div', { class: 'give-main' }, q.el,
      h('p', { class: 'qr-note' }, t('give.copy-of', e.card.author ? `@${e.card.author}` : '—')),
      h('p', { class: 'qr-note' }, q.plan.kind === 'single' ? t('give.single', env.length) : t('give.carousel', q.plan.fragments))));
  document.getElementById('app')?.append(sheet);
}
