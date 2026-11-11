// core.tray: notification area icons (inline + overflow flyout). Clicks are forwarded to the
// owning app through actions.trayAction with screen coordinates.

import { defineMod, type ModManifest } from '@arui/sdk';
import TrayArea from './TrayArea.svelte';
import TrayFlyout from './TrayFlyout.svelte';
import TrayInline from './TrayInline.svelte';

export const manifest: ModManifest = {
	id: 'core.tray',
	name: 'Tray',
	version: '0.1.0',
	apiVersion: 1,
	description: 'Notification area icons with an overflow flyout.'
};

export default defineMod({
	activate(ctx) {
		ctx.settings.define({
			title: 'Tray',
			fields: [
				{
					key: 'inlineCount',
					label: 'Icons shown on the taskbar',
					description: 'Used until you move icons yourself (shift + right click).',
					type: 'number',
					default: 3,
					min: 0,
					max: 12,
					step: 1
				}
			]
		});
		ctx.widgets.register('taskbar.end', TrayArea, { id: 'core.tray:area', order: 200 });
		ctx.widgets.register('tray.inline', TrayInline, { id: 'core.tray:inline', order: 100 });
		ctx.surfaces.register({
			id: 'tray-flyout',
			component: TrayFlyout,
			window: { kind: 'popup', anchor: 'rect', width: 172, height: 140, offset: 10 }
		});
	}
});
