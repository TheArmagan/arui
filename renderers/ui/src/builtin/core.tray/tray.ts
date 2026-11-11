// Shared tray helpers: which icons are inline, click forwarding with screen coordinates.

import type { AruiContext, MenuItem, TrayAction, TrayIcon } from '@arui/sdk';
import type { UiHost } from '@arui/sdk/ui';
import Eye from '@lucide/svelte/icons/eye';
import EyeOff from '@lucide/svelte/icons/eye-off';

export function inlineIds(ctx: AruiContext): string[] | null {
	return ctx.settings.get<string[] | null>('inline') ?? null;
}

export function isInline(ctx: AruiContext, icon: TrayIcon, index: number): boolean {
	const ids = inlineIds(ctx);
	if (ids) return ids.includes(icon.id);
	return index < (ctx.settings.get<number>('inlineCount') ?? 3);
}

function setInline(ctx: AruiContext, icon: TrayIcon, inline: boolean) {
	const visible = ctx.state.tray.filter((t) => t.visible);
	const current = visible.filter((t, i) => isInline(ctx, t, i)).map((t) => t.id);
	const next = inline ? [...new Set([...current, icon.id])] : current.filter((id) => id !== icon.id);
	void ctx.settings.set('inline', next);
}

export function forward(ctx: AruiContext, icon: TrayIcon, action: TrayAction, e: MouseEvent) {
	// Apps place their own menus at these coordinates, so they must be physical screen px.
	const s = window.devicePixelRatio || 1;
	void ctx.actions.trayAction(icon.id, action, Math.round(e.screenX * s), Math.round(e.screenY * s));
}

/** Mouse handlers for a tray icon button. Shift + right click opens ARUI's own menu. */
export function trayHandlers(ctx: AruiContext, host: UiHost, icon: TrayIcon, inline: boolean) {
	return {
		onclick: (e: MouseEvent) => forward(ctx, icon, 'leftClick', e),
		ondblclick: (e: MouseEvent) => forward(ctx, icon, 'doubleClick', e),
		onauxclick: (e: MouseEvent) => {
			if (e.button === 1) forward(ctx, icon, 'middleClick', e);
		},
		oncontextmenu: (e: MouseEvent) => {
			// Stop here so the taskbar's own context menu does not open on top of the app's menu.
			e.preventDefault();
			e.stopPropagation();
			if (!e.shiftKey) return forward(ctx, icon, 'rightClick', e);
			const items: MenuItem[] = [
				inline
					? { id: 'hide', label: 'Move to overflow', icon: EyeOff, onSelect: () => setInline(ctx, icon, false) }
					: { id: 'show', label: 'Show in taskbar', icon: Eye, onSelect: () => setInline(ctx, icon, true) },
				...host.menuItems('tray.icon', { icon })
			];
			host.popupMenu(items, e);
		},
		onmouseenter: (e: MouseEvent) => forward(ctx, icon, 'hoverEnter', e),
		onmouseleave: (e: MouseEvent) => forward(ctx, icon, 'hoverLeave', e)
	};
}
