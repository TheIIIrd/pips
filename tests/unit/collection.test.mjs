// Unit tests of the interface's plain parts: no browser, only node.
//   node --test tests/unit/*.test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import { normalTag, matches, select, toggleTag, tagCounts } from '../../web/src/collection/query.js';
import { grid, feedRect, runs, groupKey, weekStart, columns } from '../../web/src/collection/layout.js';
import { Spring, flight, rubber } from '../../web/src/kit/motion.js';

const DAY = 86_400_000;
const card = (o) => ({ state: 'collection', mine: false, received: 0, title: '', author: '', tags: [], ...o });

test('a tag is one lower-case word within 32 bytes, as pips-store checks it', () => {
  assert.equal(normalTag('  #Ночные Совы '), 'ночные-совы');
  assert.equal(normalTag('##a#b'), 'ab');
  assert.equal(normalTag('   '), '');
  assert.equal(new TextEncoder().encode(normalTag('я'.repeat(40))).length, 32, 'cut at a whole letter');
});

test('search words match title or artist; #words match tags exactly', () => {
  const c = card({ title: 'Пряник', author: 'dapple', tags: ['котики'] });
  assert.ok(matches(c, 'прян'));
  assert.ok(matches(c, 'DAPP #котики'));
  assert.ok(!matches(c, '#кот'), 'a tag is not a prefix');
  assert.ok(!matches(c, 'пряник #совы'));
});

test('a place, whose cards and the order', () => {
  const all = [
    card({ title: 'a', author: 'b', received: 3, mine: true }),
    card({ title: 'b', author: 'a', received: 2 }),
    card({ title: 'c', author: 'a', received: 4, state: 'archive' }),
    card({ title: 'd', author: 'b', received: 1 }),
  ];
  const f = { place: 'collection', owner: 'all', query: '', by: 'date' };
  assert.deepEqual(select(all, f).map((c) => c.title), ['a', 'b', 'd'], 'newest first, archive apart');
  assert.deepEqual(select(all, { ...f, by: 'author' }).map((c) => c.title), ['b', 'a', 'd'], 'artist by artist');
  assert.deepEqual(select(all, { ...f, owner: 'got' }).map((c) => c.title), ['b', 'd']);
  assert.deepEqual(select(all, { ...f, place: 'archive' }).map((c) => c.title), ['c']);
});

test('tags toggle in a query and are counted', () => {
  assert.equal(toggleTag('лес', 'совы'), 'лес #совы');
  assert.equal(toggleTag('лес #совы', 'совы'), 'лес');
  assert.deepEqual(tagCounts([card({ tags: ['б', 'а'] }), card({ tags: ['а'] })]), [['а', 2], ['б', 1]]);
});

test('the grid: a label cell starts each group, tiles never overlap', () => {
  const f = { W: 390, H: 844, top: 76, bottom: 84, lead: 0 };
  const now = new Date(2026, 9, 20).getTime();
  const list = [0, 1, 9, 10, 40].map((d) => ({ received: now - d * DAY, author: 'x' }));
  const groups = runs(list, (c) => groupKey(1, 'date', c));
  assert.deepEqual(groups.map((g) => g.cards.length), [2, 2, 1], 'three weeks');
  const g = grid(1, groups, f);
  assert.equal(g.labels.length, 3);
  const cells = [...g.labels, ...g.rects.values()].map((r) => `${Math.round(r.x)},${Math.round(r.y)}`);
  assert.equal(new Set(cells).size, cells.length);
  assert.equal(columns(1, 390), 3);
  assert.equal(columns(0, 390), 5);
  assert.equal(new Date(weekStart(now)).getDay(), 1, 'weeks start on Monday');
});

test('the feed: the centre card is the largest and on top', () => {
  const f = { W: 390, H: 844, top: 76, bottom: 84, lead: 0 };
  const mid = feedRect(3, 3, false, f), side = feedRect(4, 3, false, f);
  assert.ok(mid.w > side.w && mid.z > side.z);
  assert.ok(Math.abs(mid.x + mid.w / 2 - 195) < 1, 'centred');
});

test('springs settle; a flight ends where it was sent', () => {
  const s = new Spring(0);
  s.to = 1;
  let n = 0;
  while (s.step(1 / 60) && n < 600) n++;
  assert.equal(s.v, 1);
  assert.ok(n < 120, 'within two seconds');
  const f = flight({ x: 0, y: 0, w: 100 }, { x: 50, y: 300, w: 56 }, { start: 0, dur: 600 });
  assert.deepEqual(f.at(600), { x: 50, y: 300, w: 56, t: 1 });
  assert.equal(rubber(110, 0, 100), 103.5);
});
