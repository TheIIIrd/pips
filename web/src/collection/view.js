// @ts-check
// The collection screen, put together from its parts:
//
//   stage.js     cards on the zoom axis, and the gestures that move them
//   turn.js      the card that turns over at the last level
//   back.js      what is on its back
//   share.js     what the share sheet shows
//   query.js     which cards, in what order
//   kit/*        bars, buttons, fields, sheets, the strip
//
// This file only decides what things mean: a pull up shares, a pull down
// sends the card to the archive (or back from it), and so on.

import { h } from '../dom.js';
import { t } from '../i18n.js';
import { app, subscribe, move, purge } from '../state.js';
import { getMeta, setMeta } from '../store.js';
import { frame, artCanvas, FRAME_SHADES } from '../ui/card.js';
import { toast, ask, go, hideTabs } from '../ui/shell.js';
import { showRecipientCode } from '../ui/edition.js';
import { loop, clamp, flight } from '../kit/motion.js';
import { iconButton } from '../kit/icon-button.js';
import { segmented } from '../kit/segmented.js';
import { chips } from '../kit/chips.js';
import { searchField } from '../kit/search-field.js';
import { topSheet } from '../kit/top-sheet.js';
import { strip as makeStrip } from '../kit/strip.js';
import { actionBar } from '../kit/action-bar.js';
import { identicon } from '../kit/identicon.js';
import { createStage } from './stage.js';
import { createTurn } from './turn.js';
import { cardBack } from './back.js';
import { fillShare } from './share.js';
import { select, tagCounts, toggleTag, words } from './query.js';
import { weekLabel, monthLabel } from './dates.js';

/** @typedef {import('./query.js').Place} Place */
/** @typedef {import('./stage.js').Item} Item */

const PLACE_TITLE = { collection: 'collection.title', archive: 'archive.title', trash: 'archive.trash' };
/** Where a card pulled down goes from each place. */
const DOWN = /** @type {const} */ ({ collection: 'archive', archive: 'collection', trash: 'collection' });

/** @param {{draw: () => void}} o what the first-run "draw" button does */
export function collectionView(o) {
  /** @type {import('./query.js').Filter} */
  const filter = { place: 'collection', owner: 'all', query: '', by: 'date' };
  let selecting = false, editing = false, lock = 0, pending = false;
  /** @type {Set<string>} */
  const selected = new Set();
  /** @type {Map<string, Item>} */
  const items = new Map();
  /** Latest arrivals of each place, newest first, for the strip. */
  const recents = /** @type {Record<Place, string[]>} */ ({ collection: [], archive: [], trash: [] });

  // ------------------------------------------------------------- the parts --
  const backBtn = iconButton({ icon: 'back', label: t('collection.back'), plain: true, onClick: () => back() });
  const archBtn = iconButton({ icon: 'archive', label: t('archive.title'), onClick: () => setPlace('archive') });
  const binBtn = iconButton({ icon: 'trash', label: t('archive.trash'), onClick: () => setPlace('trash') });
  const selBtn = h('button', { type: 'button', class: 'pill', onclick: () => (selecting ? stopSelect() : startSelect()) });
  const title = h('h1');
  const sub = h('span');
  const chip = h('button', { type: 'button', class: 'filter-chip', onclick: () => clearFilter() });
  const bar = h('header', { class: 'bar collection-bar' }, backBtn.el,
    h('div', { class: 'title' }, title, h('div', { class: 'sub' }, sub, chip)), archBtn.el, binBtn.el, selBtn);

  const search = searchField({ placeholder: t('search.ph'), clearLabel: t('search.clear'), onInput: (v) => refilter(() => { filter.query = v; }) });
  const owner = segmented({
    label: t('owner.label'), value: filter.owner,
    options: [{ value: 'all', label: t('owner.all') }, { value: 'mine', label: t('owner.mine') }, { value: 'got', label: t('owner.got') }],
    onChange: (v) => refilter(() => { filter.owner = v; }),
  });
  const groupBtn = h('button', { type: 'button', class: 'group-btn', onclick: () => regroup() });
  const tagChips = chips({ label: t('tags.label'), onToggle: (tag) => setQuery(toggleTag(filter.query, tag)) });
  const none = h('p', { class: 'search-none' }, t('search.none'));
  const searchPanel = h('div', { class: 'search-panel' }, search.el, h('div', { class: 'search-opts' }, owner.el, groupBtn), tagChips.el, none);

  const host = h('div', { class: 'stage-host' });
  const fly = h('div', { class: 'fly-layer' });
  const placeEmpty = h('p', { class: 'place-empty' });
  const first = firstRun();
  const coarse = typeof matchMedia === 'function' && matchMedia('(pointer: coarse)').matches;
  const coach = h('div', { class: 'coach', hidden: true }, t(coarse ? 'collection.coach' : 'collection.coach.desk'));
  const strip = makeStrip();
  /** @type {null | (() => void)} */
  let stopShare = null;
  /** @type {string|null} */
  let sharing = null;
  const sheet = topSheet({ label: t('share.title'), onClose: () => { stopShare?.(); stopShare = null; sharing = null; lock = Math.max(0, lock - 1); sync(); } });
  const feedBar = actionBar({ label: t('collection.card'), compact: true });
  const selectBar = actionBar({ label: t('collection.select-title') });

  const el = h('section', { class: 'view collection-view' },
    host, placeEmpty, first.el, searchPanel, bar, fly, strip.el, feedBar.el, selectBar.el, coach, ...sheet.els);

  // The safe-area insets in pixels, measured once per size.
  const probe = h('div', { class: 'safe-probe', 'aria-hidden': 'true' });
  let safe = { t: 0, b: 0 };
  const measure = () => {
    const cs = getComputedStyle(probe);
    safe = { t: parseFloat(cs.paddingTop) || 0, b: parseFloat(cs.paddingBottom) || 0 };
  };
  /** @returns {import('./layout.js').Frame} */
  const frameOf = () => ({
    W: el.clientWidth || innerWidth, H: el.clientHeight || innerHeight,
    top: 76 + safe.t, bottom: 84 + safe.b, lead: searchPanel.offsetHeight + 8,
  });
  el.append(probe);

  const stage = createStage({
    host, surface: el, frame: frameOf,
    by: () => filter.by,
    label: (level, g) => {
      const c = g.cards[0], n = g.cards.length;
      if (filter.by === 'author') return { title: c.author ? `@${c.author}` : '—', sub: level === 0 ? String(n) : t('collection.count', n) };
      return level === 0 ? { title: monthLabel(c.received), sub: String(n) } : { title: weekLabel(c.received), sub: t('collection.count', n) };
    },
    skip: '.bar, .search-panel, .action-bar, .top-sheet, .scrim, .strip, .first-run, .coach',
    control: '.turn button, .turn textarea, .turn input',
    blocked: () => lock > 0,
    tap, hold, pull, turn: () => (stage.level === 3 ? turn.drag() : null),
    drawn, levelChanged,
  });
  const turn = createTurn({ host });

  // The strip is up while a card is pulled down, while cards fly to it and
  // a moment after; every frame checks that, so it can never stay behind.
  let stripUp = false, stripHold = 0, pulling = false;
  /** @param {number} f @param {boolean} [anim] */
  function showStrip(f, anim = false) {
    stripUp = f > 0;
    strip.reveal(f, anim);
  }

  // ------------------------------------------------------------- the cards --
  /** What the query functions need of an entry. @param {import('../state.js').Entry} e */
  const asItem = (e) => ({ e, state: e.rec.state, mine: e.rec.mine, received: e.rec.received, title: e.card.title ?? '', author: e.card.author ?? '', tags: e.rec.tags ?? [] });
  /** @param {import('../state.js').Entry} e */
  function itemFor(e) {
    let it = items.get(e.rec.digest);
    if (!it) {
      it = stage.item(e.rec.digest, frame(e), { received: e.rec.received, author: e.card.author ?? '' });
      items.set(e.rec.digest, it);
    }
    return it;
  }
  const entries = () => [...app.cards.values()];
  const inPlace = (/** @type {Place} */ p) => entries().filter((e) => e.rec.state === p);

  /** Brings the stage in line with the state. Waits while cards are flying. */
  function sync() {
    if (lock) { pending = true; return; }
    pending = false;
    for (const id of items.keys()) if (!app.cards.has(id)) items.delete(id);
    paintChrome(); // first: the search panel's height places the months
    stage.reflow(select(entries().map(asItem), filter).map((x) => itemFor(x.e)));
    turn.invalidate();
    paintChrome();
  }

  /** Changes the filter; the cards that stay glide, the others fade. @param {() => void} change */
  function refilter(change) {
    change();
    sync();
  }
  /** @param {string} q */
  function setQuery(q) {
    search.set(q);
    refilter(() => { filter.query = q; });
  }
  function clearFilter() {
    search.set('');
    owner.set('all');
    refilter(() => { filter.query = ''; filter.owner = 'all'; });
  }
  function regroup() {
    refilter(() => { filter.by = filter.by === 'date' ? 'author' : 'date'; });
    for (const s of stage.scroll) s.to = 0;
    stage.kick();
  }
  /** Opens the archive or the bin as a place of its own, or goes home. @param {Place} p */
  function setPlace(p) {
    if (lock || stage.busy || p === filter.place) return;
    stopSelect();
    if (stage.Z.to > 1) stage.setLevel(1);
    for (const s of stage.scroll) s.set(0);
    showStrip(0);
    refilter(() => { filter.place = p; });
  }
  function back() {
    if (lock || stage.busy) return;
    if (stage.level >= 2) stage.setLevel(stage.level - 1);
    else setPlace('collection');
  }

  // --------------------------------------------------------------- gestures --
  /** @param {Item|null} c @param {number} level */
  function tap(c, level) {
    if (level <= 1) {
      if (!c) return;
      if (selecting) { toggle(c.id); return; }
      stage.align(c);
      stage.setLevel(2);
    } else if (level === 2) {
      if (!c) return;
      const i = stage.list.indexOf(c);
      if (i === Math.round(stage.feed.v)) stage.setLevel(3);
      else { stage.feed.to = i; stage.kick(); }
    } else turn.flipOver();
  }
  /** @param {Item} c */
  function hold(c) {
    startSelect();
    toggle(c.id);
    navigator.vibrate?.(10);
    return true;
  }
  /** The feed card pulled up or down. @param {Item} c @returns {import('../kit/gestures.js').DragHandler} */
  function pull(c) {
    const H = el.clientHeight, canShare = filter.place !== 'trash';
    return {
      move(p) {
        pulling = true;
        const dy = p.dy;
        stage.dragY.set(dy < 0 ? (canShare ? dy * 0.6 : dy * 0.25) : dy);
        if (dy < 0 && canShare) {
          prepareShare(c);
          // The code waits until the card is lifted well up, then follows the finger.
          sheet.follow(clamp((-dy - 90) / (H * 0.2), 0, 1));
          showStrip(0);
        } else {
          drawStrip();
          showStrip(clamp(dy / (H * 0.26), 0, 1));
          sheet.follow(0);
        }
        stage.kick();
      },
      end(_p, v) {
        pulling = false;
        const d = stage.dragY.v;
        if (canShare && (d < -150 || (v.y < -900 && d < -70))) { stage.dragY.to = 0; openShare(c); }
        else if (d > H * 0.2 || v.y > 900) send([c], true);
        else {
          stage.dragY.to = 0;
          if (!sheet.open) { sheet.follow(0, true); dropShare(); }
          showStrip(0, true);
        }
        stage.kick();
      },
    };
  }

  // ---------------------------------------------------------------- sharing --
  /** @param {Item} c */
  function prepareShare(c) {
    if (sharing === c.id) return;
    const e = app.cards.get(c.id);
    if (!e) return;
    dropShare();
    sharing = c.id;
    sheet.paint(FRAME_SHADES[e.card.frame] ?? FRAME_SHADES[0]);
    stopShare = fillShare(sheet.body, e);
  }
  function dropShare() {
    if (sheet.open) return;
    stopShare?.();
    stopShare = null;
    sharing = null;
  }
  /** @param {Item} c */
  function openShare(c) {
    if (sheet.open) return;
    prepareShare(c);
    lock++;
    sheet.show();
    paintChrome();
  }

  // ------------------------------------------------------- moving cards --
  /**
   * Cards rise and fly into the strip one after another, each landing on
   * its first place while the others move along; the rest close up.
   * @param {Item[]} list @param {boolean} pulled
   */
  function send(list, pulled) {
    if (!list.length) return;
    lock++;
    const from = filter.place, to = DOWN[from], inFeed = stage.Z.v > 1.5;
    sheet.follow(0, true);
    dropShare();
    drawStrip();
    showStrip(1, true);
    stripHold = Infinity;
    const now = performance.now(), start = now + (pulled ? 0 : 160);
    const dur = 620, gap = list.length <= 6 ? 190 : Math.max(60, 1100 / list.length);
    list.forEach((c, k) => {
      const r0 = c.el.getBoundingClientRect(), t0 = start + k * gap;
      const path = flight({ x: r0.left, y: r0.top, w: r0.width }, () => strip.slot(0), { start: t0, dur });
      fly.append(c.el); // above the strip: the card lands on it, not under it
      if (inFeed) {
        // The neighbours close the gap while it flies, so nothing jumps at the end.
        c.collapse = { t0: t0 + 90, dur: 470 };
        const i = stage.list.indexOf(c);
        if (i === stage.list.length - 1 && i > 0) setTimeout(() => { stage.feed.to = i - 1; stage.kick(); }, 90);
      }
      c.flying = {
        at: (tt) => {
          const p = path.at(tt);
          // Picked from the grid, a card first lifts a little.
          const lift = pulled ? 0 : Math.sin(clamp((tt - now) / 160, 0, 1) * Math.PI / 2) * (1 - p.t);
          return { x: p.x, y: p.y - lift * 10, w: p.w * (1 + lift * 0.06), cap: inFeed ? Math.max(0, 1 - p.t * 2) : 0 };
        },
      };
      // Near the end the strip moves along to make room…
      setTimeout(() => drawStrip(1), t0 + dur * 0.7 - now);
      // …and on touching down the card becomes the first of the strip.
      setTimeout(() => {
        c.flying = null;
        c.collapse = null;
        c.fade.set(0);
        host.append(c.el);
        recents[to] = [c.id, ...recents[to].filter((x) => x !== c.id)].slice(0, 8);
        selected.delete(c.id);
        move(c.id, to);
        stage.reflow(stage.list.filter((x) => x !== c));
        drawStrip(0, c.id, to);
      }, t0 + dur - now);
    });
    stage.kick();
    setTimeout(() => {
      stage.dragY.set(0);
      lock--;
      stopSelect();
      sync();
      const ids = list.map((c) => c.id);
      toast(t(`moved.${to}`, ids.length), { label: t('toast.undo'), run: () => relocate(ids, from) });
      stripHold = performance.now() + 1300;
      setTimeout(() => stage.kick(), 1350); // the frame loop sleeps when nothing moves
    }, start - now + (list.length - 1) * gap + dur + 40);
  }

  /** Moves cards without a flight: they fade out here, or in. @param {string[]} ids @param {Place} to */
  async function relocate(ids, to) {
    lock++;
    for (const id of ids) await move(id, to);
    lock--;
    sync();
  }
  /** @param {string[]} ids */
  async function toTrash(ids) {
    const from = filter.place;
    await relocate(ids, 'trash');
    stopSelect();
    toast(t('moved.trash', ids.length), { label: t('toast.undo'), run: () => relocate(ids, from) });
  }
  /** @param {string[]} ids */
  async function restore(ids) {
    await relocate(ids, 'collection');
    stopSelect();
    toast(t('moved.collection', ids.length), { label: t('toast.undo'), run: () => relocate(ids, 'trash') });
  }
  /** @param {string[]} ids */
  async function purgeAll(ids) {
    if (!(await ask(t('confirm.purge-n', ids.length), { yes: t('ask.delete'), danger: true }))) return;
    lock++;
    for (const id of ids) await purge(id);
    lock--;
    stopSelect();
    sync();
    toast(t('moved.purged', ids.length));
  }

  /** @param {number} [room] @param {string} [landed] @param {Place} [target] */
  function drawStrip(room = 0, landed, target = DOWN[filter.place]) {
    const there = inPlace(target);
    if (!recents[target].length) recents[target] = there.sort((a, b) => b.rec.received - a.rec.received).slice(0, 8).map((e) => e.rec.digest);
    const ids = recents[target].filter((id) => app.cards.get(id)?.rec.state === target || id === landed);
    strip.draw(ids.map((id) => ({
      id,
      make: () => {
        const e = /** @type {import('../state.js').Entry} */ (app.cards.get(id));
        const m = h('div', { class: 'mini-paper' }, artCanvas(e.card, id));
        m.style.setProperty('--shade', (FRAME_SHADES[e.card.frame] ?? FRAME_SHADES[0]).paper);
        return m;
      },
    })), {
      room, landed, title: t(PLACE_TITLE[target]), count: there.length ? t('collection.count', there.length) : '',
      empty: t(target === 'archive' ? 'strip.archive.empty' : 'strip.collection.empty'),
    });
  }

  // -------------------------------------------------------------- selecting --
  function startSelect() {
    if (selecting) return;
    selecting = true;
    el.classList.add('selecting');
    paintChrome();
  }
  function stopSelect() {
    if (!selecting) return;
    selecting = false;
    selected.clear();
    el.classList.remove('selecting');
    for (const it of items.values()) it.el.classList.remove('chosen');
    paintChrome();
  }
  /** @param {string} id */
  function toggle(id) {
    if (selected.has(id)) selected.delete(id); else selected.add(id);
    items.get(id)?.el.classList.toggle('chosen', selected.has(id));
    if (!selected.size) stopSelect();
    else paintChrome();
  }
  const chosen = () => stage.list.filter((c) => selected.has(c.id));

  // ----------------------------------------------------------------- chrome --
  /** Things that follow the zoom every frame: cheap writes only. @param {number} z */
  function drawn(z) {
    const c = stage.current();
    // A finger on the back's pencil is a touch too: only a pinch or a
    // flight makes the back stop taking presses.
    turn.draw(z, c, build, !stage.pinching && !lock);
    if (stripUp && !pulling && !lock && performance.now() > stripHold) showStrip(0, true);
    const sv = Math.max(0, 1 - z * 2.2);
    searchPanel.style.opacity = String(sv);
    searchPanel.style.visibility = sv < 0.02 ? 'hidden' : '';
    searchPanel.style.pointerEvents = sv > 0.6 ? '' : 'none';
    searchPanel.style.transform = `translate3d(0,${frameOf().top - stage.scroll[0].v}px,0)`;
    feedBar.show(z > 1.6 && !lock && !stage.dragY.v && !!c);
    const away = z > 1.4 || selecting || filter.place !== 'collection';
    if (away !== tabsAway) { tabsAway = away; if (!el.hidden) hideTabs(away); }
    const lv = Math.round(z);
    if (lv !== drawnLevel) { drawnLevel = lv; paintChrome(); }
  }
  let tabsAway = false, drawnLevel = -1;

  /** @param {number} level */
  function levelChanged(level) {
    if (level >= 2) stopSelect();
    if (level < 3 && editing) setEditing(false);
    if (coachShown) { coachShown = false; coach.hidden = true; setMeta('coached', true); }
    paintChrome();
  }

  /** The bar, the panels and the actions, for the current state. */
  function paintChrome() {
    const lv = Math.round(stage.Z.to), filtered = !!(filter.query || filter.owner !== 'all');
    const firstRun = filter.place === 'collection' && !inPlace('collection').length;
    title.textContent = selecting ? t('collection.select-title') : t(PLACE_TITLE[filter.place]);
    const levelName = lv >= 3 ? 'collection.card' : lv === 2 ? 'collection.feed' : filter.by === 'author' ? 'collection.by-authors' : lv === 0 ? 'collection.by-months' : 'collection.by-weeks';
    sub.textContent = selecting ? t('collection.selected', selected.size) : `${t(levelName)}${filter.place === 'trash' && lv <= 1 ? ` · ${t('collection.bin-note')}` : ''}`;
    const showChip = filtered && lv >= 1 && !selecting && !firstRun;
    chip.hidden = !showChip;
    sub.hidden = showChip;
    chip.textContent = `${[filter.owner === 'all' ? '' : t(`owner.chip.${filter.owner}`), filter.query].filter(Boolean).join(' · ')}  ×`;
    backBtn.el.hidden = (lv < 2 && filter.place === 'collection') || selecting;
    archBtn.el.hidden = binBtn.el.hidden = selecting || filter.place !== 'collection';
    archBtn.count(inPlace('archive').length);
    binBtn.count(inPlace('trash').length);
    selBtn.hidden = (lv >= 2 && !selecting) || !stage.list.length;
    selBtn.textContent = t(selecting ? 'collection.done' : 'collection.select');
    groupBtn.textContent = t(filter.by === 'author' ? 'group.author' : 'group.date');
    tagChips.update(tagCounts(inPlace(filter.place).map(asItem)).map(([tag, n]) => ({ value: tag, text: `#${tag}`, count: n, on: words(filter.query).includes(`#${tag}`) })));
    none.hidden = !filtered || stage.list.length > 0;
    first.el.hidden = !firstRun;
    if (firstRun) coach.hidden = true; else if (coachShown) coach.hidden = false;
    searchPanel.hidden = firstRun;
    placeEmpty.hidden = filter.place === 'collection' || inPlace(filter.place).length > 0;
    placeEmpty.textContent = t(filter.place === 'archive' ? 'place.archive.empty' : 'place.trash.empty');
    if (firstRun) first.paint();

    const ids = () => chosen().map((c) => c.id);
    selectBar.set(filter.place === 'collection'
      ? [{ id: 'trash', label: t('act.to-trash'), run: () => toTrash(ids()) }, { id: 'archive', label: t('act.archive'), kind: 'key', run: () => send(chosen(), false) }]
      : filter.place === 'archive'
        ? [{ id: 'trash', label: t('act.to-trash'), run: () => toTrash(ids()) }, { id: 'collection', label: t('act.to-collection'), kind: 'key', run: () => send(chosen(), false) }]
        : [{ id: 'purge', label: t('act.purge'), kind: 'warn', run: () => purgeAll(ids()) }, { id: 'restore', label: t('act.restore'), kind: 'key', run: () => restore(ids()) }]);
    selectBar.show(selecting);

    const cur = () => stage.current();
    const one = (/** @type {(c: Item) => void} */ f) => () => { const c = cur(); if (c && !lock) f(c); };
    feedBar.set(filter.place === 'trash'
      ? [{ id: 'turn', label: t('act.turn'), icon: 'turn', run: one(() => { stage.setLevel(3); turn.flipOver(); }) },
        { id: 'restore', label: t('act.restore'), icon: 'restore', run: one((c) => send([c], true)) },
        { id: 'purge', label: t('act.purge'), icon: 'trash', run: one((c) => purgeAll([c.id])) }]
      : [{ id: 'share', label: t('act.share'), icon: 'share', run: one(openShare) },
        { id: 'turn', label: t('act.turn'), icon: 'turn', run: one(() => { stage.setLevel(3); turn.flipOver(); }) },
        ...(filter.place === 'archive' ? [{ id: 'trash', label: t('act.to-trash'), icon: /** @type {const} */ ('trash'), run: one((c) => toTrash([c.id])) }] : []),
        { id: 'down', label: t(filter.place === 'archive' ? 'act.to-collection' : 'act.archive'), icon: filter.place === 'archive' ? 'restore' : 'archive', run: one((c) => send([c], true)) }]);
  }

  // -------------------------------------------------------------- the twin --
  /** @param {string} id */
  function build(id) {
    const e = /** @type {import('../state.js').Entry} */ (app.cards.get(id));
    const back = cardBack(e, { editing, onEdit: setEditing, onTag: (tag) => { setQuery(`#${tag}`); stage.setLevel(0); } }).el;
    const s = FRAME_SHADES[e.card.frame] ?? FRAME_SHADES[0];
    back.style.setProperty('--shade', s.paper);
    back.style.setProperty('--shade-ink', s.ink);
    back.style.setProperty('--shade-dim', s.dim);
    return { front: frame(e), back };
  }
  /** @param {boolean} on */
  function setEditing(on) {
    editing = on;
    turn.invalidate();
    loop.kick();
  }

  // ----------------------------------------------------------- first run --
  /** The empty collection starts with the person's own calling card. */
  function firstRun() {
    const mark = h('div', { class: 'me-art' });
    const name = h('b');
    const info = h('small');
    const card = h('button', { type: 'button', class: 'me-card', onclick: () => showRecipientCode() }, mark, name, info);
    const el = h('div', { class: 'first-run', hidden: true }, card,
      h('h2', {}, t('first.title')), h('p', {}, t('first.text')),
      h('div', { class: 'row-btns' },
        h('button', { type: 'button', class: 'btn', onclick: () => o.draw() }, t('first.draw')),
        h('button', { type: 'button', class: 'btn key', onclick: () => go('receive') }, t('first.scan'))));
    let painted = '';
    return {
      el,
      paint() {
        const key = app.identity.publicKey();
        if (painted === key.hex + app.profile.nick) return;
        painted = key.hex + app.profile.nick;
        mark.replaceChildren(identicon(key.hex));
        name.textContent = app.profile.nick || '—';
        info.textContent = `${t('first.card')} · ${key.short}`;
      },
    };
  }

  // ------------------------------------------------------------- keyboard --
  document.addEventListener('keydown', (e) => {
    if (el.hidden || e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
    if (document.querySelector('.overlay, .dialog')) return;
    const lv = stage.level;
    if (e.key === 'Escape') {
      if (sheet.open) return;
      if (editing) setEditing(false);
      else if (selecting) stopSelect();
      else back();
    } else if (lv >= 2 && (e.key === 'ArrowLeft' || e.key === 'ArrowRight')) {
      stage.feed.to = clamp(Math.round(stage.feed.to) + (e.key === 'ArrowLeft' ? -1 : 1), 0, stage.list.length - 1);
      stage.kick();
    } else if (lv === 3 && e.key === ' ') {
      e.preventDefault();
      turn.flipOver();
    }
  });
  addEventListener('resize', () => { measure(); stage.relayout(); });

  let coachShown = false;
  getMeta('coached').then((done) => { if (!done) { coachShown = true; coach.hidden = false; } });

  subscribe(() => { if (!el.hidden) sync(); });
  return {
    el,
    /** @param {{open?: string}} [arg] opens one card large: a card just received or made */
    show(arg) {
      measure();
      hideTabs(tabsAway);
      if (arg?.open) {
        const e = app.cards.get(arg.open);
        if (e) {
          filter.place = e.rec.state;
          if (!select([asItem(e)], filter).length) { filter.query = ''; filter.owner = 'all'; search.set(''); owner.set('all'); }
        }
      }
      sync();
      stage.relayout();
      const it = arg?.open ? items.get(arg.open) : null;
      if (it) {
        stage.feed.set(Math.max(0, stage.list.indexOf(it)));
        turn.reset();
        stage.Z.set(3);
        stage.kick();
      }
    },
    hide() { hideTabs(false); sheet.close(); },
  };
}
