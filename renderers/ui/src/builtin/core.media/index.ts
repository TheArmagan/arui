// core.media: now-playing card in the right side of the taskbar.

import { defineMod, type ModManifest } from '@arui/sdk';
import MediaCard from './MediaCard.svelte';

export const manifest: ModManifest = {
	id: 'core.media',
	name: 'Media',
	version: '0.1.0',
	apiVersion: 1,
	description: 'Current media session with artwork and playback controls.'
};

export default defineMod({
	activate(ctx) {
		ctx.settings.define({
			title: 'Media',
			fields: [
				{
					key: 'hideWhenIdle',
					label: 'Hide when nothing is playing',
					type: 'boolean',
					default: false
				}
			]
		});
		ctx.widgets.register('taskbar.right', MediaCard, { id: 'core.media:card', order: 500 });
	}
});
