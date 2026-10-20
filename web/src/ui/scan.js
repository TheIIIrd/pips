// @ts-check
// A QR scanner: the camera, plus "from a picture" for when there is none.
// Hands every text it reads to `onText`; deciding what it means is the
// caller's job. Repeats of the same text are left for the caller too.

import { h, icon } from '../dom.js';
import { t } from '../i18n.js';
import { qrReader } from '../qrscan.js';
import { toast } from './shell.js';

/** @param {(text: string) => void} onText @param {string} [hint] */
export function scanner(onText, hint = t('receive.point')) {
  const video = /** @type {HTMLVideoElement} */ (h('video', { playsInline: true, muted: true, hidden: true }));
  const message = h('p', {}, hint);
  const startBtn = h('button', { class: 'btn key', onclick: () => start() }, t('receive.start'));
  const imageInput = /** @type {HTMLInputElement} */ (h('input', { type: 'file', accept: 'image/*', hidden: true, onchange: () => fromImage() }));
  const el = h('div', {},
    h('div', { class: 'scanner' }, video, h('div', { class: 'corners' }), message),
    h('div', { class: 'group row-btns' }, startBtn,
      h('button', { class: 'btn', onclick: () => imageInput.click() }, icon('image'), t('receive.image'))),
    imageInput);

  /** @type {MediaStream|null} */
  let stream = null;
  let timer = 0;
  let busy = false;

  /** @returns {Promise<import('../qrscan.js').Reader|null>} */
  async function reader() {
    try {
      return await qrReader();
    } catch {
      message.textContent = t('scan.failed');
      return null;
    }
  }

  async function start() {
    message.textContent = t('scan.loading');
    const r = await reader();
    if (!r) return;
    try {
      stream = await navigator.mediaDevices.getUserMedia({ video: { facingMode: 'environment' }, audio: false });
    } catch {
      message.textContent = t('receive.no-camera');
      return;
    }
    video.srcObject = stream;
    video.hidden = false;
    message.textContent = '';
    startBtn.hidden = true;
    await video.play();
    // The browser's detector is fast; ours is given more time per frame.
    timer = window.setInterval(async () => {
      if (busy || video.readyState < 2) return;
      busy = true;
      try {
        for (const text of await r.read(video)) onText(text);
      } catch { /* a frame that could not be read */ }
      busy = false;
    }, r.native ? 150 : 250);
  }

  function stop() {
    clearInterval(timer);
    stream?.getTracks().forEach((tr) => tr.stop());
    stream = null;
    video.hidden = true;
    startBtn.hidden = false;
    message.textContent = hint;
  }

  async function fromImage() {
    const f = imageInput.files?.[0];
    imageInput.value = '';
    if (!f) return;
    const r = await reader();
    if (!r) return toast(t('scan.failed'));
    const texts = await r.read(await createImageBitmap(f)).catch(() => []);
    if (!texts.length) return toast(t('scan.none'));
    for (const text of texts) onText(text);
  }

  return { el, stop };
}
