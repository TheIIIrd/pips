// Shared helpers: a static server for web/, phones (browser contexts) and
// the QR round trip — a code shown on one phone is photographed (an element
// screenshot) and opened "from picture" on another, exactly as a person
// without a camera would do. The app's own QR reader does the reading.

import { createServer } from 'node:http';
import { readFile, mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, normalize, extname } from 'node:path';
import { deflateSync, crc32 } from 'node:zlib';
import { chromium } from 'playwright';

const WEB = new URL('../../web/', import.meta.url).pathname;
const TYPES = {
  '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.wasm': 'application/wasm',
  '.json': 'application/json', '.webmanifest': 'application/manifest+json', '.svg': 'image/svg+xml',
  '.png': 'image/png', '.woff2': 'font/woff2', '.txt': 'text/plain',
};

/** Serves web/ on a free port of 127.0.0.1 (a dev host: samples allowed). */
export async function serve() {
  const server = createServer(async (req, res) => {
    const path = normalize(decodeURIComponent(new URL(req.url ?? '/', 'http://x').pathname));
    const file = join(WEB, path.endsWith('/') ? `${path}index.html` : path);
    if (!file.startsWith(WEB)) return res.writeHead(403).end();
    try {
      const body = await readFile(file);
      res.writeHead(200, { 'content-type': TYPES[extname(file)] ?? 'application/octet-stream' }).end(body);
    } catch {
      res.writeHead(404).end();
    }
  });
  await new Promise((ok) => server.listen(0, '127.0.0.1', () => ok(undefined)));
  const { port } = /** @type {import('node:net').AddressInfo} */ (server.address());
  return { url: `http://127.0.0.1:${port}/`, close: () => server.close() };
}

/** One browser for a test file, plus a scratch folder for pictures. */
export async function setup() {
  const site = await serve();
  const browser = await chromium.launch();
  const dir = await mkdtemp(join(tmpdir(), 'pips-e2e-'));
  const errors = [];
  /** A fresh phone; with `words`, restored instead of created. */
  async function phone(nick, words) {
    const ctx = await browser.newContext({ viewport: { width: 390, height: 844 }, locale: 'ru-RU', acceptDownloads: true });
    const p = await ctx.newPage();
    p.on('pageerror', (e) => errors.push(`${nick}: ${e.message}`));
    p.on('console', (m) => { if (m.type() === 'error') errors.push(`${nick}: ${m.text()}`); });
    await p.goto(`${site.url}?dev`);
    if (words) {
      await p.click('text=У меня уже есть ключ');
      await p.fill('textarea.words', words);
      await p.click('text=Восстановить');
    } else {
      await p.click('text=Создать ключ');
      await p.check('.check input');
      await p.click('text=Дальше');
    }
    await p.fill('.field input', nick);
    await p.click('text=Готово');
    await p.locator('nav.tabs').waitFor();
    return p;
  }
  return {
    phone, dir, errors,
    async close() {
      await browser.close();
      site.close();
    },
  };
}

/** Photographs a QR code on screen into a file. */
export async function photo(page, selector, dir, name) {
  const file = join(dir, `${name}.png`);
  await page.locator(selector).first().screenshot({ path: file });
  return file;
}

/** Opens a picture through a scanner's "from picture" button in `scope`. */
export async function fromPicture(page, scope, file) {
  const [chooser] = await Promise.all([page.waitForEvent('filechooser'), page.click(`${scope} >> text=Из картинки`)]);
  await chooser.setFiles(file);
}

/** Picks a file through whatever `click` opens. */
export async function choose(page, click, file) {
  const [chooser] = await Promise.all([page.waitForEvent('filechooser'), click()]);
  await chooser.setFiles(file);
}

/** The text of `selector` once it matches `re` (waits up to 5 s). */
export async function textMatching(page, selector, re) {
  const at = Date.now();
  for (;;) {
    const text = (await page.locator(selector).first().textContent()) ?? '';
    if (re.test(text) || Date.now() - at > 5000) return text;
    await page.waitForTimeout(50);
  }
}

/** Taps a tab. The feed, a single card and the archive hide the tab bar,
 * as a person would, Escape steps back until it shows. */
export async function tab(page, name) {
  for (let i = 0; i < 4 && (await page.locator('nav.tabs.away').count()); i++) {
    await page.keyboard.press('Escape');
    await page.waitForTimeout(700);
  }
  await page.click(`nav.tabs button[data-tab="${name}"]`);
}
export const toast = (page) => page.locator('.toast span').textContent();
/** Writes a PNG of RGBA pixels drawn by `f(x, y)`. */
export async function png(dir, name, w, h, f) {
  const rows = [];
  for (let y = 0; y < h; y++) {
    const row = Buffer.alloc(1 + 4 * w);
    for (let x = 0; x < w; x++) Buffer.from(f(x, y)).copy(row, 1 + 4 * x);
    rows.push(row);
  }
  const chunk = (type, data) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const body = Buffer.concat([Buffer.from(type), data]);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(body));
    return Buffer.concat([len, body, crc]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr.set([8, 6, 0, 0, 0], 8);
  const file = join(dir, name);
  await writeFile(file, Buffer.concat([
    Buffer.from('\x89PNG\r\n\x1a\n', 'latin1'), chunk('IHDR', ihdr), chunk('IDAT', deflateSync(Buffer.concat(rows))), chunk('IEND', Buffer.alloc(0)),
  ]));
  return file;
}

/** A heart in PICO-8 colours, exported at ×6. */
export const heart = (dir) => png(dir, 'heart-x6.png', 60, 48, (x, y) => {
  const m = ['..##..##..', '.########.', '##########', '##########', '.########.', '..######..', '...####...', '....##....'];
  const [X, Y] = [Math.floor(x / 6), Math.floor(y / 6)];
  return m[Y][X] === '#' ? ((X + Y) % 5 ? [255, 0, 77, 255] : [255, 119, 168, 255]) : [0, 0, 0, 0];
});

/** 512 colours: needs a palette. */
export const gradient = (dir) => png(dir, 'gradient.png', 64, 8, (x, y) => [x * 4, y * 30, 128, 255]);
