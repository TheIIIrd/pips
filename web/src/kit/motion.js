// @ts-check
// Motion: springs, one shared frame loop, and the easings of the app.
//
// Everything that moves in the interface moves through here, so the whole
// app has one feel: change a spring in `SPRINGS` and every place that uses
// it changes with it. No DOM is touched; this module is plain arithmetic
// and runs under node for tests.

/** Whether the person asked for less motion: springs then arrive at once. */
export const REDUCED = typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;

/**
 * Stiffness and damping pairs, by role. Damping near 2·√stiffness is
 * critical: no overshoot. A little below gives weight.
 */
export const SPRINGS = /** @type {const} */ ({
  zoom: [120, 22], // the zoom axis: months · weeks · feed · card
  scroll: [170, 26], // settling a grid after a fling past an edge
  feed: [130, 23], // the feed's position, in cards
  turn: [150, 22], // turning a card over: weight without wobble
  tilt: [160, 24],
  drag: [160, 22], // a card let go after a pull
  glide: [130, 22], // things finding their new place after a change
  fade: [170, 26], // appearing and leaving
});

export class Spring {
  /** @param {number} v start value @param {readonly [number, number]} [kc] stiffness, damping */
  constructor(v, kc = SPRINGS.glide) {
    this.v = v;
    this.to = v;
    this.vel = 0;
    this.k = kc[0];
    this.c = kc[1];
  }

  /** Jumps there, at rest. @param {number} v */
  set(v) {
    this.v = this.to = v;
    this.vel = 0;
  }

  /** Advances by `dt` seconds; returns whether it is still moving. @param {number} dt */
  step(dt) {
    if (REDUCED) {
      this.v = this.to;
      this.vel = 0;
      return false;
    }
    const a = this.k * (this.to - this.v) - this.c * this.vel;
    this.vel += a * dt;
    this.v += this.vel * dt;
    if (Math.abs(this.to - this.v) < 1e-3 && Math.abs(this.vel) < 1e-2) {
      this.v = this.to;
      this.vel = 0;
      return false;
    }
    return true;
  }
}

/**
 * One requestAnimationFrame loop for everyone. A client is a function of
 * the frame time `dt` (seconds) that returns whether it wants another
 * frame; the loop sleeps when nobody does, and `kick` wakes it.
 */
export function frameLoop() {
  /** @type {Set<(dt: number) => boolean>} */
  const clients = new Set();
  let running = false, last = 0;
  const tick = (/** @type {number} */ now) => {
    const dt = Math.min(0.032, (now - last) / 1000 || 0.016);
    last = now;
    let busy = false;
    for (const f of clients) busy = f(dt) || busy;
    if (busy) requestAnimationFrame(tick);
    else running = false;
  };
  return {
    /** @param {(dt: number) => boolean} f */
    add(f) {
      clients.add(f);
      return () => clients.delete(f);
    },
    kick() {
      if (running) return;
      running = true;
      last = performance.now();
      requestAnimationFrame(tick);
    },
  };
}

/** The app's one loop. */
export const loop = frameLoop();

export const mix = (/** @type {number} */ a, /** @type {number} */ b, /** @type {number} */ t) => a + (b - a) * t;
export const clamp = (/** @type {number} */ v, /** @type {number} */ lo, /** @type {number} */ hi) => Math.max(lo, Math.min(hi, v));
/** Smoothstep: eases in and out. @param {number} t */
export const ease = (t) => t * t * (3 - 2 * t);
/** Cubic in-out, for flights. @param {number} t */
export const easeInOut = (t) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2);
/** Past either end a value gives a little, like rubber. @param {number} v @param {number} lo @param {number} hi @param {number} [k] */
export const rubber = (v, lo, hi, k = 0.35) => (v < lo ? lo + (v - lo) * k : v > hi ? hi + (v - hi) * k : v);

/**
 * A flight from one rectangle to another along a shallow arc, as a function
 * of time; `to` may be a function, for a target that moves meanwhile.
 * @param {{x: number, y: number, w: number}} from
 * @param {{x: number, y: number, w: number} | (() => {x: number, y: number, w: number})} to
 * @param {{start: number, dur?: number, arc?: number}} o
 */
export function flight(from, to, { start, dur = 620, arc = 50 }) {
  return {
    end: start + dur,
    /** @param {number} now @returns {{x: number, y: number, w: number, t: number}} */
    at(now) {
      const target = typeof to === 'function' ? to() : to;
      const t = easeInOut(clamp((now - start) / dur, 0, 1));
      return {
        x: mix(from.x, target.x, t),
        y: mix(from.y, target.y, t) - Math.sin(t * Math.PI) * arc,
        w: mix(from.w, target.w, t),
        t,
      };
    },
  };
}
