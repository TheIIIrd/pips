// @ts-check
// What the share sheet holds for a card: its code, ready to be scanned.
// An own card goes out signed by its author, as a plain copy or as a
// numbered one; someone else's card goes out as a copy passed on by you.

import { h } from '../dom.js';
import { t } from '../i18n.js';
import { app, subscribe } from '../state.js';
import { qrFor, keepAwake } from '../ui/qr.js';
import { editionPanel } from '../ui/edition.js';
import { segmented } from '../kit/segmented.js';

/**
 * Fills `body` and returns a function that stops what keeps running
 * (a QR carousel, the screen lock).
 * @param {HTMLElement} body @param {import('../state.js').Entry} entry
 */
export function fillShare(body, entry) {
  let stop = () => {};
  let release = () => {};
  keepAwake().then((r) => (release = r));
  const size = Math.min(250, innerWidth * 0.62, innerHeight * 0.34);
  const code = h('div', { class: 'share-code' });

  /** @param {Uint8Array} env @param {string} note */
  const showCode = (env, note) => {
    stop();
    const q = qrFor(env, size);
    stop = q.stop;
    code.replaceChildren(q.el,
      h('p', {}, note),
      h('p', { class: 'small' }, q.plan.kind === 'single' ? t('give.single', env.length) : t('give.carousel', q.plan.fragments)));
  };
  const copy = () => showCode(app.identity.sealAsAuthor(entry.rec.payload, undefined), t('share.hint'));

  const title = h('b', {}, entry.card.title || '—');
  if (entry.rec.mine) {
    const mode = segmented({
      label: t('share.title'), value: 'copy',
      options: [{ value: 'copy', label: t('give.mode.copy') }, { value: 'edition', label: t('give.mode.edition') }],
      onChange: (v) => {
        if (v === 'copy') return copy();
        stop();
        // The panel shows the issue log: drawn again whenever a copy is issued.
        const draw = () => code.replaceChildren(editionPanel(entry.rec.digest));
        draw();
        const unsub = subscribe(draw);
        stop = () => { unsub(); };
      },
    });
    body.replaceChildren(title, mode.el, code);
    copy();
  } else {
    body.replaceChildren(title, code);
    showCode(app.identity.sealAsSender(entry.rec.payload, app.profile.nick || 'pips'), t('give.copy-of', entry.card.author ? `@${entry.card.author}` : '—'));
  }
  return () => { stop(); release(); };
}
