// @ts-check
// The stage: cards on one continuous zoom axis — months · weeks · feed ·
// card — driven by the fingers.
//
// One number, `Z`, says where along the axis the screen is (0 months … 3
// card); every frame each card is drawn between its rectangle at the level
// below Z and the one above, so a pinch moves through the levels as one
// motion rather than switching screens. Scrolling, the feed's position and
// every card's small offsets are springs on the shared frame loop.
//
// The stage knows how cards move, not what they mean: taps, holds and pulls
// are reported to its owner (the collection view), which decides.

import { h } from '../dom.js';
import { Spring, SPRINGS, loop, mix, clamp, ease, rubber } from '../kit/motion.js';
import { gestures } from '../kit/gestures.js';
import { grid, feedRect, feedWidth, runs, groupKey } from './layout.js';

/**
 * One card on the stage.
 * @typedef {{
 *   id: string, el: HTMLElement, received: number, author: string,
 *   off: {x: Spring, y: Spring}, fade: Spring,
 *   ghost: null | {x: number, y: number, w: number, cap: number},
 *   collapse: null | {t0: number, dur: number},
 *   flying: null | {at: (now: number) => {x: number, y: number, w: number, cap: number}},
 *   rr: null | {x: number, y: number, w: number, h: number, cap: number},
 *   dragged?: boolean,
 * }} Item
 */

/**
 * @param {{
 *   host: HTMLElement, surface: HTMLElement,
 *   frame: () => import('./layout.js').Frame,
 *   by: () => 'date'|'author',
 *   label: (level: 0|1, group: {key: string, cards: Item[]}) => {title: string, sub: string},
 *   skip: string, control: string, blocked: () => boolean,
 *   tap: (item: Item|null, level: number) => void,
 *   hold: (item: Item) => boolean,
 *   pull: (item: Item) => import('../kit/gestures.js').DragHandler | null,
 *   turn: () => import('../kit/gestures.js').DragHandler | null,
 *   drawn: (z: number) => void,
 *   levelChanged: (level: number) => void,
 * }} o
 */
export function createStage(o) {
  const Z = new Spring(1, SPRINGS.zoom);
  const scroll = [new Spring(0, SPRINGS.scroll), new Spring(0, SPRINGS.scroll)];
  const fling = [0, 0];
  const feed = new Spring(0, SPRINGS.feed);
  const dragY = new Spring(0, SPRINGS.drag);
  /** @type {Item[]} */
  let list = [];
  /** @type {Map<string, Item>} */
  const all = new Map(); // shown, leaving or flying
  /** @type {ReturnType<typeof layouts>} */
  let cache = layouts();
  /** @type {Map<Item, number>} */
  let fidx = new Map();
  /** @type {HTMLElement[][]} */
  const labelEls = [[], []];
  let lastLevel = 1;

  /** @param {string} id @param {HTMLElement} el @param {{received: number, author: string}} data @returns {Item} */
  function item(id, el, data) {
    el.classList.add('on-stage');
    return {
      id, el, ...data, off: { x: new Spring(0), y: new Spring(0) }, fade: new Spring(1, SPRINGS.fade),
      ghost: null, collapse: null, flying: null, rr: null,
    };
  }

  function layouts() {
    const f = o.frame();
    return /** @type {const} */ ([0, 1]).map((l) => grid(l, runs(list, (c) => groupKey(l, o.by(), c)), f));
  }

  /** The rectangle of `c` at a level, on screen. @param {number} level @param {Item} c */
  function rectAt(level, c) {
    if (level <= 1) {
      const r = cache[level].rects.get(c);
      return r ? { ...r, y: r.y - scroll[level].v, o: 1, z: 1 } : { x: 0, y: 0, w: 0, cap: 0, o: 0, z: 1 };
    }
    return feedRect(fidx.get(c) ?? list.indexOf(c), feed.v, level === 3, o.frame());
  }

  function render() {
    const z = Z.v, a = clamp(Math.floor(z), 0, 2), t = ease(clamp(z - a, 0, 1)), now = performance.now();
    // Feed positions: a card leaving the feed gives up its place gradually.
    fidx = new Map();
    let acc = 0;
    for (const c of list) {
      fidx.set(c, acc);
      acc += 1 - ease(c.collapse ? clamp((now - c.collapse.t0) / c.collapse.dur, 0, 1) : 0);
    }
    const centre = list[Math.round(feed.v)];
    for (const c of all.values()) {
      const shown = list.includes(c);
      const ghost = !shown && !c.flying && c.ghost && c.fade.v > 0.01;
      if (!shown && !c.flying && !ghost) {
        c.el.remove();
        all.delete(c.id);
        continue;
      }
      /** @type {{x: number, y: number, w: number, cap: number, o: number, z: number}} */
      let r;
      if (c.flying) r = { ...c.flying.at(now), o: 1, z: 300 };
      else if (ghost && c.ghost) {
        // Leaving the set: it stays where it was and fades, slightly shrinking.
        const g = c.ghost, w = g.w * (1 - (1 - c.fade.v) * 0.08);
        r = { x: g.x + (g.w - w) / 2, y: g.y + (g.w - w) / 2, w, cap: g.cap, o: c.fade.v, z: 0 };
      } else {
        const ra = rectAt(a, c), rb = rectAt(a + 1, c);
        r = { x: mix(ra.x, rb.x, t), y: mix(ra.y, rb.y, t), w: mix(ra.w, rb.w, t), cap: mix(ra.cap, rb.cap, t), o: mix(ra.o, rb.o, t), z: t < 0.5 ? ra.z : rb.z };
        r.x += c.off.x.v;
        r.y += c.off.y.v;
        // Arriving: rises out of the stage, a beat after the others leave.
        const f = clamp(c.fade.v, 0, 1);
        if (f < 1) {
          const w = r.w * (0.92 + 0.08 * f);
          r.x += (r.w - w) / 2;
          r.y += (r.w - w) / 2 + (1 - f) * 10;
          r.w = w;
          r.o *= f;
        }
        // The feed card being pulled follows the finger, shrinking a little going down.
        if (z > 1.5 && c === centre && dragY.v) {
          const p = clamp(dragY.v / (o.frame().H * 0.45), 0, 1), cx = r.x + r.w / 2;
          r.w *= 1 - p * 0.45;
          r.x = cx - r.w / 2;
          r.y += dragY.v;
        }
      }
      const hgt = r.w + (r.w * 0.17 + 8) * r.cap;
      c.rr = { x: r.x, y: r.y, w: r.w, h: hgt, cap: r.cap };
      const s = c.el.style;
      s.width = `${r.w}px`;
      s.transform = `translate3d(${r.x}px,${r.y}px,0)`;
      s.opacity = String(r.o);
      s.zIndex = String(r.z);
      s.setProperty('--cap', String(r.cap));
      s.visibility = r.w < 1 ? 'hidden' : '';
    }
    drawLabels(z);
    o.drawn(z);
    const lv = Math.round(Z.to);
    if (lv !== lastLevel) { lastLevel = lv; o.levelChanged(lv); }
  }

  /** Group labels belong to their level and fade with distance from it. @param {number} z */
  function drawLabels(z) {
    for (const level of /** @type {const} */ ([0, 1])) {
      const vis = Math.max(0, 1 - Math.abs(z - level) * 2.2), labs = cache[level].labels, els = labelEls[level];
      while (els.length < labs.length) {
        const l = h('div', { class: 'stage-label', 'aria-hidden': 'true' }, h('b'), h('span'));
        o.host.append(l);
        els.push(l);
      }
      els.forEach((l, k) => {
        const lab = labs[k];
        l.hidden = !lab || vis === 0;
        if (l.hidden) return;
        const { title, sub } = o.label(level, lab);
        /** @type {HTMLElement} */ (l.firstChild).textContent = title;
        /** @type {HTMLElement} */ (l.lastChild).textContent = sub;
        l.classList.toggle('by-author', o.by() === 'author');
        // Type follows the cell: the label is part of the grid, not above it.
        const fs = lab.w * (o.by() === 'author' ? (level === 0 ? 0.15 : 0.13) : level === 0 ? 0.19 : 0.15);
        Object.assign(l.style, {
          width: `${lab.w}px`, height: `${lab.w}px`, fontSize: `${fs}px`, opacity: String(vis),
          transform: `translate3d(${lab.x}px,${lab.y - scroll[level].v}px,0)`,
        });
      });
    }
  }

  // --------------------------------------------------------------- motion --
  function momentum(/** @type {number} */ dt) {
    let busy = false;
    for (const l of [0, 1]) {
      if (scrolling === l) continue;
      const s = scroll[l], max = cache[l].max;
      if (Math.abs(fling[l]) > 5) { s.set(s.v + fling[l] * dt); fling[l] *= Math.pow(0.03, dt); busy = true; } else fling[l] = 0;
      if (s.v < 0 || s.v > max) { s.to = clamp(s.v, 0, max); if (Math.abs(fling[l]) > 5) fling[l] *= 0.6; busy = true; }
    }
    return busy;
  }
  loop.add((dt) => {
    let busy = false;
    for (const s of [Z, ...scroll, feed, dragY]) busy = s.step(dt) || busy;
    for (const c of all.values()) for (const s of [c.off.x, c.off.y, c.fade]) busy = s.step(dt) || busy;
    busy = momentum(dt) || busy;
    for (const c of all.values()) if (c.flying || c.collapse) busy = true;
    render();
    return busy || g.busy;
  });
  const kick = () => loop.kick();

  // ------------------------------------------------------------- gestures --
  /** @type {number|null} */
  let scrolling = null;
  const level = () => Math.round(Z.to);
  /** The card under a point; where cards overlap (the feed), the topmost. @param {number} x @param {number} y */
  function cardAt(x, y) {
    let best = null, bz = -Infinity;
    for (const c of list) {
      const r = c.el.getBoundingClientRect(), zi = Number(c.el.style.zIndex) || 0;
      if (Number(c.el.style.opacity) < 0.05) continue;
      if (x >= r.left && x <= r.right && y >= r.top && y <= r.bottom && zi > bz) { best = c; bz = zi; }
    }
    return best;
  }
  /** @param {{x: number, y: number}} p */
  function nearest(p) {
    let best = null, bd = Infinity;
    for (const c of list) {
      const r = c.el.getBoundingClientRect(), d = Math.hypot(r.left + r.width / 2 - p.x, r.top + r.height / 2 - p.y);
      if (d < bd) { bd = d; best = c; }
    }
    return best;
  }
  /** Lines the levels up around `c`: the card under the fingers keeps its
   * place on screen as the layout changes around it. @param {Item|null} c */
  function align(c) {
    if (!c) return;
    const r0 = cache[0].rects.get(c), r1 = cache[1].rects.get(c), lv = Math.floor(Z.v);
    if (lv === 0 && r0 && r1) scroll[1].set(clamp(r1.y - (r0.y - scroll[0].v) - (r1.w - r0.w) / 2, 0, cache[1].max));
    else if (lv >= 1 && r0 && r1) scroll[0].set(clamp(r0.y - (r1.y - scroll[1].v) + (r1.w - r0.w) / 2, 0, cache[0].max));
    feed.set(Math.max(0, list.indexOf(c)));
  }
  /** Scrolls a grid level so `c` is in view. @param {Item} c @param {0|1} l */
  function scrollTo(c, l) {
    const r = cache[l].rects.get(c);
    if (r) scroll[l].set(clamp(r.y - o.frame().H / 2 + r.w / 2, 0, cache[l].max));
  }

  let z0 = 0;
  /** @type {{z: number, t: number}[]} */
  let zh = [];
  const g = gestures(o.surface, {
    skip: o.skip, control: o.control, blocked: o.blocked,
    tap: (p) => o.tap(cardAt(p.x, p.y), level()),
    hold: (p) => {
      const c = level() <= 1 ? cardAt(p.x, p.y) : null;
      return c ? o.hold(c) : false;
    },
    drag: (p) => {
      for (const l of [0, 1]) fling[l] = 0;
      const lv = level();
      if (lv <= 1) {
        const s0 = scroll[lv].v;
        scrolling = lv;
        return {
          move: (q) => { scroll[lv].set(rubber(s0 - q.dy, 0, cache[lv].max)); kick(); },
          end: (_q, v) => { scrolling = null; fling[lv] = -v.y; kick(); },
        };
      }
      if (lv === 2) {
        const c = list[Math.round(feed.v)];
        if (Math.abs(p.dy) > Math.abs(p.dx) * 1.2 && c && cardAt(p.x0, p.y0) === c) return o.pull(c);
        const f0 = feed.v, n = list.length - 1, step = feedWidth(o.frame()) * 0.66;
        return {
          move: (q) => { feed.set(rubber(f0 - q.dx / step, 0, n, 0.3)); kick(); },
          end: (_q, v) => { feed.to = clamp(Math.round(feed.v - (v.x / step) * 0.38), 0, n); kick(); },
        };
      }
      return o.turn();
    },
    pinch: {
      start(i) {
        const lv = level();
        const anchor = lv <= 1 ? cardAt(i.mid.x, i.mid.y) ?? nearest(i.mid) : list[Math.round(feed.v)] ?? null;
        z0 = Z.v;
        zh = [];
        align(anchor);
        kick();
      },
      move(i) {
        // One level per doubling (or halving) of the distance between the fingers.
        Z.set(clamp(z0 + Math.log2(i.d / i.d0), -0.25, 3.25));
        zh.push({ z: Z.v, t: performance.now() });
        if (zh.length > 5) zh.shift();
        kick();
      },
      end() {
        const vz = zh.length > 1 ? (zh[zh.length - 1].z - zh[0].z) / Math.max(0.016, (zh[zh.length - 1].t - zh[0].t) / 1000) : 0;
        const from = Math.round(z0);
        // A flick may carry one level further than the fingers went, no more.
        let to = Math.round(Z.v + clamp(vz * 0.06, -0.5, 0.5));
        if (Math.abs(Z.v - from) < 1.5) to = clamp(to, from - 1, from + 1);
        setLevel(clamp(to, 0, 3));
      },
    },
    change: () => kick(),
  });

  // Desktop: ctrl/⌘ + wheel (or a trackpad pinch) zooms; the wheel scrolls.
  let wheelTimer = 0;
  o.surface.addEventListener('wheel', (e) => {
    if (/** @type {Element} */ (e.target).closest(o.skip) || o.blocked()) return;
    e.preventDefault();
    const lv = level();
    if (e.ctrlKey || e.metaKey) {
      if (!g.busy && !wheelTimer) align(lv <= 1 ? cardAt(e.clientX, e.clientY) ?? nearest({ x: e.clientX, y: e.clientY }) : list[Math.round(feed.v)] ?? null);
      Z.set(clamp(Z.v - e.deltaY * 0.012, 0, 3));
      clearTimeout(wheelTimer);
      wheelTimer = window.setTimeout(() => { wheelTimer = 0; setLevel(Math.round(Z.v)); }, 160);
    } else if (lv <= 1) scroll[lv].set(clamp(scroll[lv].v + e.deltaY, 0, cache[lv].max));
    else if (lv === 2) feed.to = clamp(feed.to + Math.sign(e.deltaY + e.deltaX), 0, list.length - 1);
    kick();
  }, { passive: false });

  /** @param {number} l */
  function setLevel(l) {
    l = clamp(l, 0, list.length ? 3 : 1);
    if (l <= 1 && Z.v > 1.5) { const c = list[Math.round(feed.v)]; if (c) scrollTo(c, /** @type {0|1} */ (l)); }
    Z.to = l;
    kick();
  }

  /**
   * Shows `next` in order. Cards that stay glide to their new places,
   * those that leave fade where they were, newcomers rise a beat later.
   * @param {Item[]} next @param {{instant?: boolean}} [opt]
   */
  function reflow(next, opt = {}) {
    const cur = list[Math.round(feed.v)];
    const was = new Set(list);
    const before = new Map(list.map((c) => [c, c.rr ? { ...c.rr } : null]));
    list = next;
    for (const c of next) if (!all.has(c.id)) { all.set(c.id, c); o.host.append(c.el); }
    cache = layouts();
    const k = cur ? next.indexOf(cur) : -1;
    feed.set(k >= 0 ? k : clamp(Math.round(feed.v), 0, Math.max(0, next.length - 1)));
    for (const l of [0, 1]) scroll[l].set(clamp(scroll[l].v, 0, cache[l].max));
    if (!next.length && Z.to > 1) Z.set(1);
    for (const c of was) if (!next.includes(c) && c.rr && !c.flying) { c.ghost = { ...c.rr }; c.fade.to = 0; }
    const arriving = next.filter((c) => !was.has(c));
    render();
    if (!opt.instant) {
      for (const c of next) {
        const b = before.get(c), n = c.rr;
        if (!b || !n || Z.v > 1.5) continue; // the feed closes its own gaps
        c.off.x.set(c.off.x.v + b.x - n.x);
        c.off.y.set(c.off.y.v + b.y - n.y);
        c.off.x.to = 0;
        c.off.y.to = 0;
      }
      const top = Math.min(...arriving.map((c) => c.rr?.y ?? 0));
      for (const c of arriving) {
        c.ghost = null;
        c.fade.set(-0.35 - Math.min(0.5, ((c.rr?.y ?? top) - top) / o.frame().H));
        c.fade.to = 1;
      }
    }
    kick();
  }

  return {
    item, reflow, setLevel, render, kick, align, scrollTo,
    Z, feed, dragY, scroll,
    get list() { return list; },
    get level() { return level(); },
    get busy() { return g.busy; },
    get pinching() { return g.pinching; },
    /** The card in the middle of the feed. */
    current: () => list[Math.round(feed.v)] ?? null,
    /** Re-measures after a resize. */
    relayout() { cache = layouts(); render(); },
    /** Re-labels after the language or grouping changed. */
    labels() { cache = layouts(); render(); },
    /** The feed index each card would have with the leaving ones gone. @param {Item} c */
    index: (c) => fidx.get(c) ?? list.indexOf(c),
  };
}
