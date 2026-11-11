// Entry point. `?surface=<id>&monitor=<id>` selects what this window renders:
//   host -> hidden window that collects surface definitions and sends them to the shell
//   dev  -> browser-only fake desktop that hosts every surface in an iframe (mock backend)
//   any other id -> that surface, provided by whichever mod registered it last

import './app.css';
import { mount } from 'svelte';
import { getBackend, isTauri, readLocation } from './lib/backend';
import { registerSharedModules } from './lib/runtime/shared-modules';
import { Runtime } from './lib/runtime/runtime.svelte';
import { BUILTINS } from './builtin';
import { MockServer } from './lib/backend/mock/server';

async function boot() {
	const loc = readLocation();
	const target = document.getElementById('app')!;

	if (loc.surface === 'dev' && !isTauri()) {
		window.__ARUI_MOCK_SERVER__ = new MockServer();
		const { default: DevDesktop } = await import('./dev/DevDesktop.svelte');
		mount(DevDesktop, { target, props: { server: window.__ARUI_MOCK_SERVER__ } });
		return;
	}

	const backend = getBackend();
	// Before any mod is imported: the import map shims read from this registry.
	registerSharedModules((key) => backend.iconUrl(key));

	const runtime = new Runtime(backend, loc, BUILTINS);
	(window as unknown as { __ARUI__: Runtime }).__ARUI__ = runtime;
	await runtime.init();
	mount((await import('./App.svelte')).default, { target, props: { runtime } });
}

boot().catch((err) => {
	console.error('[arui] boot failed', err);
	document.body.innerHTML = `<pre style="color:#f88;padding:12px;font:12px monospace;white-space:pre-wrap">ARUI failed to start\n${String(err?.stack ?? err)}</pre>`;
});
