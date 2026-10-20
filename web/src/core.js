// @ts-check
// Loads the WebAssembly core built by scripts/build-wasm.sh into web/pkg/.
//
// On localhost only, if the core has not been built, a development stub with
// the same interface is used instead, a banner says so, and data goes to a
// separate database — so the stub can never touch a real identity.

const DEV_HOSTS = new Set(['localhost', '127.0.0.1', '[::1]']);

export const isDevHost = () => DEV_HOSTS.has(location.hostname);

/** @returns {Promise<{core: any, stub: boolean}>} */
export async function loadCore() {
  try {
    const m = await import('../pkg/pips_wasm.js');
    await m.default();
    return { core: m, stub: false };
  } catch (e) {
    if (!isDevHost()) throw e;
    console.warn('pips: core not built, using the development stub', e);
    const stub = await import('./stub-core.js');
    await stub.default();
    return { core: stub, stub: true };
  }
}
