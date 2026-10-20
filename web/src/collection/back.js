// @ts-check
// The back of a card in the collection: who drew it, how it came, whether
// it is genuine — and what is the person's own: tags and a note.
//
// Tags and the note change only after the pencil: a stray touch never
// opens a keyboard. Both have a fixed room, like a real card: two rows of
// at most five tags, and a ruled box that takes what fits and no more.

import { h, icon, fitBytes } from '../dom.js';
import { t, date } from '../i18n.js';
import { app, setNote, setTags, NOTE_BYTES } from '../state.js';
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
 * @param {{editing: boolean, onEdit: (on: boolean) => void, onTag: (tag: string) => void}} o
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

  // Tags: filters outside editing, removable inside it.
  const tags = h('div', { class: 'tags' });
  const fits = () => {
    const last = /** @type {HTMLElement|null} */ (tags.lastElementChild);
    return !last || last.offsetTop - tags.offsetTop + last.offsetHeight <= tags.clientHeight + 1;
  };
  const drawTags = () => {
    const list = rec.tags ?? [];
    tags.replaceChildren(...list.map((tag) => h('button', {
      type: 'button', class: 'tag', title: t(o.editing ? 'back.tag.remove' : 'back.tag.filter'),
      onclick: o.editing ? () => save(list.filter((x) => x !== tag)) : () => o.onTag(tag),
    }, `#${tag}`)));
    if (!o.editing && !list.length) tags.append(h('span', { class: 'hint' }, t('back.tags.none')));
    if (!o.editing || list.length >= MAX_TAGS) return;
    const add = h('button', { type: 'button', class: 'tag add', onclick: () => edit(add) }, t('back.tag.add'));
    tags.append(add);
    if (!fits()) add.remove(); // the two rows are full
  };
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
      save([...list, v]);
      // A tag that would need a third row does not go in.
      const chip = /** @type {HTMLElement} */ (tags.children[list.length]);
      if (chip.offsetTop - tags.offsetTop + chip.offsetHeight > tags.clientHeight + 1) {
        save(list);
        shake(tags);
      }
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

  const el = h('div', { class: `back-in${o.editing ? ' editing' : ''}` }, head, facts, tags, note);
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
