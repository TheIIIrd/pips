// @ts-check
// DEVELOPMENT STUB of the WebAssembly core (crates/pips-wasm). Same
// functions, same field names, same {code, message} errors — so the app
// can be built and clicked through before the core is compiled.
//
// It is NOT secure: signatures are fake, keys are derived trivially, QR
// symbols scan only when dev/qrcode.js is loaded. core.js loads it only on
// localhost, behind a banner,
// with a separate database.

import { PALETTES } from '../dev/palettes.js';

const enc = new TextEncoder();
const dec = new TextDecoder();

/** @param {string} code @param {string} message */
const fail = (code, message) => { throw { code, message }; };

const B45 = '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:';
/** @param {Uint8Array} b */
function b45enc(b) {
  let s = '';
  for (let i = 0; i + 1 < b.length; i += 2) {
    let n = b[i] * 256 + b[i + 1];
    s += B45[n % 45]; n = Math.floor(n / 45); s += B45[n % 45] + B45[Math.floor(n / 45)];
  }
  if (b.length % 2) { const n = b[b.length - 1]; s += B45[n % 45] + B45[Math.floor(n / 45)]; }
  return s;
}
/** @param {string} s */
function b45dec(s) {
  if (s.length % 3 === 1) return null;
  const out = [];
  for (let i = 0; i < s.length; i += 3) {
    const c = [...s.slice(i, i + 3)].map((ch) => B45.indexOf(ch));
    if (c.some((x) => x < 0)) return null;
    if (c.length === 3) { const n = c[0] + c[1] * 45 + c[2] * 2025; if (n > 65535) return null; out.push(n >> 8, n & 255); }
    else { const n = c[0] + c[1] * 45; if (n > 255) return null; out.push(n); }
  }
  return new Uint8Array(out);
}

/** @param {number} kind @param {any} obj */
const pack = (kind, obj) => new Uint8Array([kind, ...enc.encode(JSON.stringify(obj))]);
/** @param {Uint8Array} b */
const unpack = (b) => { try { return JSON.parse(dec.decode(b.subarray(1))); } catch { return fail('invalid_card', 'stub: unreadable'); } };
/** @param {Uint8Array} b */
const hex = (b) => [...b].map((x) => x.toString(16).padStart(2, '0')).join('');
/** @param {string} s */
const unhex = (s) => new Uint8Array(s.match(/../g)?.map((x) => parseInt(x, 16)) ?? []);
/** Cheap stand-in hash: FNV-1a widened to 32 bytes. @param {string} s */
function fakeHash(s) {
  const out = new Uint8Array(32);
  let h = 2166136261;
  for (let i = 0; i < 32; i++) {
    for (const ch of s + i) h = Math.imul(h ^ ch.charCodeAt(0), 16777619) >>> 0;
    out[i] = h & 255;
  }
  return hex(out);
}
/** @param {string} k */
const keyData = (k) => ({ hex: k, short: `${k.slice(0, 2)}·${k.slice(2, 4)}·${k.slice(4, 6)}·${k.slice(6, 8)}` });
/** @param {Uint8Array} b */
const b64 = (b) => btoa(String.fromCharCode(...b)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
/** @param {string} s */
const unb64 = (s) => Uint8Array.from(atob(s.replace(/-/g, '+').replace(/_/g, '/')), (c) => c.charCodeAt(0));

/** Loads the vendored QR generator (MIT, Kazuhiko Arase) so stub codes scan. */
export default async function init() {
  if (/** @type {any} */ (globalThis).qrcode) return;
  await new Promise((resolve) => {
    const tag = document.createElement('script');
    tag.src = new URL('../dev/qrcode.js', import.meta.url).href;
    tag.onload = tag.onerror = resolve;
    document.head.append(tag);
  });
}

export function palettes() {
  return PALETTES.map((p) => ({ id: p.id, slug: p.slug, name: p.slug, summary: '', credit: null, colors: p.colors }));
}

/** @param {Uint8Array} bytes */
export function parsePalette(bytes) {
  const colors = dec.decode(bytes).split(/\s+/).filter(Boolean).map((h) => [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16)));
  if (colors.length < 2 || colors.some((c) => c.some(Number.isNaN))) fail('bad_palette', 'stub: .hex only');
  return { name: null, colors };
}

/** @param {any} card */
function check(card) {
  if (card.width < 1 || card.height < 1 || card.pixels.length !== card.width * card.height * card.frames) fail('invalid_card', 'pixel count');
  if (card.link && !/^[a-z0-9-]+(\.[a-z0-9-]+)+([/?#][!-~]*)?$/.test(card.link)) fail('invalid_card', 'bad link');
}

/** @param {any} card */
export function shareSize(card) {
  check(card);
  return 120 + Math.round(card.pixels.length / 5) + enc.encode(card.title + card.author + card.link).length;
}

/** @param {any} card @param {number} frame */
export function render(card, frame) {
  check(card);
  const colors = 'builtin' in card.palette ? PALETTES.find((p) => p.id === card.palette.builtin)?.colors : card.palette.inline;
  if (!colors) fail('invalid_card', 'unknown palette');
  const n = card.width * card.height, rgba = new Uint8Array(n * 4);
  for (let i = 0; i < n; i++) {
    const px = card.pixels[frame * n + i];
    if (px === 0 && !card.opaque) continue;
    const c = colors[px] ?? [255, 0, 255];
    rgba.set([c[0], c[1], c[2], 255], i * 4);
  }
  return { width: card.width, height: card.height, rgba };
}

// Short field names on the wire: the stub has no binary format, and long
// JSON keys alone would push a card past one code.
const SHORT = { width: 'w', height: 'h', opaque: 'o', palette: 'pl', frame: 'f', frames: 'fr', delay_ms: 'd', title: 't', author: 'a', handle: 'hd', link: 'l' };
const LONG = Object.fromEntries(Object.entries(SHORT).map(([k, v]) => [v, k]));
/** @param {any} o @param {Record<string, string>} map */
const rename = (o, map) => Object.fromEntries(Object.entries(o).map(([k, v]) => [map[k] ?? k, v]));

/** Packs the pixels as tightly as the stub can, so a card fits one code. @param {any} full */
function serialiseCard(full) {
  const card = rename({ ...full, pixels: undefined }, SHORT);
  const px = full.pixels;
  if (px.every((/** @type {number} */ v) => v < 16)) {
    const packed = new Uint8Array(Math.ceil(px.length / 2));
    for (let i = 0; i < px.length; i++) packed[i >> 1] |= px[i] << (i & 1 ? 0 : 4);
    // Runs of one colour, up to 16 per byte: (run - 1) << 4 | colour.
    const runs = [];
    for (let i = 0; i < px.length;) {
      let j = i;
      while (j < px.length && px[j] === px[i] && j - i < 16) j++;
      runs.push(((j - i - 1) << 4) | px[i]);
      i = j;
    }
    return runs.length < packed.length
      ? { ...card, pixels: undefined, r: b64(new Uint8Array(runs)), n: px.length }
      : { ...card, pixels: undefined, p4: b64(packed), n: px.length };
  }
  return { ...card, pixels: undefined, p8: b64(px), n: px.length };
}
/** @param {any} c */
function reviveCard(c) {
  const { p4, p8, r, n, ...short } = c;
  const rest = rename(short, LONG);
  let pixels;
  if (p8) pixels = unb64(p8);
  else if (r) {
    pixels = new Uint8Array(n);
    let at = 0;
    for (const b of unb64(r)) { pixels.fill(b & 15, at, Math.min(n, at + (b >> 4) + 1)); at += (b >> 4) + 1; }
  } else {
    const packed = unb64(p4);
    pixels = new Uint8Array(n);
    for (let i = 0; i < n; i++) pixels[i] = (packed[i >> 1] >> (i & 1 ? 0 : 4)) & 15;
  }
  return { ...rest, pixels };
}

/** @param {Uint8Array} payload */
export function inspectCard(payload) {
  if (payload[0] !== 1) fail('invalid_card', 'stub: not a card');
  const p = unpack(payload);
  return { card: reviveCard(p.card), digest: fakeHash(JSON.stringify(p.card)), author: p.author ? keyData(p.author) : null };
}

/** @param {Uint8Array} bytes */
export function openEnvelope(bytes) {
  if (bytes[0] !== 0x45) fail('invalid_code', 'stub: not an envelope');
  const e = unpack(bytes);
  // The card travels as JSON inside the envelope rather than as base64 of
  // its bytes, which would grow it by a third; its bytes are rebuilt here.
  const payload = pack(1, e.card);
  const i = inspectCard(payload);
  const cert = e.cert ? unpack(unb64(e.cert)) : null;
  return {
    card: i.card, payload, digest: i.digest, author: i.author, unknown_extensions: 0,
    certificate: cert ? { edition: cert.edition, of: cert.of, holder: keyData(cert.holder), issued_day: cert.day, bytes: unb64(e.cert) } : null,
    sender: e.sender ? { nick: e.sender.nick, key: keyData(e.sender.key) } : null,
  };
}

/** @param {string} text */
/** Import needs the real core. @returns {never} */
export function importPicture() {
  throw { code: 'unsupported', message: 'import needs the WebAssembly core' };
}

export function classify(text) {
  text = text.trim();
  if (text.startsWith('PIPS:KEY:1:')) return { kind: 'paper_key' };
  let bytes = null;
  if (text.startsWith('https://')) { try { bytes = parseLink(text); } catch { return { kind: 'unknown' }; } }
  else bytes = b45dec(text);
  if (!bytes || !bytes.length) return { kind: 'unknown' };
  if (bytes[0] === 0x45) return { kind: 'envelope', bytes };
  if (bytes[0] === 0x46) return { kind: 'frame', bytes };
  if (bytes[0] === 0x52) { const r = unpack(bytes); return { kind: 'recipient', key: keyData(r.key), nick: r.nick, bytes }; }
  return { kind: 'unknown' };
}

/** @param {string} base @param {Uint8Array} env */
export function cardLink(base, env) {
  if (!base.startsWith('https://') || base.includes('#')) fail('bad_link', 'bad base');
  return `${base}#${b64(env)}`;
}

/** @param {string} url */
export function parseLink(url) {
  const f = url.split('#')[1];
  if (!f) fail('bad_link', 'no card');
  try { const b = unb64(f); if (b[0] !== 0x45) throw 0; return b; } catch { return fail('bad_link', 'damaged'); }
}

const CAROUSEL_CHUNK = 400;
const SINGLE_MAX = 1100; // the stub's cards are bulkier than real ones
/** @param {Uint8Array} bytes */
export function qrPlan(bytes) {
  return bytes.length <= SINGLE_MAX ? { kind: 'single', text: b45enc(bytes) } : { kind: 'carousel', fragments: Math.ceil(bytes.length / CAROUSEL_CHUNK) };
}

/** A real QR symbol when the page has loaded dev/qrcode.js; otherwise a
 * pattern that looks like one and is not. @param {string} text */
export function qrMatrix(text) {
  const lib = /** @type {any} */ (globalThis).qrcode;
  if (lib) {
    const qr = lib(0, 'L');
    qr.addData(text, /^[0-9A-Z $%*+\-./:]*$/.test(text) ? 'Alphanumeric' : 'Byte');
    qr.make();
    const size = qr.getModuleCount(), modules = new Uint8Array(size * size);
    for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) modules[y * size + x] = qr.isDark(y, x) ? 1 : 0;
    return { size, modules };
  }
  const size = 21 + 4 * Math.min(30, Math.floor(text.length / 40));
  const modules = new Uint8Array(size * size);
  let s = 2166136261;
  for (const ch of text) s = Math.imul(s ^ ch.charCodeAt(0), 16777619) >>> 0;
  for (let i = 0; i < modules.length; i++) { s = (Math.imul(s, 1103515245) + 12345) >>> 0; modules[i] = (s >>> 16) & 1; }
  /** @param {number} ox @param {number} oy */
  const finder = (ox, oy) => {
    for (let y = -1; y < 8; y++) for (let x = -1; x < 8; x++) {
      const X = ox + x, Y = oy + y;
      if (X < 0 || Y < 0 || X >= size || Y >= size) continue;
      const ring = Math.max(Math.abs(x - 3), Math.abs(y - 3));
      modules[Y * size + X] = ring === 4 || ring === 2 ? 0 : 1;
    }
  };
  finder(0, 0); finder(size - 7, 0); finder(0, size - 7);
  return { size, modules };
}

/** @param {string} text */
export function qrSvg(text) {
  const m = qrMatrix(text);
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${m.size} ${m.size}"></svg>`;
}

export class Identity {
  /** @param {string} seedHex */
  constructor(seedHex) { this.s = seedHex; this.k = fakeHash(`key:${seedHex}`); }
  /** @param {Uint8Array} seed */
  static fromSeed(seed) { if (seed.length !== 32) fail('bad_seed', 'a seed is 32 bytes'); return new Identity(hex(seed)); }
  /** @param {string} words */
  static fromWords(words) {
    const w = words.trim().split(/\s+/);
    if (w.length !== 24 || w[0] !== 'stub') fail('bad_words', 'stub words start with "stub"');
    return new Identity(w.slice(1, 17).map((x) => x.padStart(2, '0')).join('').padEnd(64, '0').slice(0, 64));
  }
  /** @param {string} text */
  static fromPaper(text) { return new Identity(text.replace('PIPS:KEY:1:', '').padEnd(64, '0').slice(0, 64)); }
  seed() { return unhex(this.s); }
  words() { return ['stub', ...this.s.match(/../g).slice(0, 16), 'demo', 'demo', 'demo', 'demo', 'demo', 'demo', 'demo'].join(' '); }
  paper() { return `PIPS:KEY:1:${this.s.toUpperCase()}`; }
  publicKey() { return keyData(this.k); }
  /** @param {any} card */
  signCard(card) { check(card); return pack(1, { card: serialiseCard(card), author: this.k }); }
  /** @param {string} nick */
  recipientCode(nick) { return b45enc(pack(0x52, { key: this.k, nick })); }
  /** @param {Uint8Array} payload @param {number} edition @param {number} of @param {Uint8Array} recipient @param {number} day */
  issue(payload, edition, of, recipient, day) {
    if (inspectCard(payload).author?.hex !== this.k) fail('not_author', 'only the author issues');
    return pack(0x43, { edition, of, holder: unpack(recipient).key, day });
  }
  /** @param {Uint8Array} payload @param {Uint8Array|undefined} cert */
  sealAsAuthor(payload, cert) { inspectCard(payload); return pack(0x45, { card: unpack(payload), cert: cert ? b64(cert) : null }); }
  /** @param {Uint8Array} payload @param {string} nick */
  sealAsSender(payload, nick) { inspectCard(payload); return pack(0x45, { card: unpack(payload), sender: { nick, key: this.k } }); }
}

export class Carousel {
  /** @param {Uint8Array} bytes */
  constructor(bytes) { this.b = bytes; this.n = Math.ceil(bytes.length / CAROUSEL_CHUNK); }
  fragments() { return this.n; }
  /** @param {number} seq */
  frameText(seq) {
    const i = seq % this.n;
    return b45enc(pack(0x46, { i, n: this.n, d: b64(this.b.subarray(i * CAROUSEL_CHUNK, (i + 1) * CAROUSEL_CHUNK)) }));
  }
}

export class Joiner {
  constructor() { /** @type {Map<number, Uint8Array>} */ this.parts = new Map(); this.n = 0; }
  /** @param {Uint8Array} frame */
  push(frame) {
    const f = unpack(frame);
    this.n = f.n;
    this.parts.set(f.i, unb64(f.d));
    if (this.parts.size < this.n) return { kind: 'partial', have: this.parts.size, need: this.n };
    const all = [...Array(this.n).keys()].flatMap((i) => [...(this.parts.get(i) ?? [])]);
    return { kind: 'done', bytes: new Uint8Array(all) };
  }
}
