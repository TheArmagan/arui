// Core context menu items for a taskbar group and helpers shared by the taskbar widgets.

import type { AppGroup, AruiContext, MenuItem, PinnedItem } from '@arui/sdk';
import Pin from '@lucide/svelte/icons/pin';
import PinOff from '@lucide/svelte/icons/pin-off';
import SquareArrowOutUpRight from '@lucide/svelte/icons/square-arrow-out-up-right';
import X from '@lucide/svelte/icons/x';

export function pinFor(group: AppGroup): PinnedItem {
	return group.app
		? {
				id: group.app.id,
				appId: group.appId,
				name: group.name,
				iconKey: group.iconKey,
				launch: { kind: 'app', id: group.app.id }
			}
		: {
				id: `exe:${group.exePath}`,
				appId: group.appId,
				name: group.name,
				iconKey: group.iconKey,
				launch: { kind: 'exe', path: group.exePath ?? '' }
			};
}

export function launchNew(ctx: AruiContext, group: AppGroup) {
	const pin = ctx.state.pins.find((p) => p.appId.toLowerCase() === group.appId.toLowerCase());
	if (pin?.launch.kind === 'app') return ctx.actions.launchApp(pin.launch.id);
	if (pin?.launch.kind === 'exe') return ctx.actions.shellExecute(pin.launch.path);
	if (group.app) return ctx.actions.launchApp(group.app.id);
	if (group.exePath) return ctx.actions.shellExecute(group.exePath);
}

export function groupMenu(ctx: AruiContext, group: AppGroup): MenuItem[] {
	const items: MenuItem[] = [
		{
			id: 'launch',
			label: group.windows.length ? `New ${group.name} window` : `Open ${group.name}`,
			icon: SquareArrowOutUpRight,
			onSelect: () => void launchNew(ctx, group)
		},
		group.pinned
			? {
					id: 'unpin',
					label: 'Unpin from taskbar',
					icon: PinOff,
					onSelect: () => ctx.actions.unpin(group.appId)
				}
			: {
					id: 'pin',
					label: 'Pin to taskbar',
					icon: Pin,
					onSelect: () => ctx.actions.pin(pinFor(group))
				}
	];
	if (group.windows.length) {
		items.push({
			id: 'close-all',
			label: group.windows.length > 1 ? `Close all ${group.windows.length} windows` : 'Close window',
			icon: X,
			separatorBefore: true,
			onSelect: () => group.windows.forEach((w) => void ctx.actions.closeWindow(w.hwnd))
		});
	}
	return items;
}
