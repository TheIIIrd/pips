// @ts-check
// Start-up: load the core, open storage, then either the first-run flow or
// the app itself.

import { h } from './dom.js';
import { loadCore, isDevHost } from './core.js';
import { t, setLanguage, defaultLanguage } from './i18n.js';
import { load, hasIdentity } from './state.js';
import { getMeta } from './store.js';
import { mountShell, go } from './ui/shell.js';
import { collectionView } from './ui/collection.js';
import { giveView } from './ui/give.js';
import { receiveView } from './ui/receive.js';
import { meView } from './ui/me.js';
import { onboarding } from './ui/onboarding.js';

const root = /** @type {HTMLElement} */ (document.getElementById('app'));

async function main() {
  setLanguage(defaultLanguage());
  let loaded;
  try {
    loaded = await loadCore();
  } catch (e) {
    root.append(h('div', { class: 'empty' }, h('b', {}, t('core.failed')), String(/** @type {any} */ (e)?.message ?? e)));
    return;
  }
  await load(loaded.core, loaded.stub);
  const lang = await getMeta('language');
  if (lang) setLanguage(lang);
  if (loaded.stub) document.documentElement.classList.add('dev');
  if (loaded.stub) document.body.append(h('div', { class: 'dev-banner' }, 'DEV STUB — NOT SECURE'));

  const start = () => {
    mountShell(root, { collection: collectionView(), give: giveView(), receive: receiveView(), me: meView() });
    go('collection');
  };
  if (hasIdentity()) start();
  else onboarding(root, start);

  if ('serviceWorker' in navigator && !isDevHost()) navigator.serviceWorker.register('sw.js').catch(() => {});
}

main();
