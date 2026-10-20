// @ts-check
// Local storage: IndexedDB, nothing leaves the device.
//
// `meta`  — key/value: identity seed, profile, settings.
// `cards` — one record per card, keyed by digest:
//   { digest, payload: Uint8Array, cert: Uint8Array|null, edition: {…}|null, mine: boolean,
//     state: 'collection'|'archive'|'trash', received: ms, trashedAt: ms|null,
//     from: {nick, key}|null, note: string, tags?: string[] }
// Payload bytes are stored exactly as received, so a card passed on later
// carries everything it arrived with.

/** @typedef {{digest: string, payload: Uint8Array, cert: Uint8Array|null,
 *   edition: {edition: number, of: number, holder: string, day: number}|null, mine: boolean,
 *   state: 'collection'|'archive'|'trash', received: number, trashedAt: number|null,
 *   from: {nick: string, key: string}|null, note: string, tags?: string[],
 *   editions?: import('./state.js').EditionLog}} CardRecord */

/** @type {IDBDatabase} */
let db;
/** When IndexedDB is unavailable (some private modes, blocked site data),
 * everything lives in memory for this session; `memoryOnly` says so. */
export let memoryOnly = false;
const mem = { meta: new Map(), cards: new Map() };

/** @param {string} name */
export async function open(name) {
  try {
    await openDb(name);
  } catch (e) {
    console.warn('pips: IndexedDB unavailable, keeping data in memory', e);
    memoryOnly = true;
  }
}

/** @param {string} name */
function openDb(name) {
  return new Promise((resolve, reject) => {
    if (!globalThis.indexedDB) throw new Error('no IndexedDB');
    const req = indexedDB.open(name, 1);
    req.onupgradeneeded = () => {
      const d = req.result;
      d.createObjectStore('meta');
      d.createObjectStore('cards', { keyPath: 'digest' });
    };
    req.onsuccess = () => {
      db = req.result;
      resolve(undefined);
    };
    req.onerror = () => reject(req.error);
  });
}

/** @param {IDBRequest} req */
function done(req) {
  return new Promise((resolve, reject) => {
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

/** @param {string} store @param {IDBTransactionMode} mode */
function tx(store, mode) {
  return db.transaction(store, mode).objectStore(store);
}

/** @param {string} key */
export const getMeta = async (key) => (memoryOnly ? mem.meta.get(key) : done(tx('meta', 'readonly').get(key)));
/** @param {string} key @param {any} value */
export const setMeta = async (key, value) => (memoryOnly ? void mem.meta.set(key, value) : done(tx('meta', 'readwrite').put(value, key)));

/** @returns {Promise<CardRecord[]>} */
export const allCards = async () => (memoryOnly ? [...mem.cards.values()] : done(tx('cards', 'readonly').getAll()));
/** @param {string} digest @returns {Promise<CardRecord|undefined>} */
export const getCard = async (digest) => (memoryOnly ? mem.cards.get(digest) : done(tx('cards', 'readonly').get(digest)));
/** @param {CardRecord} rec */
export const putCard = async (rec) => (memoryOnly ? void mem.cards.set(rec.digest, rec) : done(tx('cards', 'readwrite').put(rec)));
/** @param {string} digest */
export const deleteCard = async (digest) => (memoryOnly ? void mem.cards.delete(digest) : done(tx('cards', 'readwrite').delete(digest)));

/** Asks the browser not to evict our data under storage pressure. */
export async function persist() {
  if (!navigator.storage?.persist) return false;
  return (await navigator.storage.persisted()) || navigator.storage.persist();
}

export async function isPersisted() {
  return navigator.storage?.persisted ? navigator.storage.persisted() : false;
}
