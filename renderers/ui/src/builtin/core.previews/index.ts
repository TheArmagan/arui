// core.previews: window preview popup shown while hovering a taskbar group.
//
// Taskbar-like surfaces call the window-local commands 'core.previews:hover' / ':leave' with
// { appId, rect (screen px), monitor }; the preview window reports pointer enter/leave through
// the 'core.previews:pointer' broadcast so the popup stays open while the mouse is on it.

import { defineMod, type ModManifest, type Rect } from '@arui/sdk';
import Preview from './Preview.svelte';

export const manifest: ModManifest = {
	id: 'core.previews',
	name: 'Window previews',
	version: '0.1.0',
	apiVersion: 1,
	description: 'Live window thumbnails when hovering taskbar buttons.'
};

interface HoverArgs {
	appId: string;
	rect?: Rect;
	monitor?: string | null;
	immediate?: boolean;
}

export default defineMod({
	activate(ctx) {
		ctx.settings.define({
			title: 'Window previews',
			fields: [
				{ key: 'delay', label: 'Hover delay (ms)', type: 'number', default: 350, min: 0, max: 2000, step: 50 }
			]
		});
		ctx.surfaces.register({
			id: 'preview',
			component: Preview,
			window: {
				kind: 'popup',
				anchor: 'rect',
				width: 240,
				height: 186,
				offset: 10,
				closeOnBlur: false,
				focusable: false
			}
		});

		let openFor: string | null = null;
		let openTimer: ReturnType<typeof setTimeout> | undefined;
		let closeTimer: ReturnType<typeof setTimeout> | undefined;
		let pointerInside = false;

		const open = (a: HoverArgs) => {
			openFor = a.appId;
			void ctx.surfaces.open({
				id: 'preview',
				monitor: a.monitor ?? undefined,
				rect: a.rect,
				params: { appId: a.appId }
			});
		};
		const close = () => {
			clearTimeout(openTimer);
			clearTimeout(closeTimer);
			if (openFor) void ctx.surfaces.close('preview');
			openFor = null;
		};
		const scheduleClose = () => {
			clearTimeout(closeTimer);
			closeTimer = setTimeout(() => {
				if (!pointerInside) close();
			}, 300);
		};

		ctx.onDispose(
			ctx.commands.register('core.previews:hover', (a: HoverArgs) => {
				clearTimeout(closeTimer);
				clearTimeout(openTimer);
				if (openFor && openFor !== a.appId) return open(a);
				if (openFor === a.appId) return;
				openTimer = setTimeout(() => open(a), ctx.settings.get<number>('delay') ?? 350);
			})
		);
		ctx.onDispose(
			ctx.commands.register('core.previews:leave', (a: HoverArgs) => {
				clearTimeout(openTimer);
				if (a?.immediate) close();
				else scheduleClose();
			})
		);
		ctx.events.on<{ inside: boolean }>('core.previews:pointer', ({ inside }) => {
			pointerInside = inside;
			if (!openFor) return;
			if (inside) clearTimeout(closeTimer);
			else scheduleClose();
		});
		ctx.onDispose(close);
	}
});
