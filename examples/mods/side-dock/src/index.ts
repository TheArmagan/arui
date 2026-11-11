// Side dock: registers a surface with the id 'taskbar'. The latest registration of an id wins,
// so while this mod is enabled every taskbar window renders the dock instead, and the host moves
// the appbar to the left edge. Disabling the mod brings the core taskbar back.

import { defineMod } from '@arui/sdk';
import { registerDock } from './surface.svelte';

export default defineMod({
	activate(ctx) {
		ctx.settings.define({
			title: 'Side dock',
			fields: [
				{
					key: 'edge',
					label: 'Edge',
					type: 'select',
					default: 'left',
					options: [
						{ value: 'left', label: 'Left' },
						{ value: 'right', label: 'Right' }
					]
				},
				{ key: 'labels', label: 'Show the focused app name', type: 'boolean', default: true }
			]
		});

		registerDock(ctx);
	}
});
