// core.settings: the 'settings' panel window.

import { defineMod, type ModManifest } from '@arui/sdk';
import SettingsPanel from './SettingsPanel.svelte';

export const manifest: ModManifest = {
	id: 'core.settings',
	name: 'Settings',
	version: '0.1.0',
	apiVersion: 1,
	description: 'Mods, themes and per-mod settings.'
};

export default defineMod({
	activate(ctx) {
		ctx.surfaces.register({
			id: 'settings',
			component: SettingsPanel,
			window: { kind: 'panel', anchor: 'center', width: 920, height: 640, closeOnBlur: false }
		});
	}
});
