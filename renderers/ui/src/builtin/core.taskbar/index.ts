// core.taskbar: the 'taskbar' appbar surface plus its start button and app group widgets.

import { defineMod, type ModManifest } from '@arui/sdk';
import AppGroups from './AppGroups.svelte';
import StartButton from './StartButton.svelte';
import Taskbar from './Taskbar.svelte';

export const manifest: ModManifest = {
	id: 'core.taskbar',
	name: 'Taskbar',
	version: '0.1.0',
	apiVersion: 1,
	description: 'Bottom taskbar with pinned and running apps, grouped by app.'
};

export default defineMod({
	activate(ctx) {
		ctx.surfaces.register({
			id: 'taskbar',
			component: Taskbar,
			window: { kind: 'appbar', edge: 'bottom', thickness: 72, perMonitor: true, reserve: true }
		});
		ctx.widgets.register('taskbar.start', StartButton, { id: 'core.taskbar:start', order: 100 });
		ctx.widgets.register('taskbar.center', AppGroups, { id: 'core.taskbar:groups', order: 100 });
	}
});
