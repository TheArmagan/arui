// core.menus: the 'menu' popup surface used for context menus opened from small windows.

import { defineMod, type ModManifest } from '@arui/sdk';
import MenuSurface from './MenuSurface.svelte';

export const manifest: ModManifest = {
	id: 'core.menus',
	name: 'Context menus',
	version: '0.1.0',
	apiVersion: 1,
	description: 'Popup window that shows context menus for the taskbar and other small surfaces.'
};

export default defineMod({
	activate(ctx) {
		ctx.surfaces.register({
			id: 'menu',
			component: MenuSurface,
			window: { kind: 'popup', anchor: 'cursor', width: 272, height: 240, closeOnBlur: true }
		});
	}
});
