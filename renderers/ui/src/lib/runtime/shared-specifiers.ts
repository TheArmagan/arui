// Bare module specifiers that mods import but must NOT bundle. The UI exposes its own already
// loaded copies of these through an import map (see vite/arui-shims.ts and shared-modules.ts),
// so every mod runs on the exact same Svelte runtime instance as the core UI.
//
// Keep this list in sync with the loaders in shared-modules.ts (checked at startup).
export const SHARED_SPECIFIERS = [
	'svelte',
	'svelte/animate',
	'svelte/attachments',
	'svelte/easing',
	'svelte/events',
	'svelte/internal/client',
	'svelte/internal/disclose-version',
	'svelte/internal/flags/async',
	'svelte/internal/flags/legacy',
	'svelte/internal/flags/tracing',
	'svelte/legacy',
	'svelte/motion',
	'svelte/reactivity',
	'svelte/reactivity/window',
	'svelte/store',
	'svelte/transition',
	'@arui/sdk',
	'@arui/sdk/ui'
] as const;

export type SharedSpecifier = (typeof SHARED_SPECIFIERS)[number];

/** URL prefix the shim modules are served from (dev middleware and build output). */
export const SHIM_BASE = '/__arui/shims/';

export function shimFileName(spec: string): string {
	return spec.replace(/[^a-zA-Z0-9]+/g, '_').replace(/^_+|_+$/g, '') + '.js';
}

/** Name of the global registry the UI fills before any mod is imported. */
export const MODULE_REGISTRY_GLOBAL = '__ARUI_MODULES__';
