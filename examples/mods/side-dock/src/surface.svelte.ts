// Runes work in mod code too (this file is compiled by the mod's Svelte compiler and runs on the
// shell's shared runtime). The effect re-registers the surface whenever the edge setting changes,
// and the host moves the appbar to the new edge.

import type { AruiContext } from '@arui/sdk';
import Dock from './Dock.svelte';

export function registerDock(ctx: AruiContext) {
	const stop = $effect.root(() => {
		$effect(() => {
			const edge = ctx.settings.get<'left' | 'right'>('edge') ?? 'left';
			const dispose = ctx.surfaces.register({
				id: 'taskbar',
				component: Dock,
				window: { kind: 'appbar', edge, thickness: 76, perMonitor: true, reserve: true }
			});
			return dispose;
		});
	});
	ctx.onDispose(stop);
}
