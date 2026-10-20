// @ts-check
// Touch arbitration: one owner per touch.
//
// Whoever claims a touch first keeps it until every finger has lifted;
// everything else waits. A finger that lands on a control (a text field, a
// button of the card) belongs to that control — but a second finger anywhere
// still makes a pinch of the whole surface, because a pinch is never meant
// for a control.
//
// The arbiter knows nothing about what the gestures mean. It reports:
//   tap      — a finger down and up without moving;
//   hold     — a finger kept still for HOLD_MS;
//   drag     — a finger moved past SLOP: `drag(start)` returns the handler
//              that owns it from then on, or null to ignore it;
//   pinch    — two fingers: start, move (distance and midpoint), end.

const SLOP = 8;
const HOLD_MS = 420;

/** @typedef {{x: number, y: number, x0: number, y0: number, dx: number, dy: number, id: number, target: Element|null}} Point */
/** @typedef {{move: (p: Point) => void, end: (p: Point, v: {x: number, y: number}) => void}} DragHandler */
/** @typedef {{d0: number, d: number, mid: {x: number, y: number}}} PinchInfo */

/**
 * @param {HTMLElement} el
 * @param {{
 *   skip?: string, control?: string, blocked?: () => boolean,
 *   tap?: (p: Point) => void, hold?: (p: Point) => boolean | void,
 *   drag?: (p: Point) => DragHandler | null,
 *   pinch?: { start: (i: PinchInfo) => void, move: (i: PinchInfo) => void, end: (i: PinchInfo) => void },
 *   change?: (busy: boolean) => void,
 * }} o
 */
export function gestures(el, o) {
  /** @type {Map<number, {x: number, y: number, x0: number, y0: number, target: Element|null}>} */
  const pts = new Map();
  /** @type {null | {kind: 'pending'|'drag'|'held'|'pinch'|'control', id: number, hold?: number, h?: DragHandler|null, d0?: number, hist: {x: number, y: number, t: number}[]}} */
  let g = null;

  const point = (/** @type {number} */ id) => {
    const p = /** @type {any} */ (pts.get(id));
    return /** @type {Point} */ ({ ...p, dx: p.x - p.x0, dy: p.y - p.y0, id });
  };
  const pinchInfo = () => {
    const [a, b] = [...pts.values()];
    return { d0: /** @type {number} */ (g?.d0), d: Math.hypot(a.x - b.x, a.y - b.y), mid: { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 } };
  };
  const velocity = () => {
    const h = g?.hist ?? [];
    if (h.length < 2) return { x: 0, y: 0 };
    const a = h[0], b = h[h.length - 1], dt = Math.max(1, b.t - a.t) / 1000;
    return { x: (b.x - a.x) / dt, y: (b.y - a.y) / dt };
  };
  const set = (/** @type {typeof g} */ v) => {
    const was = !!g;
    g = v;
    if (was !== !!g) o.change?.(!!g);
  };

  el.addEventListener('pointerdown', (e) => {
    const target = /** @type {Element} */ (e.target);
    if (o.skip && target.closest(o.skip)) return;
    if (o.blocked?.() && !g) return;
    const onControl = !!(o.control && target.closest(o.control));
    if (!onControl) try { el.setPointerCapture(e.pointerId); } catch { /* synthetic pointers */ }
    pts.set(e.pointerId, { x: e.clientX, y: e.clientY, x0: e.clientX, y0: e.clientY, target });

    if (pts.size === 2 && o.pinch && (!g || g.kind !== 'held')) {
      // The second finger takes over whatever the first one was doing.
      clearTimeout(g?.hold);
      g?.h?.end(point(g.id), { x: 0, y: 0 });
      /** @type {HTMLElement|null} */ (document.activeElement)?.blur?.();
      const [a, b] = [...pts.values()];
      set({ kind: 'pinch', id: e.pointerId, d0: Math.max(1, Math.hypot(a.x - b.x, a.y - b.y)), hist: [] });
      o.pinch.start(pinchInfo());
      return;
    }
    if (pts.size !== 1) return;
    if (onControl) return set({ kind: 'control', id: e.pointerId, hist: [] });
    const s = { kind: /** @type {'pending'} */ ('pending'), id: e.pointerId, hist: [], hold: 0 };
    set(s);
    if (o.hold) {
      s.hold = window.setTimeout(() => {
        if (g !== s || g.kind !== 'pending') return;
        if (o.hold?.(point(s.id)) !== false) /** @type {any} */ (g).kind = 'held';
      }, HOLD_MS);
    }
  });

  el.addEventListener('pointermove', (e) => {
    const p = pts.get(e.pointerId);
    if (!p) return;
    p.x = e.clientX;
    p.y = e.clientY;
    if (!g || g.kind === 'control' || g.kind === 'held') return;
    if (g.kind === 'pinch') {
      if (pts.size === 2) o.pinch?.move(pinchInfo());
      return;
    }
    if (e.pointerId !== g.id) return; // only the finger that owns the touch counts
    g.hist.push({ x: p.x, y: p.y, t: performance.now() });
    if (g.hist.length > 6) g.hist.shift();
    const pt = point(g.id);
    if (g.kind === 'pending') {
      if (Math.hypot(pt.dx, pt.dy) <= SLOP) return;
      clearTimeout(g.hold);
      g.h = o.drag?.(pt) ?? null;
      g.kind = 'drag';
    }
    g.h?.move(pt);
  });

  const up = (/** @type {PointerEvent} */ e) => {
    if (!pts.has(e.pointerId)) return;
    const pt = point(e.pointerId);
    const info = g?.kind === 'pinch' && pts.size === 2 ? pinchInfo() : null;
    pts.delete(e.pointerId);
    if (!g) return;
    if (g.kind === 'pinch') {
      // A pinch ends when its first finger lifts; the other one rests.
      if (info) o.pinch?.end(info);
      if (info) g.kind = 'held';
      if (!pts.size) set(null);
      return;
    }
    if (pts.size) return;
    const s = g;
    clearTimeout(s.hold);
    set(null);
    if (s.kind === 'pending' && e.type === 'pointerup') o.tap?.(pt);
    else if (s.kind === 'drag') s.h?.end(pt, velocity());
  };
  el.addEventListener('pointerup', up);
  el.addEventListener('pointercancel', up);

  return {
    /** Whether a touch is in progress. */
    get busy() { return !!g; },
    /** Whether a pinch is in progress. */
    get pinching() { return g?.kind === 'pinch'; },
  };
}
