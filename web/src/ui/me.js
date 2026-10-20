// @ts-check
// Me: identity, what goes on the back of your cards, secrets, settings.

import { h, icon, replace, byteLimit } from '../dom.js';
import { t, language, setLanguage, date } from '../i18n.js';
import { app, setProfile, addOwn, receive, backdate, makeBackup, restoreBackup, lastBackup } from '../state.js';
import { isPersisted, setMeta, memoryOnly } from '../store.js';
import { toast, ask, go } from './shell.js';
import { qrText } from './qr.js';
import { isDevHost } from '../core.js';
import { identicon } from '../kit/identicon.js';

const NETWORKS = ['telegram', 'instagram', 'x', 'bluesky', 'mastodon', 'vk', 'tiktok', 'other'];

export function meView() {
  const body = h('div');
  const el = h('section', { class: 'view' },
    h('div', { class: 'bar' }, h('div', { class: 'title' }, h('h1', {}, t('me.title')))),
    h('div', { class: 'scroll' }, body));

  async function render() {
    const key = app.identity.publicKey();
    const p = app.profile;
    const nick = byteLimit(/** @type {HTMLInputElement} */ (h('input', { value: p.nick, autocomplete: 'nickname' })), 24);
    const network = /** @type {HTMLSelectElement} */ (h('select', {}, NETWORKS.map((n) => h('option', { value: n, selected: p.handle?.network === n }, n))));
    const handle = byteLimit(/** @type {HTMLInputElement} */ (h('input', { value: p.handle?.text ?? '', autocomplete: 'off' })), 48);
    const link = byteLimit(/** @type {HTMLInputElement} */ (h('input', { value: p.link, placeholder: 'kitsu.art', autocomplete: 'off', inputMode: 'url' })), 64);
    const error = h('div', { class: 'error' });
    const secret = h('div', { class: 'secret' });
    const lang = /** @type {HTMLSelectElement} */ (h('select', {
      onchange: async () => {
        await setMeta('language', lang.value);
        // In memory-only mode a reload would lose everything: apply on next start.
        if (memoryOnly) toast(lang.value === 'ru' ? 'Язык сменится при следующем запуске' : 'The language changes on the next start');
        else location.reload();
      },
    }, h('option', { value: 'ru', selected: language() === 'ru' }, 'Русский'), h('option', { value: 'en', selected: language() === 'en' }, 'English')));
    const persisted = await isPersisted();

    const save = async () => {
      const next = {
        nick: nick.value.trim(),
        handle: handle.value.trim() ? { network: network.value, text: handle.value.trim() } : null,
        link: normalLink(link.value),
      };
      // The core is the judge of what a card may carry.
      try {
        app.core.shareSize({ ...probe, author: next.nick, handle: next.handle, link: next.link });
      } catch (e) {
        error.textContent = /** @type {any} */ (e).message ?? t('err.generic');
        return;
      }
      error.textContent = '';
      await setProfile(next);
      toast(t('me.saved'));
    };

    // Each button shows its secret and, pressed again, hides it; only one
    // is open at a time, and leaving the tab hides both.
    /** @type {'words'|'paper'|null} */
    let shown = null;
    const wordsBtn = h('button', { class: 'btn', onclick: () => reveal('words') });
    const paperBtn = h('button', { class: 'btn', onclick: () => reveal('paper') });
    const labels = () => {
      wordsBtn.replaceChildren(icon('key'), t(shown === 'words' ? 'me.words.hide' : 'me.words'));
      paperBtn.replaceChildren(t(shown === 'paper' ? 'me.paper.hide' : 'me.paper'));
    };
    labels();
    const reveal = async (/** @type {'words'|'paper'} */ what) => {
      if (shown === what) {
        shown = null;
        secret.replaceChildren();
        return labels();
      }
      if (!(await ask(t('me.secret.confirm'), { yes: t('ask.show') }))) return;
      shown = what;
      if (what === 'words') {
        replace(secret, h('div', { class: 'word-grid' }, app.identity.words().split(' ').map((w) => h('span', {}, w))));
      } else {
        // The QR is what gets printed; the text stays folded away unless asked for.
        const paper = app.identity.paper();
        replace(secret, qrText(paper, 240),
          h('details', { class: 'paper-text' }, h('summary', {}, t('me.paper.text')), h('code', {}, paper)));
      }
      labels();
    };

    replace(body,
      h('div', { class: 'idcard' }, identicon(key.hex), h('div', {}, h('b', {}, p.nick || '—'), h('small', {}, `${t('me.key')} ${key.short}`))),
      h('div', { class: 'group' }, h('h2', {}, t('me.card-back')),
        h('div', { class: 'field' }, h('label', {}, t('me.nick')), nick),
        h('div', { class: 'field' }, h('label', {}, t('me.network')), network),
        h('div', { class: 'field' }, h('label', {}, t('me.handle')), handle),
        h('div', { class: 'field' }, h('label', {}, t('me.link')), h('span', { class: 'prefix', 'aria-hidden': 'true' }, 'https://'), link),
        h('p', { class: 'hint' }, t('me.link.hint')), error,
        h('button', { class: 'btn key wide', onclick: save }, t('me.save'))),
      h('div', { class: 'group' }, h('h2', {}, t('me.key')),
        h('div', { class: 'row-btns' }, wordsBtn, paperBtn),
        secret),
      await backupGroup(),
      h('div', { class: 'group' }, h('h2', {}, t('me.language')), h('div', { class: 'field' }, lang)),
      h('div', { class: 'group' }, h('h2', {}, t('me.storage')),
        h('p', { class: 'hint' }, t(memoryOnly ? 'me.storage.memory' : persisted ? 'me.storage.persisted' : 'me.storage.not-persisted'))),
      app.stub || (isDevHost() && new URLSearchParams(location.search).has('dev'))
        ? h('div', { class: 'group' }, h('h2', {}, t('me.dev')), h('button', {
          class: 'btn wide',
          onclick: async (/** @type {Event} */ e) => {
            const b = /** @type {HTMLButtonElement} */ (e.currentTarget);
            b.disabled = true;
            try {
              toast(t('me.dev.seeded', await seed()));
              go('collection');
            } catch (err) {
              toast(`${t('err.generic')}: ${/** @type {any} */ (err)?.message ?? err}`);
            } finally {
              b.disabled = false;
            }
          },
        }, t('me.dev.seed')))
        : null);
  }

  return { el, show: render };
}

/** Backup: save, restore, and how stale the last copy is. The group redraws
 * itself after each action, so its status is never out of date. */
async function backupGroup() {
  const box = h('div', { class: 'group' });
  const input = /** @type {HTMLInputElement} */ (h('input', { type: 'file', accept: '.pipsbak,application/octet-stream', hidden: true }));
  input.addEventListener('change', async () => {
    const f = input.files?.[0];
    input.value = '';
    if (!f) return;
    try {
      const r = await restoreBackup(new Uint8Array(await f.arrayBuffer()));
      toast(t('backup.restored', r.added, r.known));
    } catch (e) {
      toast(backupError(/** @type {any} */ (e)));
    }
    await draw();
  });

  const save = async () => {
    try {
      const { bytes } = await makeBackup();
      const day = new Date().toISOString().slice(0, 10);
      const a = /** @type {HTMLAnchorElement} */ (h('a', { href: URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: 'application/octet-stream' })), download: `pips-${day}.pipsbak` }));
      document.body.append(a);
      a.click();
      a.remove();
      setTimeout(() => URL.revokeObjectURL(a.href), 10_000);
      toast(t('backup.saved'));
    } catch (e) {
      toast(backupError(/** @type {any} */ (e)));
    }
    await draw();
  };

  async function draw() {
    const last = await lastBackup();
    const total = app.cards.size;
    const fresh = last ? Math.max(0, total - last.count) : total;
    const stale = total > 0 && (!last || fresh >= 10);
    const status = !last
      ? t('backup.never')
      : `${t('backup.last', date(last.at, { day: 'numeric', month: 'long', year: 'numeric' }), last.count)}${fresh ? ` · ${t('backup.since', fresh)}` : ''}`;
    box.replaceChildren(h('h2', {}, t('backup.title')),
      h('p', { class: `hint${stale ? ' warn-text' : ''}`, style: { marginTop: '0' } }, status),
      h('div', { class: 'row-btns' },
        h('button', { class: `btn${stale ? ' key' : ''}`, onclick: save }, t('backup.save')),
        h('button', { class: 'btn', onclick: () => input.click() }, t('backup.restore'))),
      h('p', { class: 'hint' }, t('backup.hint')), input);
  }

  await draw();
  return box;
}

/** @param {{code?: string, message?: string}} e */
function backupError(e) {
  if (e?.code === 'other_owner') return t('backup.err.other_owner', e.message);
  const key = `backup.err.${e?.code}`;
  const text = t(key);
  return text === key ? `${t('err.generic')}: ${e?.message ?? e}` : text;
}


/** Without the scheme, and with the host in lower case, as the format wants;
 * the path keeps its case. @param {string} s */
function normalLink(s) {
  const v = s.trim().replace(/^https?:\/\//i, '');
  const i = v.search(/[/?#]/);
  return i < 0 ? v.toLowerCase() : v.slice(0, i).toLowerCase() + v.slice(i);
}

const probe = {
  width: 1, height: 1, opaque: false, palette: { builtin: 0 }, frame: 0, frames: 1, delay_ms: 0,
  title: '', author: '', handle: null, link: '', pixels: new Uint8Array(1),
};

/** Development: own cards from the sample art, and received ones from
 * made-up artists. No reload afterwards: with storage unavailable the app
 * keeps data in memory, and a reload would throw it all away.
 * @returns {Promise<number>} cards added */
async function seed() {
  const { SAMPLES } = await import('../../dev/samples.js');
  const day = 86_400_000;
  const artists = ['dapple', 'mossfern', 'kitsu', 'vellum'].map((nick) => ({ nick, id: app.core.Identity.fromSeed(crypto.getRandomValues(new Uint8Array(32))) }));
  /** @param {any} s @param {{author: string, handle: any, link: string}} back @param {number} shade */
  const card = (s, back, shade) => ({
    width: 32, height: 32, opaque: s.opaque, palette: { builtin: 0 }, frame: shade, frames: 1, delay_ms: 0,
    title: s.title, ...back,
    pixels: Uint8Array.from(s.pixels, (/** @type {string} */ c) => parseInt(c, 16)),
  });
  // Own cards carry the person's real back, exactly as an import would;
  // made-up artists get made-up (valid) ones.
  const p = app.profile;
  const mine = { author: p.nick, handle: p.handle, link: p.link };
  const theirs = (/** @type {string} */ nick) => ({ author: nick, handle: { network: 'telegram', text: `@${nick}` }, link: `${nick}.art` });
  let n = 0;
  for (const s of SAMPLES.slice(0, 5)) {
    await addOwn(app.identity.signCard(card(s, mine, n % 6)));
    n++;
  }
  for (const [i, s] of SAMPLES.slice(5).entries()) {
    const a = artists[i % artists.length];
    const payload = a.id.signCard(card(s, theirs(a.nick), i % 8));
    const env = i % 4 === 1
      ? artists[(i + 1) % artists.length].id.sealAsSender(payload, artists[(i + 1) % artists.length].nick)
      : a.id.sealAsAuthor(payload, undefined);
    const r = await receive(app.core.openEnvelope(env));
    await backdate(r.digest, Date.now() - Math.floor(i * 6.3 + (i % 3)) * day);
    n++;
  }
  return n;
}
