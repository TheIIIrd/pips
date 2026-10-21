// @ts-check
// A new card from a PNG or Aseprite file: pick, see what the conversion
// did, adjust, name, sign. The file never leaves the device.
//
// A tab like the others: leaving it keeps what is under way (the file, the
// options, the title), so a person can look something up and come back.
// Only signing or the reset button start it over.
//
// The core does the work (pips-raster); this screen only shows its report
// and passes the options back. Each change re-runs the import from the
// original bytes, so options never compound.

import { h, icon, byteLimit, fitBytes } from '../dom.js';
import { t } from '../i18n.js';
import { app, addOwn } from '../state.js';
import { frame, FRAME_SHADES } from './card.js';
import { go, toast } from './shell.js';
import { iconButton } from '../kit/icon-button.js';

const ACCEPT = '.png,.aseprite,.ase,image/png';

export function importView() {
  /** @type {Uint8Array|null} */
  let bytes = null;
  let fileName = '';
  let autoTitle = '';
  /** @type {string|null} */
  let sourceUrl = null; // what the browser can show of the file, for "original"
  let showOriginal = false;
  const opts = {
    keepScale: false, firstFrameOnly: false,
    /** @type {number|undefined} */ palette: undefined,
    /** @type {number|undefined} */ fit: undefined,
  };
  let autoFit = false; // the screen chose "own, fitted" by itself
  let shade = 0;
  let timer = 0;
  /** @type {any} */
  let result = null;
  /** @type {{code: string, args?: number[], message?: string}|null} */
  let error = null;

  // One fixed skeleton; only the contents of its slots change, so nothing
  // above the buttons ever moves.
  const input = /** @type {HTMLInputElement} */ (h('input', { type: 'file', accept: ACCEPT, hidden: true }));
  const preview = h('button', { class: 'import-preview', onclick: () => onPreview() });
  const status = h('p', { class: 'import-status', 'aria-live': 'polite' });
  const title = byteLimit(/** @type {HTMLInputElement} */ (h('input', { placeholder: t('import.name.ph'), autocomplete: 'off' })), 32);
  // Values: '' exact colours, 'fit:N' own colours fitted to N, else a built-in id.
  const palette = /** @type {HTMLSelectElement} */ (h('select', {
    onchange: () => {
      const v = palette.value;
      opts.fit = v.startsWith('fit:') ? Number(v.slice(4)) : undefined;
      opts.palette = v === '' || v.startsWith('fit:') ? undefined : Number(v);
      autoFit = false;
      run();
    },
  }, h('option', { value: '' }, t('import.opt.palette.auto')),
  h('option', { value: 'fit:32' }, t('import.opt.palette.fit', 32)),
  h('option', { value: 'fit:16' }, t('import.opt.palette.fit', 16)),
  ...palettes().map((/** @type {any} */ p) => h('option', { value: String(p.id) }, paletteLabel(p)))));
  const paletteField = h('div', { class: 'field' }, h('label', {}, t('import.opt.palette')), palette);
  const shades = h('div', { class: 'shades', role: 'radiogroup', 'aria-label': t('import.shade') });
  const save = /** @type {HTMLButtonElement} */ (h('button', { class: 'btn key', onclick: () => sign() }, t('import.save')));
  const pickBtn = h('button', { class: 'btn', onclick: () => input.click() });
  const extras = h('div', { class: 'group import-extras' });

  const body = h('div', { class: 'import-body' },
    preview, status,
    h('div', { class: 'group' }, paletteField,
      h('div', { class: 'field' }, h('label', {}, t('import.name')), title),
      h('h2', {}, t('import.shade')), shades),
    h('div', { class: 'row-btns import-actions' }, save, pickBtn),
    extras);

  /** Back to an empty frame: after signing, or when the person asks. */
  const reset = () => {
    clearInterval(timer);
    if (sourceUrl) URL.revokeObjectURL(sourceUrl);
    bytes = null;
    fileName = autoTitle = '';
    sourceUrl = null;
    showOriginal = autoFit = false;
    Object.assign(opts, { keepScale: false, firstFrameOnly: false, palette: undefined, fit: undefined });
    palette.value = '';
    shade = 0;
    result = error = null;
    title.value = '';
    paint();
  };
  const resetBtn = iconButton({ icon: 'close', label: t('import.reset'), onClick: reset });
  const el = h('section', { class: 'view import-view' },
    h('div', { class: 'bar' }, h('div', { class: 'title' }, h('h1', {}, t('import.title'))), resetBtn.el),
    body, input);

  title.addEventListener('input', () => {
    const cap = preview.querySelector('.cap b');
    if (cap) cap.textContent = title.value || t('import.name.ph');
    resetBtn.el.hidden = !bytes && !title.value;
  });

  input.addEventListener('change', async () => {
    const f = input.files?.[0];
    input.value = '';
    if (!f) return;
    if (f.size > 16 * 1024 * 1024) return toast(t('import.err.too_large'));
    bytes = new Uint8Array(await f.arrayBuffer());
    fileName = f.name;
    if (sourceUrl) URL.revokeObjectURL(sourceUrl);
    sourceUrl = /\.(aseprite|ase)$/i.test(f.name) ? null : URL.createObjectURL(f);
    showOriginal = false;
    // The file name is a starting title, replaced with the file, but never
    // over something the person typed.
    const fromName = fitBytes(f.name.replace(/\.[^.]+$/, ''), 32);
    if (!title.value || title.value === autoTitle) title.value = fromName;
    autoTitle = fromName;
    Object.assign(opts, { keepScale: false, firstFrameOnly: false, palette: undefined, fit: undefined });
    autoFit = false;
    palette.value = '';
    run();
  });

  function onPreview() {
    if (!bytes) return input.click();
    if (result && sourceUrl) {
      showOriginal = !showOriginal;
      paint();
    }
  }

  function run() {
    if (bytes) {
      try {
        result = app.core.importPicture(bytes, { ...opts });
        error = null;
      } catch (e) {
        result = null;
        error = /** @type {any} */ (e);
        // Too many colours for exact: fit the picture's own palette rather
        // than stop. It is said so in the status, and any palette can still
        // be chosen; the original is a tap away for comparison.
        if (error?.code === 'too_many_colors' && opts.palette == null && opts.fit == null) {
          opts.fit = 32;
          autoFit = true;
          palette.value = 'fit:32';
          return run();
        }
      }
    }
    paint();
  }

  function paint() {
    clearInterval(timer);
    resetBtn.el.hidden = !bytes && !title.value;
    // Preview: always a card frame of the same size.
    const card = result
      ? { ...result.card, title: title.value, frame: shade, author: app.profile.nick }
      : { ...BLANK, title: title.value, frame: shade, author: app.profile.nick };
    const f = frame({ card, rec: /** @type {any} */ ({ digest: '', received: Date.now() }), author: null });
    // The caption shows the title as typed, or a hint; the hint is not part
    // of any card (and may be longer than a title may be).
    const cap = f.querySelector('.cap b');
    if (cap && !title.value) cap.textContent = t('import.name.ph');
    const art = /** @type {HTMLElement} */ (f.querySelector('.art'));
    if (!bytes) {
      f.classList.add('blank');
      art.replaceChildren(h('div', { class: 'import-empty' }, icon('image'), h('b', {}, t('import.pick')), h('small', {}, t('import.pick.short'))));
    } else if (!result || showOriginal) {
      f.classList.toggle('blank', !result);
      art.replaceChildren(sourceUrl
        ? h('img', { class: `import-original${result ? '' : ' dim'}`, src: sourceUrl, alt: '' })
        : h('div', { class: 'import-empty' }, icon('file'), h('small', {}, fileName)));
    } else {
      animate(f, card);
    }
    const chip = bytes && result && sourceUrl ? h('span', { class: 'import-chip' }, t(showOriginal ? 'import.before' : 'import.after')) : null;
    preview.replaceChildren(f, chip ?? '');
    preview.setAttribute('aria-label', !bytes ? t('import.pick') : t('import.toggle'));

    status.classList.toggle('bad', !!error);
    status.textContent = error ? explain(error) : result ? report(result) : t('import.pick.hint');
    paletteField.classList.toggle('attention', error?.code === 'too_many_colors' || autoFit);

    shades.replaceChildren(...FRAME_SHADES.map((s, i) => h('button', {
      class: 'shade', role: 'radio', 'aria-checked': String(i === shade), style: { background: s.paper },
      onclick: () => {
        shade = i;
        paint();
      },
    })));
    save.disabled = !result;
    pickBtn.textContent = t(bytes ? 'import.other' : 'import.pick');
    extras.replaceChildren(...options());
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
    if (c.palette.builtin != null) bits.push(t('import.palette.builtin', paletteName(c.palette.builtin)));
    else if (opts.fit != null && r.changedPixels) bits.push(t('import.palette.fitted', r.colors, c.palette.inline.length - (c.opaque ? 0 : 1)));
    else bits.push(t('import.palette.own', r.colors));
    if (r.changedPixels && opts.fit == null) bits.push(t('import.changed', r.changedPixels));
    const size = app.core.shareSize(c);
    bits.push(size <= 600 ? t('import.code.single', size) : t('import.code.carousel', size));
    return (autoFit ? `${t('import.fitted-auto')} ` : '') + bits.join(' · ');
  }

  /** The tick boxes some files need; they live below the buttons. */
  function options() {
    const rows = [];
    if (result?.scale > 1 || opts.keepScale) rows.push(toggle('keepScale', t('import.opt.keep-scale')));
    if ((result?.sourceFrames ?? 0) > 1 || opts.firstFrameOnly || error?.code === 'unequal_delays' || error?.code === 'too_many_frames') {
      rows.push(toggle('firstFrameOnly', t('import.opt.first-frame')));
    }
    return rows;
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

  async function sign() {
    if (!result) return;
    const p = app.profile;
    const card = { ...result.card, title: title.value.trim(), frame: shade, author: p.nick, handle: p.handle, link: p.link };
    try {
      const r = await addOwn(app.identity.signCard(card));
      reset();
      go('collection', { open: r.digest });
      toast(t('import.saved'));
    } catch (e) {
      toast(explain(/** @type {any} */ (e)));
    }
  }

  paint();
  return {
    el,
    show: paint, // also restarts an animated preview
    hide: () => clearInterval(timer),
  };
}

/** A blank card, only to give the empty preview the exact frame size. */
const BLANK = {
  width: 1, height: 1, opaque: false, palette: { builtin: 0 }, frame: 0, frames: 1, delay_ms: 0,
  title: '', author: '', handle: null, link: '', pixels: new Uint8Array(1),
};

/** "Sweetie 16" with 16 colours reads "Sweetie · 16": the count once. @param {any} p */
function paletteLabel(p) {
  const n = p.colors.length;
  const name = p.name.endsWith(` ${n}`) ? p.name.slice(0, -String(n).length - 1) : p.name;
  return `${name} · ${n}`;
}

/** @type {any[]|null} */
let paletteCache = null;
const palettes = () => (paletteCache ??= app.core.palettes());

/** @param {number} id */
function paletteName(id) {
  const p = palettes().find((/** @type {any} */ x) => x.id === id);
  return p ? paletteLabel(p).replace(/ · \d+$/, '') : `#${id}`;
}

/** A translated reason, with the numbers the core attached.
 * @param {{code: string, args?: number[], message?: string}} e */
function explain(e) {
  const key = `import.err.${e?.code}`;
  const text = t(key, ...(e?.args ?? []));
  return text === key ? `${t('err.generic')}: ${e?.message ?? e}` : text;
}
