// @ts-check
// The turnable card: near the last level the centre card hands over to a
// 3D twin with two faces. Only the finger that started the turn turns it;
// a flick turns one face further at most, never a spin. Leaving the card
// level turns it face up first, smoothly, before the twin hands back.

import { h } from '../dom.js';
import { Spring, SPRINGS, loop, clamp } from '../kit/motion.js';

/** The nearest angle with the front up. @param {number} a */
const frontOf = (a) => Math.round(a / 360) * 360;

/** @param {{host: HTMLElement}} o */
export function createTurn(o) {
  const rotY = new Spring(0, SPRINGS.turn), rotX = new Spring(0, SPRINGS.tilt);
  const front = h('div', { class: 'face front-face' });
  const back = h('div', { class: 'face back-face' });
  const flip = h('div', { class: 'turn-flip' }, front, back);
  const el = h('div', { class: 'turn', 'aria-live': 'polite' }, flip);
  el.hidden = true;
  o.host.append(el);
  /** @type {string|null} */
  let of = null;
  let lastZ = 0, wantTurn = 0, fit = 1; // wantTurn: a turn asked for, and its direction
  loop.add((dt) => {
    const a = rotY.step(dt), b = rotX.step(dt);
    return a || b;
  });

  const turned = () => Math.abs(rotY.v - frontOf(rotY.v)) > 1.5 || Math.abs(rotX.v) > 1.5;
  const showsBack = () => { const a = ((rotY.v % 360) + 360) % 360; return a > 90 && a < 270; };

  return {
    el,
    get backShown() { return showsBack(); },
    /** Whether the twin is out of its resting front-up pose. */
    get turned() { return turned(); },
    /**
     * Places the twin over the card drawn at `rr`, or hides it.
     * @param {number} z
     * @param {null | {id: string, el: HTMLElement, rr: {x: number, y: number, w: number, h: number, cap: number} | null}} c
     * @param {(id: string) => {front: HTMLElement, back: HTMLElement}} build faces for a card
     * @param {boolean} interactive
     */
    draw(z, c, build, interactive) {
      lastZ = z;
      // A turn asked for on the way to the card happens once there.
      if (wantTurn && z > 2.95) { rotY.to = Math.round(rotY.v / 180) * 180 + 180 * wantTurn; wantTurn = 0; loop.kick(); }
      if (!wantTurn && z < 2.95 && Math.round(rotY.to) % 360 !== 0) { rotY.to = frontOf(rotY.v); rotX.to = 0; loop.kick(); }
      const on = !!(c && c.rr && (z > 2.6 || (z > 1.6 && turned())));
      el.hidden = !on;
      if (!on || !c || !c.rr) {
        if (c) c.el.style.visibility = '';
        return;
      }
      c.el.style.visibility = 'hidden';
      if (of !== c.id) {
        of = c.id;
        const f = build(c.id);
        fit = 1;
        front.replaceChildren(f.front);
        back.replaceChildren(f.back);
      }
      const { x, y, w, h: hh, cap } = c.rr;
      Object.assign(el.style, { width: `${w}px`, height: `${hh}px`, transform: `translate3d(${x}px,${y}px,0)` });
      const fr = /** @type {HTMLElement|null} */ (front.firstElementChild);
      if (fr) { fr.style.width = `${w}px`; fr.style.setProperty('--cap', String(cap)); }
      // The back has no scrolling: if a long back does not fit, its type
      // gets a little smaller until it does.
      back.style.fontSize = `${Math.max(9, w * 0.037 * fit)}px`;
      const inner = /** @type {HTMLElement|null} */ (back.firstElementChild);
      if (inner && fit > 0.8 && inner.scrollHeight > inner.clientHeight + 1) { fit -= 0.04; loop.kick(); }
      flip.style.transform = `rotateX(${rotX.v}deg) rotateY(${rotY.v}deg)`;
      // Each face is hidden outright when turned away: backface-visibility
      // alone shows a mirrored front on some engines (Firefox).
      const b = showsBack();
      front.style.visibility = b ? 'hidden' : '';
      back.style.visibility = b ? '' : 'hidden';
      back.classList.toggle('live', b && z > 2.9 && interactive);
    },
    /** Rebuilds the faces next time (the card changed). */
    invalidate() { of = null; fit = 1; },
    /**
     * Turns over, or to a given side. `dir` 1 sends the right edge away,
     * -1 the left one. @param {boolean} [toBack] @param {1|-1} [dir]
     */
    flipOver(toBack, dir = 1) {
      const now = showsBack();
      if (toBack === now) return;
      if (lastZ < 2.95) { wantTurn = dir; loop.kick(); return; }
      rotY.to = Math.round(rotY.to / 180) * 180 + 180 * dir;
      loop.kick();
    },
    /** Face up at once (another card came). */
    reset() { rotY.set(frontOf(rotY.v)); rotX.set(0); },
    /** The drag that turns the card. @returns {import('../kit/gestures.js').DragHandler} */
    drag() {
      const ry0 = rotY.v, rx0 = rotX.v;
      return {
        move: (p) => { rotY.set(ry0 + p.dx * 0.36); rotX.set(clamp(rx0 - p.dy * 0.15, -18, 18)); loop.kick(); },
        end: (_p, v) => {
          const push = clamp(v.x * 0.05, -80, 80);
          rotY.to = Math.round((rotY.v + push) / 180) * 180;
          rotX.to = 0;
          loop.kick();
        },
      };
    },
  };
}
