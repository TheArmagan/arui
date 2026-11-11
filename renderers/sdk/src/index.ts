// @arui/sdk: types and helpers for ARUI mods.
// At runtime inside ARUI, `@arui/sdk` resolves (via import map) to the copy bundled with the UI,
// so mods must mark it (and `svelte`) as external when bundling.

export * from './types';
export * from './protocol';
export * from './api';

import type { ModModule } from './api';

/** Identity helper for typed mod entries: `export default defineMod({ activate(ctx) {} })`. */
export function defineMod(mod: ModModule): ModModule {
	return mod;
}
