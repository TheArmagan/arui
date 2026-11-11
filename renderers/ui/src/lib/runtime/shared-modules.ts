// Fills the global module registry read by the import-map shims (vite/arui-shims.ts).
// Every entry returns the namespace of the copy the UI itself uses, so mods share one Svelte
// runtime (same scheduler, same context, same signals) and one SDK instance.

import * as svelte from 'svelte';
import * as svelteAnimate from 'svelte/animate';
import * as svelteAttachments from 'svelte/attachments';
import * as svelteEasing from 'svelte/easing';
import * as svelteEvents from 'svelte/events';
import * as svelteInternalClient from 'svelte/internal/client';
import * as svelteLegacy from 'svelte/legacy';
import * as svelteMotion from 'svelte/motion';
import * as svelteReactivity from 'svelte/reactivity';
import * as svelteReactivityWindow from 'svelte/reactivity/window';
import * as svelteStore from 'svelte/store';
import * as svelteTransition from 'svelte/transition';
import * as sdk from '@arui/sdk';
import * as sdkUi from '@arui/sdk/ui';
import { MODULE_REGISTRY_GLOBAL, SHARED_SPECIFIERS, type SharedSpecifier } from './shared-specifiers';

type Loader = () => Promise<Record<string, unknown>>;

const now = (ns: object): Loader => () => Promise.resolve(ns as Record<string, unknown>);

export function registerSharedModules(iconUrl: (key: string | null | undefined) => string) {
	const loaders: Record<SharedSpecifier, Loader> = {
		svelte: now(svelte),
		'svelte/animate': now(svelteAnimate),
		'svelte/attachments': now(svelteAttachments),
		'svelte/easing': now(svelteEasing),
		'svelte/events': now(svelteEvents),
		'svelte/internal/client': now(svelteInternalClient),
		// Side-effect modules: only evaluated when a mod actually needs them (for example the
		// legacy flag, which a mod compiled in non-runes mode imports).
		'svelte/internal/disclose-version': () => import('svelte/internal/disclose-version'),
		'svelte/internal/flags/async': () => import('svelte/internal/flags/async'),
		'svelte/internal/flags/legacy': () => import('svelte/internal/flags/legacy'),
		'svelte/internal/flags/tracing': () => import('svelte/internal/flags/tracing'),
		'svelte/legacy': now(svelteLegacy),
		'svelte/motion': now(svelteMotion),
		'svelte/reactivity': now(svelteReactivity),
		'svelte/reactivity/window': now(svelteReactivityWindow),
		'svelte/store': now(svelteStore),
		'svelte/transition': now(svelteTransition),
		// iconUrl is swapped for the backend-aware one so mods calling it directly also work in
		// the browser mock.
		'@arui/sdk': now({ ...sdk, iconUrl }),
		'@arui/sdk/ui': now(sdkUi)
	};
	for (const spec of SHARED_SPECIFIERS) {
		if (!loaders[spec]) console.error(`[arui] no shared module loader for ${spec}`);
	}
	(globalThis as Record<string, unknown>)[MODULE_REGISTRY_GLOBAL] = loaders;
}
