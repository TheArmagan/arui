// core.clock: date/time widget at the end of the taskbar and a calendar popup.

import { defineMod, type ModManifest } from '@arui/sdk';
import Calendar from './Calendar.svelte';
import Clock from './Clock.svelte';

export const manifest: ModManifest = {
	id: 'core.clock',
	name: 'Clock',
	version: '0.1.0',
	apiVersion: 1,
	description: 'Date and time in the taskbar; click for a calendar.'
};

export default defineMod({
	activate(ctx) {
		ctx.settings.define({
			title: 'Clock',
			fields: [
				{ key: 'showSeconds', label: 'Show seconds', type: 'boolean', default: true },
				{ key: 'hour12', label: '12-hour clock', type: 'boolean', default: true }
			]
		});
		ctx.widgets.register('taskbar.end', Clock, { id: 'core.clock:clock', order: 900 });
		ctx.surfaces.register({
			id: 'calendar',
			component: Calendar,
			window: { kind: 'popup', anchor: 'rect', width: 340, height: 480, offset: 10 }
		});
	}
});
