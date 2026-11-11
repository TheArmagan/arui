// core.start-menu: the 'start-menu' popup. Opened by the start button and, in the host window,
// by the Windows key (`shell.startKey` local event raised from native://shell).

import { defineMod, type ModManifest } from '@arui/sdk';
import StartMenu from './StartMenu.svelte';

export const manifest: ModManifest = {
	id: 'core.start-menu',
	name: 'Start menu',
	version: '0.1.0',
	apiVersion: 1,
	description: 'App launcher with search, pinned apps and all apps by folder.'
};

export default defineMod({
	activate(ctx) {
		ctx.surfaces.register({
			id: 'start-menu',
			component: StartMenu,
			window: { kind: 'popup', anchor: 'bottomLeft', width: 600, height: 640, offset: 8 }
		});
		// Only one window may react to the key, otherwise every surface would toggle it.
		if (ctx.surface.id === 'host') {
			ctx.events.on('shell.startKey', () => void ctx.surfaces.toggle('start-menu'));
		}
	}
});
