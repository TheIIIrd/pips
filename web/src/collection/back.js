// @ts-check
// The back of a card in the collection: who drew it, how it came, whether
// it is genuine — and what is the person's own: tags and a note.
//
// Tags and the note change only after the pencil: a stray touch never
// opens a keyboard. Both have a fixed room, like a real card: two rows of
// tags (the rest behind a "+N" chip), and a ruled box that
// takes what fits and no more.

import { h, icon, fitBytes } from '../dom.js';
import { t, date } from '../i18n.js';
import { app, setNote, setTags, daysLeft, NOTE_BYTES } from '../state.js';
import { toast, ask } from '../ui/shell.js';
import { normalTag, MAX_TAGS } from './query.js';

/** @param {Element} el */
const shake = (el) => {
  el.classList.remove('shake');
  void (/** @type {HTMLElement} */ (el)).offsetWidth;
  el.classList.add('shake');
  navigator.vibrate?.(8);
};

/**
 * @param {import('../state.js').Entry} entry
 * @param {{editing: boolean, onEdit: (on: boolean) => void, onTag: (tag: string) => void, onAuthor: (nick: string) => void}} o
 */
export function cardBack(entry, o) {
  const { card, rec, author } = entry;
  const by = card.author ? `@${card.author}` : '—';

  const pen = h('button', {
    type: 'button', class: 'pen', 'aria-label': t(o.editing ? 'back.done' : 'back.edit'), title: t(o.editing ? 'back.done' : 'back.edit'),
    onclick: () => o.onEdit(!o.editing),
  }, icon(o.editing ? 'check' : 'pen'));
  const head = h('header', {}, h('div', {}, h('h3', {}, card.title || '—'), h('div', { class: 'by' }, by)), pen);

  const facts = h('dl');
  const fact = (/** @type {string} */ k, /** @type {any} */ v, /** @type {string} */ cls = '') => facts.append(h('dt', {}, k), h('dd', { class: cls }, v));
  const chain = rec.mine
    ? t('back.yours')
    : [by, rec.from ? `@${rec.from.nick}` : null, t('back.to-you')].filter(Boolean).join(' → ');
  fact(t('back.k.path'), chain);
  fact(t('back.k.when'), date(rec.received, { day: 'numeric', month: 'long', year: 'numeric' }));
  const me = app.identity.publicKey().hex;
  if (rec.edition) {
    const ed = rec.edition;
    fact(t('back.k.copy'), `${t('back.edition', ed.edition, ed.of)} — ${ed.holder === me ? t('back.edition-yours') : t('back.edition-not-yours')}`);
  }
  fact(t('back.k.sign'), author ? `✓ ${t('back.sign.ok')} · ${author.short}` : t('back.sign.none'), author ? 'ok' : 'bad');
  for (const l of links(card)) fact(l.k, l.v);
  fact(t('back.k.pic'), t('back.tech', card.width, card.height, rec.payload.length));
  if (rec.state === 'trash') fact(t('back.k.bin'), t('back.bin-left', daysLeft(rec)), 'bad');
  // More by the same artist (by key, not by name): a tap shows them all.
  const others = author && !rec.mine ? [...app.cards.values()].filter((e) => e.author?.hex === author.hex && e.rec.digest !== rec.digest).length : 0;
  if (others && card.author) {
    facts.append(h('dt'), h('dd', {}, h('button', { type: 'button', class: 'mini-btn', onclick: () => o.onAuthor(card.author) }, t('back.more', others))));
  }

  // Tags: filters outside editing, removable inside it. Two rows show
  // what fits; the rest wait behind a "+N" chip that opens them all.
  const tags = h('div', { class: 'tags' });
  const all = h('div', { class: 'tags-all', hidden: true });
  /** @param {string} tag @param {string[]} list */
  const chip = (tag, list) => h('button', {
    type: 'button', class: 'tag', title: t(o.editing ? 'back.tag.remove' : 'back.tag.filter'),
    onclick: o.editing ? () => save(list.filter((x) => x !== tag)) : () => o.onTag(tag),
  }, `#${tag}`);
  const drawTags = () => {
    const list = rec.tags ?? [];
    const kids = [];
    // In editing, adding comes first, so it is never among the hidden ones.
    if (o.editing && list.length < MAX_TAGS) {
      const add = h('button', { type: 'button', class: 'tag add', onclick: () => edit(add) }, t('back.tag.add'));
      kids.push(add);
    }
    kids.push(...list.map((tag) => chip(tag, list)));
    if (!o.editing && !list.length) kids.push(h('span', { class: 'hint' }, t('back.tags.none')));
    tags.replaceChildren(...kids);
    fold();
    if (!all.hidden) openAll();
  };
  /** Hides the chips past the second row behind "+N". */
  const fold = () => {
    if (!tags.isConnected || !tags.clientHeight) return;
    tags.querySelector('.more')?.remove();
    const chips = /** @type {HTMLElement[]} */ ([...tags.children]);
    for (const c of chips) c.hidden = false;
    const below = (/** @type {HTMLElement} */ c) => c.offsetTop - tags.offsetTop + c.offsetHeight > tags.clientHeight + 1;
    if (!chips.some(below)) return;
    const more = h('button', { type: 'button', class: 'tag more', onclick: () => openAll() });
    tags.append(more);
    let hidden = 0;
    for (let k = chips.length - 1; k >= 0 && (below(more) || chips.slice(0, k + 1).some(below)); k--) {
      if (chips[k].classList.contains('add')) continue;
      chips[k].hidden = true;
      hidden++;
      more.textContent = `+${hidden}`;
    }
    more.setAttribute('aria-label', t('back.tags.all', (rec.tags ?? []).length));
  };
  /** Every tag at once, over the lower part of the back. */
  const openAll = () => {
    const list = rec.tags ?? [];
    all.replaceChildren(
      h('div', { class: 'tags-all-head' }, h('b', {}, t('back.tags.all', list.length)),
        h('button', { type: 'button', class: 'mini-btn', onclick: () => { all.hidden = true; } }, t('back.done'))),
      h('div', { class: 'tags-all-list' }, list.map((tag) => chip(tag, list))));
    all.hidden = false;
  };
  new ResizeObserver(() => fold()).observe(tags);
  /** @param {string[]} list */
  const save = (list) => {
    setTags(rec.digest, list);
    drawTags();
  };
  /** @param {HTMLElement} add */
  const edit = (add) => {
    // The field takes the button's place (a field inside a button would
    // press the button on every space).
    const input = /** @type {HTMLInputElement} */ (h('input', { class: 'tag add', placeholder: t('back.tag.ph'), enterkeyhint: 'done', autocapitalize: 'none' }));
    add.replaceWith(input);
    input.focus();
    // Enter, Escape and leaving the field all end it; only the first counts
    // (redrawing removes the field, which fires blur once more).
    let over = false;
    const finish = (/** @type {boolean} */ keep) => {
      if (over) return;
      over = true;
      const v = normalTag(input.value), list = rec.tags ?? [];
      if (!keep || !v || list.includes(v)) return drawTags();
      save([v, ...list]); // newest first: it stays in sight
    };
    input.addEventListener('keydown', (e) => { if (e.key === 'Enter') finish(true); if (e.key === 'Escape') finish(false); });
    input.addEventListener('blur', () => finish(true));
  };
  drawTags();

  // The note: a fixed, ruled box. What does not fit is not taken.
  const note = /** @type {HTMLTextAreaElement} */ (h('textarea', {
    'aria-label': t('back.note'), readOnly: !o.editing, tabIndex: o.editing ? 0 : -1,
    placeholder: t(o.editing ? 'back.note.ph' : 'back.note.none'),
  }));
  note.value = rec.note;
  let kept = rec.note;
  note.addEventListener('input', () => {
    const v = fitBytes(note.value, NOTE_BYTES);
    if (v !== note.value || note.scrollHeight > note.clientHeight + 1) {
      const at = Math.max(0, (note.selectionStart ?? 0) - (note.value.length - kept.length));
      note.value = kept;
      note.setSelectionRange(at, at);
      shake(note);
      return;
    }
    kept = v;
    setNote(rec.digest, v);
  });

  const el = h('div', { class: `back-in${o.editing ? ' editing' : ''}` }, head, facts, tags, note, all);
  return { el };
}

/** The artist's handle and link, each with a way to copy or open it. @param {any} card */
function links(card) {
  const copy = (/** @type {string} */ text) => h('button', {
    type: 'button', class: 'mini-btn',
    onclick: () => navigator.clipboard?.writeText(text).then(() => toast(t('toast.copied')), () => {}),
  }, t('back.copy'));
  const out = [];
  if (card.handle) out.push({ k: card.handle.network, v: [card.handle.text, ' ', copy(card.handle.text)] });
  if (card.link) {
    // Never a bare anchor: a link on a stranger's card opens only after the
    // person has seen exactly where it goes.
    const open = h('button', {
      type: 'button', class: 'mini-btn',
      onclick: () => {
        const url = `https://${card.link}`;
        ask(t('link.confirm', new URL(url).host), { yes: t('ask.open'), href: url });
      },
    }, t('back.open'));
    out.push({ k: 'web', v: [card.link, ' ', copy(card.link), ' ', open] });
  }
  return out;
}
