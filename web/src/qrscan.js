// @ts-check
// Reading QR codes: the browser's own BarcodeDetector where it has one
// (Chrome on Android and macOS), otherwise the core's reader — a separate
// WebAssembly module (pkg/pips_scan), fetched only on first use.

/** @typedef {{native: boolean, read: (src: HTMLVideoElement|ImageBitmap) => Promise<string[]>}} Reader */

/** @type {Promise<Reader>|null} */
let pending = null;

/** The reader for this browser, created once. @returns {Promise<Reader>} */
export function qrReader() {
  pending ??= make().catch((e) => {
    pending = null; // a failed download may succeed next time
    throw e;
  });
  return pending;
}

async function make() {
  const D = /** @type {any} */ (globalThis).BarcodeDetector;
  const formats = D ? await D.getSupportedFormats?.().catch(() => []) : [];
  if (D && formats?.includes('qr_code')) {
    const d = new D({ formats: ['qr_code'] });
    return { native: true, read: async (/** @type {any} */ src) => (await d.detect(src)).map((/** @type {any} */ c) => c.rawValue) };
  }
  const m = await import('../pkg/pips_scan.js');
  await m.default();
  const cv = document.createElement('canvas');
  const g = /** @type {CanvasRenderingContext2D} */ (cv.getContext('2d', { willReadFrequently: true }));
  return {
    native: false,
    read: async (/** @type {any} */ src) => {
      const w0 = src.videoWidth || src.width, h0 = src.videoHeight || src.height;
      if (!w0 || !h0) return [];
      // Camera frames are read smaller (speed); pictures larger, since the
      // code may fill only part of a photo.
      const k = Math.min(1, (src instanceof HTMLVideoElement ? 960 : 2048) / Math.max(w0, h0));
      const w = Math.round(w0 * k), h = Math.round(h0 * k);
      cv.width = w;
      cv.height = h;
      g.drawImage(src, 0, 0, w, h);
      const px = g.getImageData(0, 0, w, h).data;
      return m.scanQr(new Uint8Array(px.buffer, px.byteOffset, px.byteLength), w, h);
    },
  };
}
