// core.quick-settings: volume and network status in the taskbar and a quick settings popup.

import { defineMod, type ModManifest } from '@arui/sdk';
import NetworkTile from './NetworkTile.svelte';
import QuickButton from './QuickButton.svelte';
import QuickPanel from './QuickPanel.svelte';
import VolumeTile from './VolumeTile.svelte';

export const manifest: ModManifest = {
	id: 'core.quick-settings',
	name: 'Quick settings',
	version: '0.1.0',
	apiVersion: 1,
	description: 'Volume slider, mute and network status.'
};

export default defineMod({
	activate(ctx) {
		ctx.widgets.register('taskbar.end', QuickButton, { id: 'core.quick-settings:button', order: 300 });
		ctx.widgets.register('quick.settings', VolumeTile, { id: 'core.quick-settings:volume', order: 100 });
		ctx.widgets.register('quick.settings', NetworkTile, { id: 'core.quick-settings:network', order: 200 });
		ctx.surfaces.register({
			id: 'quick-settings',
			component: QuickPanel,
			window: { kind: 'popup', anchor: 'rect', width: 340, height: 200, offset: 10 }
		});
	}
});
