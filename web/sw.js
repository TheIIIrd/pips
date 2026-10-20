// Offline cache for the app shell. A new VERSION replaces every cached
// file at once, so a release never mixes old and new modules.
// Both lines are rewritten by scripts/build-site.sh: the version from the
// commit, the list from the files actually published. Kept valid here so
// the file still works as is.
const VERSION = 'pips-dev';
const FILES = ['./', 'index.html'];

self.addEventListener('install', (e) => {
  e.waitUntil(caches.open(VERSION).then((c) => c.addAll(FILES)).then(() => self.skipWaiting()));
});

self.addEventListener('activate', (e) => {
  e.waitUntil(caches.keys()
    .then((keys) => Promise.all(keys.filter((k) => k !== VERSION).map((k) => caches.delete(k))))
    .then(() => self.clients.claim()));
});

// Cache first: the app works the same with or without a network. Only
// same-origin GETs are ever handled; there is nothing else to fetch.
self.addEventListener('fetch', (e) => {
  const url = new URL(e.request.url);
  if (e.request.method !== 'GET' || url.origin !== location.origin) return;
  e.respondWith(caches.match(e.request, { ignoreSearch: true }).then((hit) => hit ?? fetch(e.request)));
});
