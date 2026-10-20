// @ts-check
// Receive: the camera reads codes by itself; a picture, a .pips file or a
// pasted link work too. Whatever arrives is verified by the core before it
// is stored, and nothing needs confirming.

import { h, icon } from '../dom.js';
import { t } from '../i18n.js';
import { app, receive, receivePayload } from '../state.js';
import { go, toast } from './shell.js';
import { scanner } from './scan.js';
import { showRecipientCode } from './edition.js';

const MAGIC = [0x50, 0x50, 0x1a, 0x0a];


/** Core error codes meaning "well-formed, but a signature does not check out". */
const FORGED = new Set(['bad_signature', 'bad_key', 'not_author', 'bad_edition', 'unsigned']);
export function receiveView() {
  const scan = scanner((text) => handle(text));
  const bar = h('i');
  const progress = h('div', { class: 'progress', hidden: true }, bar);
  const fileInput = /** @type {HTMLInputElement} */ (h('input', { type: 'file', accept: '.pips,application/octet-stream', hidden: true, onchange: () => fromFile() }));
  const linkInput = /** @type {HTMLInputElement} */ (h('input', { type: 'url', placeholder: t('receive.paste.placeholder'), autocomplete: 'off' }));
  const progressText = h('small', {});
  const el = h('section', { class: 'view' },
    h('div', { class: 'bar' }, h('div', { class: 'title' }, h('h1', {}, t('receive.title')))),
    h('div', { class: 'scroll' },
      scan.el,
      h('div', { class: 'progress-wrap' }, progress, progressText),
      h('div', { class: 'group row-btns' },
        h('button', { class: 'btn', onclick: () => fileInput.click() }, icon('file'), t('receive.file')),
        h('button', { class: 'btn', onclick: () => showRecipientCode() }, icon('key'), t('edition.my-code'))),
      h('div', { class: 'group' },
        h('div', { class: 'field' },
          linkInput,
          h('button', { class: 'icon-btn field-btn', 'aria-label': t('receive.paste'), title: t('receive.paste'), onclick: () => pasteLink() }, icon('link')))),
      fileInput));

  // Whatever lands in the field is read at once: pasted, or typed and Enter.
  const submit = () => {
    const v = linkInput.value.trim();
    linkInput.value = '';
    if (v) handle(v);
  };
  linkInput.addEventListener('paste', () => setTimeout(submit));
  linkInput.addEventListener('keydown', (e) => { if (e.key === 'Enter') submit(); });

  /** The chain button: straight from the clipboard where the browser allows
   * it; otherwise the field takes focus for a manual paste. */
  async function pasteLink() {
    try {
      const text = (await navigator.clipboard.readText()).trim();
      if (!text) return toast(t('receive.clipboard-empty'));
      handle(text);
    } catch {
      linkInput.focus();
      toast(t('receive.paste-here'));
    }
  }

  /** @type {any} */
  let joiner = null;
  let lastText = '';
  let lastAt = 0;
  const stopCamera = () => scan.stop();

  async function fromFile() {
    const f = fileInput.files?.[0];
    fileInput.value = '';
    if (!f || f.size > 70 * 1024) { toast(t('receive.not-pips')); return; }
    const bytes = new Uint8Array(await f.arrayBuffer());
    if (!MAGIC.every((b, i) => bytes[i] === b)) { toast(t('receive.not-pips')); return; }
    try {
      done(await receivePayload(bytes.subarray(MAGIC.length)));
    } catch {
      toast(t('receive.not-pips'));
    }
  }

  /** @param {string} text */
  async function handle(text) {
    // A camera sees the same code many times a second.
    const now = Date.now();
    if (text === lastText && now - lastAt < 2500) return;
    lastText = text;
    lastAt = now;

    let looksPips = false;
    try {
      const s = app.core.classify(text);
      looksPips = s.kind === 'envelope' || s.kind === 'frame';
      if (s.kind === 'envelope') return await accept(s.bytes);
      if (s.kind === 'frame') {
        joiner ??= new app.core.Joiner();
        let p;
        try {
          p = joiner.push(s.bytes);
        } catch {
          joiner = new app.core.Joiner(); // a different carousel: start over with it
          p = joiner.push(s.bytes);
        }
        progress.hidden = false;
        if (p.kind === 'partial') {
          bar.style.width = `${(100 * p.have) / p.need}%`;
          progressText.textContent = t('receive.parts', p.have, p.need);
          return;
        }
        joiner = null;
        progress.hidden = true;
        progressText.textContent = '';
        return await accept(p.bytes);
      }
      if (s.kind === 'recipient') return toast(t('receive.recipient'));
      if (s.kind === 'paper_key') return toast(t('receive.paper'));
      toast(t('receive.not-pips'));
    } catch (e) {
      // A damaged or forged code lands here: openEnvelope checks every signature.
      const code = /** @type {any} */ (e)?.code;
      toast(t(FORGED.has(code) ? 'receive.forged' : looksPips ? 'receive.damaged' : 'receive.not-pips'));
    }
  }

  /** @param {Uint8Array} envelope */
  async function accept(envelope) {
    const opened = app.core.openEnvelope(envelope); // throws unless every signature checks out
    done(await receive(opened));
  }

  /** @param {{status: string, digest: string}} r */
  function done(r) {
    stopCamera();
    // The card opens large in the collection, where it now is.
    go('collection', { open: r.digest });
    toast(t(r.status === 'added' ? 'receive.got' : 'receive.dup'));
  }

  return { el, hide: stopCamera };
}
