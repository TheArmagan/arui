// Glue between the shared UI components in @arui/sdk/ui and the ARUI runtime that hosts them.
// The runtime puts a UiHost into Svelte context at the root of every surface; widgets and
// surfaces also get their own mod's AruiContext through context (see getArui()).

import { getContext, hasContext } from 'svelte';
import type { AruiContext, ContextMenuTarget, ContextMenuTargets, MenuItem, WidgetEntry } from '../api';
import type { Rect } from '../types';

export interface SlotItem {
	entry: WidgetEntry;
	/** Context of the mod that registered the widget. */
	ctx: AruiContext;
}

export interface MenuPoint {
	clientX: number;
	clientY: number;
	screenX: number;
	screenY: number;
}

export interface UiHost {
	/** Visible widgets for a slot, sorted. Reactive. */
	slot(name: string): SlotItem[];
	/** Items contributed by ctx.contextMenus.extend() for a target. */
	menuItems<T extends ContextMenuTarget>(target: T, data: ContextMenuTargets[T]): MenuItem[];
	/**
	 * True when menus should open in a separate popup surface instead of inside this window
	 * (small windows like the taskbar would clip an in-page menu).
	 */
	preferPopupMenus(): boolean;
	/** Show items in the menu popup surface at a point. */
	popupMenu(items: MenuItem[], at: MenuPoint): void;
	/**
	 * Convert a rect in this window's CSS px (element.getBoundingClientRect()) to screen
	 * physical px, as expected by SurfaceOpenArgs.rect.
	 */
	screenRect(r: { left: number; top: number; right: number; bottom: number }): Rect;
}

/** Screen rect (physical px) of an element, for `ctx.surfaces.open({ rect })`. */
export function elementScreenRect(host: UiHost, el: Element): Rect {
	return host.screenRect(el.getBoundingClientRect());
}

export const ARUI_HOST_KEY = Symbol.for('arui.ui-host');
export const ARUI_CTX_KEY = Symbol.for('arui.mod-context');

export function getUiHost(): UiHost {
	const host = getContext<UiHost | undefined>(ARUI_HOST_KEY);
	if (!host) throw new Error('[arui] UI host missing: component rendered outside an ARUI surface');
	return host;
}

/** AruiContext of the mod whose widget/surface is being rendered. Call during component init. */
export function getArui(): AruiContext {
	if (!hasContext(ARUI_CTX_KEY)) throw new Error('[arui] getArui() called outside a mod component');
	return getContext<AruiContext>(ARUI_CTX_KEY);
}
