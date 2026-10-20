// The app's journeys, end to end, with the real WebAssembly core.
// Run after scripts/build-wasm.sh: `npm ci && npm test` in this folder.

import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { setup, photo, fromPicture, choose, tab, toast, heart, gradient, textMatching } from './helpers.mjs';

/** @type {Awaited<ReturnType<typeof setup>>} */
let env;
before(async () => { env = await setup(); });
after(async () => {
  await env.close();
  assert.deepEqual(env.errors, [], 'no errors in any page');
});

const words = (p) => p.evaluate(async () => (await import('/src/state.js')).app.identity.words());
const cards = (p) => p.evaluate(async () => (await import('/src/state.js')).app.cards.size);

/** Makes an own card from the heart picture; it opens large in the collection. */
async function ownCard(p, title) {
  await tab(p, 'import');
  await choose(p, () => p.click('.import-preview'), await heart(env.dir));
  await p.locator('.import-body input:not([type])').fill(title);
  await p.click('text=Подписать и добавить');
  await p.locator('.turn:not([hidden])').waitFor();
}
/** The card in front: its share sheet, its back. */
const act = (p, name) => p.click(`.action-bar.up [data-act="${name}"]`);
const backText = async (p) => {
  await act(p, 'turn');
  await p.waitForTimeout(700);
  return p.textContent('.turn .back-face');
};

test('a key restored from the words is the same key', async () => {
  const a = await env.phone('pixelfox');
  const b = await env.phone('pixelfox', (await words(a)).toUpperCase());
  const short = (p) => p.evaluate(async () => (await import('/src/state.js')).app.identity.publicKey().short);
  assert.equal(await short(b), await short(a));
  await a.context().close();
  await b.context().close();
});

test('import keeps its layout and reports what it did', async () => {
  const p = await env.phone('pixelfox');
  await tab(p, 'import');
  // Measured with the fonts in place and the sheet's entry animation over.
  await p.evaluate(() => Promise.all([document.fonts.ready, ...document.getAnimations().map((a) => a.finished)]));
  const buttonsAt = async () => (await p.locator('.import-actions').boundingBox())?.y;
  const empty = await buttonsAt();
  assert.ok(await p.locator('.import-actions .btn.key').isDisabled(), 'nothing to sign yet');

  // 512 colours: an own palette is fitted by itself, and said so.
  await choose(p, () => p.click('.import-preview'), await gradient(env.dir));
  assert.match(await textMatching(p, '.import-status', /подобрана своя палитра/), /больше 256 цветов файла → свои 32/);
  assert.equal(await p.inputValue('.import-body select'), 'fit:32');
  assert.ok(await p.locator('.import-actions .btn.key').isEnabled());
  assert.equal(await buttonsAt(), empty, 'the status does not move the buttons');

  await p.selectOption('.import-body select', { label: 'PICO-8 · 16' });
  assert.match(await textMatching(p, '.import-status', /палитра PICO-8 · 512 пикселей сменили цвет/), /палитра PICO-8 · 512 пикселей сменили цвет/);
  assert.equal(await buttonsAt(), empty);

  await choose(p, () => p.click('text=Другой файл'), await heart(env.dir));
  assert.match(await textMatching(p, '.import-status', /60×48 → 10×8 \(снято увеличение ×6\)/), /60×48 → 10×8 \(снято увеличение ×6\)/);
  assert.equal(await buttonsAt(), empty);
  await p.click('text=Подписать и добавить');
  await p.locator('.turn:not([hidden])').waitFor();
  assert.equal(await cards(p), 1);
  await p.context().close();
});

test('a card goes from one phone to another and is checked on arrival', async () => {
  const a = await env.phone('pixelfox');
  const b = await env.phone('mossfern');
  await ownCard(a, 'Сердце');
  await act(a, 'share');
  const code = await photo(a, '.top-sheet .qr-box', env.dir, 'give');

  await tab(b, 'receive');
  await fromPicture(b, 'section.view:visible', code);
  // It opens large in the collection, where it now is.
  await b.locator('.turn:not([hidden])').waitFor();
  assert.equal(await toast(b), 'Карточка в коллекции');
  const back = await backText(b);
  assert.match(back, /верна/);
  assert.match(back, /@pixelfox → вам/);
  assert.equal(await cards(b), 1);

  // The same code again: recognised, not duplicated.
  await tab(b, 'receive');
  await fromPicture(b, 'section.view:visible', code);
  await b.waitForTimeout(500);
  assert.equal(await cards(b), 1);
  await a.context().close();
  await b.context().close();
});

test('damaged and forged codes are refused with their reason', async () => {
  const a = await env.phone('pixelfox');
  await ownCard(a, 'Сердце');
  const text = await a.evaluate(async () => {
    const s = await import('/src/state.js');
    const [e] = [...s.app.cards.values()];
    return s.app.core.qrPlan(s.app.identity.sealAsAuthor(e.rec.payload, undefined)).text;
  });
  const b = await env.phone('mossfern');
  await tab(b, 'receive');
  const paste = async (t) => {
    await b.evaluate(() => { document.querySelector('.toast span').textContent = ''; });
    await b.fill('section.view:visible .field input', t);
    await b.press('section.view:visible .field input', 'Enter');
    await b.waitForTimeout(300);
    return toast(b);
  };
  const flip = (at) => text.slice(0, at) + (text[at] === 'A' ? 'B' : 'A') + text.slice(at + 1);
  assert.match(await paste(flip(60)), /Код Pips, но повреждён/);
  assert.match(await paste(flip(text.length - 3)), /Подпись не сходится/);
  assert.match(await paste('hello'), /Это не код Pips/);
  assert.equal(await cards(b), 0);
  await a.context().close();
  await b.context().close();
});

test('numbered copies: issued once each, until the run is complete', async () => {
  const a = await env.phone('pixelfox');
  const b = await env.phone('mossfern');
  await ownCard(a, 'Сердце');
  await act(a, 'share');
  await a.click('.top-sheet button:text("Экземпляр №")');
  await a.fill('.edition-panel input', '2');

  await tab(b, 'receive');
  await b.click('text=Мой код получателя');
  const rc = await photo(b, '.overlay .qr-box', env.dir, 'recipient');
  await b.click('.overlay .icon-btn');

  for (const n of [1, 2]) {
    await a.click(`text=Выдать №${n}`);
    await fromPicture(a, '.overlay', rc);
    assert.match(await a.locator('.dialog').textContent(), new RegExp(`Выдать №${n} из 2 для @mossfern`));
    await a.click('.dialog button:text-is("Выдать")');
    const env1 = await photo(a, '.overlay .qr-box', env.dir, `copy${n}`);
    await a.click('text=Готово');
    if (n === 1) {
      await fromPicture(b, 'section.view:visible', env1);
      await b.locator('.turn:not([hidden])').waitFor();
      assert.match(await backText(b), /экземпляр 1 из 2 — ваш/);
      await tab(b, 'receive');
    }
  }
  assert.equal(await a.textContent('.edition-panel .hint'), 'Выдано 2 из 2');
  assert.ok(await a.locator('.edition-panel .btn').isDisabled());
  await a.context().close();
  await b.context().close();
});

test('the collection: tags, search, archive and the bin, all without gestures', async () => {
  const p = await env.phone('pixelfox');
  await tab(p, 'me');
  await p.click('text=Добавить образцы карточек');
  await p.locator('.collection-view:visible').waitFor();
  await p.waitForTimeout(800);
  const sub = () => p.textContent('.collection-bar .sub span');
  const inPlace = (st) => p.evaluate(async (x) => [...(await import('/src/state.js')).app.cards.values()].filter((e) => e.rec.state === x).length, st);
  const shown = () => p.$$eval('.on-stage', (els) => els.filter((e) => e.isConnected && Number(e.style.opacity) > 0.5).length);
  /** Clicks the first card fully on screen below the bar. */
  const clickCard = async () => {
    const at = await p.$$eval('.on-stage', (els) => els.map((e) => e.getBoundingClientRect()).filter((r) => r.top > 70 && r.bottom < 700 && r.width > 20)[0]);
    await p.mouse.click(at.x + at.width / 2, at.y + at.height / 2);
    await p.waitForTimeout(900);
  };
  assert.equal(await shown(), 22);
  assert.equal(await sub(), 'по неделям');

  // A card opens: feed, then the card itself; a tag goes on its back.
  await clickCard();
  assert.equal(await sub(), 'лента');
  await p.keyboard.press('Escape');
  await p.waitForTimeout(900);
  assert.equal(await sub(), 'по неделям');
  await clickCard();
  await act(p, 'turn');
  await p.waitForTimeout(800);
  assert.equal(await sub(), 'карточка');
  assert.ok(await p.$eval('.turn textarea', (t) => t.readOnly), 'nothing to type before the pencil');
  await p.click('.turn .pen');
  await p.click('.turn .tag.add');
  await p.keyboard.type('Ночные Совы');
  await p.keyboard.press('Enter');
  await p.click('.turn .pen');
  await p.waitForTimeout(100);
  assert.match(await p.textContent('.turn .tags'), /#ночные-совы/);

  // The tag on the back is a filter: every card with it, in the months.
  await p.click('.turn .tag');
  await p.waitForTimeout(1200);
  assert.equal(await sub(), 'по месяцам');
  assert.equal(await p.inputValue('.search-field input'), '#ночные-совы');
  assert.equal(await shown(), 1);
  await p.click('.search-field .clear');
  await p.waitForTimeout(900);
  assert.equal(await shown(), 22);
  await p.click('.seg button[data-value="mine"]');
  await p.waitForTimeout(900);
  assert.equal(await shown(), 5, 'the five own sample cards');
  await p.click('.seg button[data-value="all"]');
  await p.waitForTimeout(900);

  // To the archive from the feed's bar, back with undo, then for good.
  await clickCard();
  await act(p, 'down');
  await p.waitForTimeout(2200);
  assert.equal(await toast(p), 'Карточка в архиве');
  assert.equal(await inPlace('collection'), 21);
  await p.click('.toast button');
  await p.waitForTimeout(900);
  assert.equal(await inPlace('collection'), 22, 'undone');
  await act(p, 'down');
  await p.waitForTimeout(2200);
  assert.equal(await p.textContent('.collection-bar .ib[aria-label="Архив"] .ib-badge'), '1');

  // The archive is a place like the collection: the card opens there too.
  await p.click('.collection-bar .ib[aria-label="Архив"]');
  await p.waitForTimeout(900);
  assert.equal(await p.textContent('.collection-bar h1'), 'Архив');
  assert.equal(await shown(), 1);
  await clickCard();
  await act(p, 'trash');
  await p.waitForTimeout(900);
  assert.equal(await toast(p), 'Карточка в корзине');
  assert.match(await p.textContent('.place-empty'), /В архиве пусто/);
  await p.click('.collection-bar .ib[aria-label="Назад"]');
  await p.waitForTimeout(900);
  await p.click('.collection-bar .ib[aria-label="Корзина"]');
  await p.waitForTimeout(900);
  assert.match(await sub(), /удаляются через 30 дней/);
  await p.click('.collection-bar .pill');
  await clickCard();
  await p.click('.action-bar.up [data-act="purge"]');
  await p.click('.dialog button:text-is("Удалить")');
  await p.waitForTimeout(900);
  assert.equal(await cards(p), 21);
  await p.context().close();
});

test('a backup restores cards, notes and the copy log, only for its key', async () => {
  const a = await env.phone('pixelfox');
  await tab(a, 'me');
  await a.click('text=Добавить образцы карточек');
  await a.waitForTimeout(1500);
  await a.evaluate(async () => {
    const s = await import('/src/state.js');
    const [d1, d2] = [...s.app.cards.keys()];
    await s.setNote(d1, 'заметка с фестиваля');
    await s.setTags(d1, ['фестиваль', 'совы']);
    await s.move(d2, 'archive');
  });
  await tab(a, 'collection');
  await tab(a, 'me');
  const status = () => a.locator('.group:has(h2:text("Резервная копия")) .hint').first().textContent();
  assert.match(await status(), /Копии ещё нет/);
  const [download] = await Promise.all([a.waitForEvent('download'), a.click('text=Сохранить копию')]);
  const file = join(env.dir, download.suggestedFilename());
  await download.saveAs(file);
  assert.match(await status(), /Последняя: .*, 22 карточки/, 'the status updates at once');

  const restore = async (p, f) => {
    await choose(p, () => p.click('.group:has(h2:text("Резервная копия")) >> text=Восстановить'), f);
    await p.waitForTimeout(800);
    return toast(p);
  };
  const b = await env.phone('новый телефон', await words(a));
  await tab(b, 'me');
  assert.equal(await restore(b, file), 'Восстановлено: 22');
  const facts = await b.evaluate(async () => {
    const s = await import('/src/state.js');
    const r = [...s.app.cards.values()].map((e) => e.rec);
    return { notes: r.filter((x) => x.note).length, tags: r.flatMap((x) => x.tags ?? []).join(' '), archived: r.filter((x) => x.state === 'archive').length, nick: s.app.profile.nick };
  });
  assert.deepEqual(facts, { notes: 1, tags: 'фестиваль совы', archived: 1, nick: 'pixelfox' });
  assert.match(await b.locator('.group:has(h2:text("Резервная копия")) .hint').first().textContent(), /Последняя/);

  const c = await env.phone('чужой');
  await tab(c, 'me');
  assert.match(await restore(c, file), /Это копия другого ключа/);
  const bytes = await readFile(file);
  bytes[bytes.length - 5] ^= 1;
  await writeFile(join(env.dir, 'bad.pipsbak'), bytes);
  assert.match(await restore(b, join(env.dir, 'bad.pipsbak')), /Копия повреждена/);
  for (const p of [a, b, c]) await p.context().close();
});
