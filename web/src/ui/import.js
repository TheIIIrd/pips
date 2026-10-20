// @ts-check
// A new card from a PNG or Aseprite file: pick, see what the conversion
// did, adjust, name, sign. The file never leaves the device.
//
// The core does the work (pips-raster); this screen only shows its report
// and passes the options back. Each change re-runs the import from the
// original bytes, so options never compound.

import { h, icon, byteLimit, fitBytes } from '../dom.js';
import { t } from '../i18n.js';
import { app, addOwn } from '../state.js';
import { frame, FRAME_SHADES } from './card.js';
import { go, toast } from './shell.js';

const ACCEPT = '.png,.aseprite,.ase,image/png';

export function openImport() {
  /** @type {Uint8Array|null} */
  let bytes = null;
  let fileName = '';
  let autoTitle = '';
  /** @type {string|null} */
  let sourceUrl = null; // PNG only: the browser can show it as "before"
  const opts = { keepScale: false, firstFrameOnly: false, /** @type {number|undefined} */ palette: undefined };
  let shade = 0;
  let timer = 0;
  /** @type {any} */
  let result = null;

  const input = /** @type {HTMLInputElement} */ (h('input', { type: 'file', accept: ACCEPT, hidden: true }));
  const body = h('div', { class: 'import-body' });
  const close = () => {
    clearInterval(timer);
    if (sourceUrl) URL.revokeObjectURL(sourceUrl);
    overlay.remove();
  };
  const overlay = h('section', { class: 'overlay', role: 'dialog', 'aria-modal': 'true' },
    h('div', { class: 'bar' },
      h('button', { class: 'icon-btn', 'aria-label': t('ask.cancel'), onclick: close }, icon('close')),
      h('div', { class: 'title' }, h('h1', {}, t('import.title')))),
    body, input);

  const title = byteLimit(/** @type {HTMLInputElement} */ (h('input', { placeholder: t('import.name.ph'), autocomplete: 'off' })), 32);

  input.addEventListener('change', async () => {
    const f = input.files?.[0];
    input.value = '';
    if (!f) return;
    if (f.size > 16 * 1024 * 1024) return toast(t('import.err.too_large'));
    bytes = new Uint8Array(await f.arrayBuffer());
    fileName = f.name;
    if (sourceUrl) URL.revokeObjectURL(sourceUrl);
    sourceUrl = /\.png$/i.test(f.name) || f.type === 'image/png' ? URL.createObjectURL(f) : null;
    // The file name is a starting title, replaced with the file, but never
    // over something the person typed.
    const fromName = fitBytes(f.name.replace(/\.[^.]+$/, ''), 32);
    if (!title.value || title.value === autoTitle) title.value = fromName;
    autoTitle = fromName;
    Object.assign(opts, { keepScale: false, firstFrameOnly: false, palette: undefined });
    run();
  });

  function pick() {
    input.click();
  }

  function run() {
    clearInterval(timer);
    if (!bytes) {
      body.replaceChildren(h('div', { class: 'empty' },
        h('b', {}, t('import.pick.title')), t('import.pick.hint'),
        h('button', { class: 'btn key wide', style: { marginTop: '18px' }, onclick: pick }, icon('file'), t('import.pick'))));
      return;
    }
    try {
      result = app.core.importPicture(bytes, { ...opts });
      paint(null);
    } catch (e) {
      result = null;
      paint(/** @type {any} */ (e));
    }
  }

  /** @param {{code: string, args?: number[], message?: string}|null} error */
  function paint(error) {
    const parts = [];
    if (error) {
      parts.push(h('div', { class: 'import-error' }, h('b', {}, explain(error)), h('small', {}, fileName)));
    } else {
      const card = { ...result.card, title: title.value, frame: shade, author: app.profile.nick };
      parts.push(beforeAfter(card), h('p', { class: 'import-report' }, report(result)));
    }
    parts.push(options(error));
    if (!error) parts.push(naming());
    parts.push(h('div', { class: 'row-btns import-actions' },
      error ? null : h('button', { class: 'btn key', onclick: save }, t('import.save')),
      h('button', { class: 'btn', onclick: pick }, t('import.other'))));
    body.replaceChildren(...parts);
  }

  /** @param {any} card */
  function beforeAfter(card) {
    const after = frame({ card, rec: /** @type {any} */ ({ digest: '', received: Date.now() }), author: null });
    animate(after, card);
    if (!sourceUrl) return h('div', { class: 'import-pair one' }, after);
    const img = h('img', { src: sourceUrl, alt: '' });
    return h('div', { class: 'import-pair' },
      h('figure', {}, h('div', { class: 'import-src' }, img), h('figcaption', {}, t('import.before'))),
      h('figure', {}, after, h('figcaption', {}, t('import.after'))));
  }

  /** Plays an animated card inside its frame. @param {HTMLElement} el @param {any} card */
  function animate(el, card) {
    if (card.frames < 2) return;
    const cv = /** @type {HTMLCanvasElement|null} */ (el.querySelector('.art canvas'));
    if (!cv) return;
    const g = /** @type {CanvasRenderingContext2D} */ (cv.getContext('2d'));
    const images = Array.from({ length: card.frames }, (_, i) => {
      const r = app.core.render(card, i);
      return new ImageData(new Uint8ClampedArray(r.rgba), r.width, r.height);
    });
    let i = 0;
    timer = window.setInterval(() => g.putImageData(images[(i = (i + 1) % images.length)], 0, 0), Math.max(card.delay_ms, 20));
  }

  /** @param {any} r */
  function report(r) {
    const c = r.card;
    const bits = [];
    bits.push(r.scale > 1
      ? t('import.size.scaled', r.sourceWidth, r.sourceHeight, c.width, c.height, r.scale)
      : t('import.size', c.width, c.height));
    if (c.frames > 1) bits.push(t('import.frames', c.frames, c.delay_ms));
    bits.push(c.palette.builtin != null
      ? t('import.palette.builtin', paletteName(c.palette.builtin))
      : t('import.palette.own', r.colors));
    if (r.changedPixels) bits.push(t('import.changed', r.changedPixels));
    const size = app.core.shareSize(c);
    bits.push(size <= 600 ? t('import.code.single', size) : t('import.code.carousel', size));
    return bits.join(' · ');
  }

  /** @param {{code: string, args?: number[]}|null} error */
  function options(error) {
    const rows = [];
    const scaled = result?.scale > 1 || opts.keepScale;
    if (scaled) rows.push(toggle('keepScale', t('import.opt.keep-scale')));
    if ((result?.sourceFrames ?? 0) > 1 || opts.firstFrameOnly || error?.code === 'unequal_delays' || error?.code === 'too_many_frames') {
      rows.push(toggle('firstFrameOnly', t('import.opt.first-frame')));
    }
    // Shown always: it is also the way out of "too many colours".
    const sel = /** @type {HTMLSelectElement} */ (h('select', {
      onchange: () => {
        opts.palette = sel.value === '' ? undefined : Number(sel.value);
        run();
      },
    }, h('option', { value: '' }, t('import.opt.palette.auto')),
    ...palettes().map((/** @type {any} */ p) => h('option', { value: String(p.id) }, `${p.name} · ${p.colors.length}`))));
    sel.value = opts.palette == null ? '' : String(opts.palette);
    rows.push(h('div', { class: `field${error?.code === 'too_many_colors' ? ' attention' : ''}` }, h('label', {}, t('import.opt.palette')), sel));
    return h('div', { class: 'group' }, ...rows);
  }

  /** @param {'keepScale'|'firstFrameOnly'} key @param {string} label */
  function toggle(key, label) {
    const box = /** @type {HTMLInputElement} */ (h('input', { type: 'checkbox' }));
    box.checked = opts[key];
    box.addEventListener('change', () => {
      opts[key] = box.checked;
      run();
    });
    return h('label', { class: 'check' }, box, h('span', {}, label));
  }

  function naming() {
    title.oninput = () => {
      const cap = body.querySelector('.cap b');
      if (cap) cap.textContent = title.value || '—';
    };
    const shades = h('div', { class: 'shades', role: 'radiogroup', 'aria-label': t('import.shade') },
      ...FRAME_SHADES.map((s, i) => h('button', {
        class: 'shade', role: 'radio', 'aria-checked': String(i === shade), style: { background: s.paper },
        onclick: () => {
          shade = i;
          run();
        },
      })));
    return h('div', { class: 'group' },
      h('div', { class: 'field' }, h('label', {}, t('import.name')), title),
      h('h2', {}, t('import.shade')), shades);
  }

  async function save() {
    if (!result) return;
    const p = app.profile;
    const card = {
      ...result.card,
      title: title.value.trim(),
      frame: shade,
      author: p.nick,
      handle: p.handle,
      link: p.link,
    };
    try {
      const r = await addOwn(app.identity.signCard(card));
      close();
      go('give', r.digest);
      toast(t('import.saved'));
    } catch (e) {
      toast(explain(/** @type {any} */ (e)));
    }
  }

  document.body.append(overlay);
  run();
  pick();
}

/** @type {any[]|null} */
let paletteCache = null;
const palettes = () => (paletteCache ??= app.core.palettes());

/** @param {number} id */
function paletteName(id) {
  return palettes().find((/** @type {any} */ p) => p.id === id)?.name ?? `#${id}`;
}

/** A translated reason, with the numbers the core attached.
 * @param {{code: string, args?: number[], message?: string}} e */
function explain(e) {
  const key = `import.err.${e?.code}`;
  const text = t(key, ...(e?.args ?? []));
  return text === key ? `${t('err.generic')}: ${e?.message ?? e}` : text;
}
